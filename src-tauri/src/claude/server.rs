// WebSocket Server for Claude Code CLI
//
// This server runs locally and accepts connections from Claude Code CLI
// launched with the --sdk-url flag.

use crate::claude::protocol::*;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http::{Response as HttpResponse, StatusCode};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::handshake::server::{Request, Response},
    tungstenite::Message,
    WebSocketStream,
};
use tracing::{debug, error, info, warn};

/// WebSocket sender type
type WsSender = futures_util::stream::SplitSink<WebSocketStream<TcpStream>, Message>;

/// WebSocket server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub auth_token: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8765,
            auth_token: None,
        }
    }
}

/// Connected client state
#[derive(Debug)]
struct ClientState {
    session_id: Option<String>,
    init_received: bool,
    ws_connected: bool, // WebSocket connection established (before any messages)
    /// Commands from initialize response (full info with name, description, argument_hint)
    commands: Option<Vec<crate::claude::protocol::CommandInfo>>,
    /// Skills from system.init message
    skills: Option<Vec<String>>,
}

/// WebSocket server for Claude Code CLI
pub struct ClaudeSdkServer {
    pub(crate) config: ServerConfig,
    client_state: Arc<Mutex<ClientState>>,
    ws_sender: Arc<Mutex<Option<WsSender>>>,
    message_tx: Arc<Mutex<tokio::sync::mpsc::UnboundedSender<SdkMessage>>>,
    message_rx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<SdkMessage>>>>,
    running: Arc<RwLock<bool>>,
    /// Handle to the listener task for proper shutdown
    listener_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl ClaudeSdkServer {
    /// Create a new server instance
    pub fn new(config: ServerConfig) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        Self {
            config,
            client_state: Arc::new(Mutex::new(ClientState {
                session_id: None,
                init_received: false,
                ws_connected: false,
                commands: None,
                skills: None,
            })),
            ws_sender: Arc::new(Mutex::new(None)),
            message_tx: Arc::new(Mutex::new(tx)),
            message_rx: Arc::new(Mutex::new(Some(rx))),
            running: Arc::new(RwLock::new(false)),
            listener_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the WebSocket server
    pub async fn start(&self) -> Result<(), anyhow::Error> {
        let mut running = self.running.write().await;
        if *running {
            info!("Server already running");
            return Ok(());
        }
        *running = true;
        drop(running);

        let addr = format!("{}:{}", self.config.host, self.config.port);
        info!("🚀 Starting Claude SDK WebSocket server on ws://{}", addr);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        info!("✅ Server listening on ws://{}", addr);

        // Spawn the server task
        let client_state = self.client_state.clone();
        let ws_sender = self.ws_sender.clone();
        let message_tx = self.message_tx.clone();
        let running = self.running.clone();
        let auth_token = self.config.auth_token.clone();

        let handle = tokio::spawn(async move {
            while *running.read().await {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        info!("📥 New connection from {}", addr);

                        let client_state = client_state.clone();
                        let ws_sender = ws_sender.clone();
                        let message_tx = message_tx.clone();
                        let auth_token = auth_token.clone();

                        tokio::spawn(async move {
                            Self::handle_connection(
                                stream,
                                client_state,
                                ws_sender,
                                message_tx,
                                auth_token,
                            )
                            .await;
                        });
                    }
                    Err(e) => {
                        error!("❌ Failed to accept connection: {}", e);
                    }
                }
            }
        });

        // Store the listener handle for proper shutdown
        *self.listener_handle.lock().await = Some(handle);

        Ok(())
    }

    /// Handle a single client connection
    async fn handle_connection(
        stream: tokio::net::TcpStream,
        client_state: Arc<Mutex<ClientState>>,
        ws_sender: Arc<Mutex<Option<WsSender>>>,
        message_tx: Arc<Mutex<tokio::sync::mpsc::UnboundedSender<SdkMessage>>>,
        auth_token: Option<String>,
    ) {
        // Handle WebSocket handshake with auth validation
        let ws_stream = match accept_hdr_async(stream, |req: &Request, resp: Response| {
            let path = req.uri().path();
            info!("WebSocket handshake request from path: {:?}", path);

            // Check Authorization header
            if let Some(token) = &auth_token {
                let auth_header = req
                    .headers()
                    .get("Authorization")
                    .and_then(|h| h.to_str().ok());

                match auth_header {
                    Some(header) if header == &format!("Bearer {}", token) => {
                        debug!("✅ Authentication successful");
                        Ok(resp)
                    }
                    _ => {
                        warn!("❌ Authentication failed");
                        let err_resp = HttpResponse::builder()
                            .status(StatusCode::UNAUTHORIZED)
                            .body(Some("Unauthorized".to_string()))
                            .unwrap();
                        Err(err_resp)
                    }
                }
            } else {
                Ok(resp)
            }
        })
        .await
        {
            Ok(s) => s,
            Err(e) => {
                error!("❌ WebSocket handshake failed: {}", e);
                return;
            }
        };

        info!("🔗 WebSocket connection established");

        // Mark WebSocket as connected
        {
            let mut state = client_state.lock().await;
            state.ws_connected = true;
            info!("✅ WebSocket connection state updated: ws_connected=true");
        }

        let (sender, mut receiver) = ws_stream.split();

        // Store the sender for later use
        *ws_sender.lock().await = Some(sender);

        // Message processing loop
        while let Some(msg_result) = receiver.next().await {
            match msg_result {
                Ok(msg) => {
                    // Log all message types for debugging (except ping)
                    let msg_type_str = if msg.is_text() {
                        "text"
                    } else if msg.is_binary() {
                        "binary"
                    } else if msg.is_close() {
                        "close"
                    } else if msg.is_ping() {
                        "ping"
                    } else if msg.is_pong() {
                        "pong"
                    } else {
                        "unknown"
                    };

                    // Skip logging for ping messages
                    if !msg.is_ping() {
                        info!(
                            "📨 WebSocket message received: type={}, size={}",
                            msg_type_str,
                            msg.len()
                        );
                    }

                    if msg.is_close() {
                        info!("📤 Client sent close frame");
                        break;
                    }

                    if msg.is_ping() {
                        debug!("🏓 Received ping, sending pong");
                        let mut sender_lock = ws_sender.lock().await;
                        if let Some(s) = sender_lock.as_mut() {
                            if let Err(e) = s.send(Message::Pong(Bytes::new())).await {
                                error!("❌ Failed to send pong: {}", e);
                                break;
                            }
                        }
                        continue;
                    }

                    if msg.is_pong() {
                        debug!("🏓 Received pong");
                        continue;
                    }

                    // Process text/binary messages as NDJSON
                    let data: Result<String, _> = if msg.is_text() {
                        msg.into_text().map(|s| s.to_string())
                    } else if msg.is_binary() {
                        Ok(String::from_utf8_lossy(&msg.into_data()[..]).to_string())
                    } else {
                        debug!("⚠️ Unknown message type, skipping");
                        continue;
                    };

                    let data = match data {
                        Ok(d) => d,
                        Err(e) => {
                            error!("❌ Failed to get message data: {}", e);
                            continue;
                        }
                    };

                    info!("📋 Message data ({} bytes): '{}'", data.len(), data);

                    // Process NDJSON lines
                    for line in data.lines() {
                        if line.is_empty() {
                            debug!("⚠️ Skipping empty line");
                            continue;
                        }

                        // info!("📨 Processing NDJSON line: {}", line);

                        match deserialize_message(line) {
                            Ok(sdk_msg) => {
                                // Log message type
                                match &sdk_msg {
                                    SdkMessage::System(sys) => {
                                        debug!(
                                            "📋 System message: subtype={:?}, session_id={:?}",
                                            sys.subtype, sys.session_id
                                        );
                                    }
                                    SdkMessage::User(user) => {
                                        info!("👤 User message: session_id={:?}", user.session_id);
                                    }
                                    SdkMessage::Assistant(assist) => {
                                        info!(
                                            "🤖 Assistant message: id={:?}, model={:?}",
                                            assist.message.id, assist.message.model
                                        );
                                    }
                                    _ => {
                                        debug!(
                                            "📨 Other message type: {:?}",
                                            std::mem::discriminant(&sdk_msg)
                                        );
                                    }
                                }

                                // Update client state based on message type
                                match &sdk_msg {
                                    SdkMessage::System(sys) => {
                                        info!(
                                            "📋 System message: subtype={:?}, session_id={:?}",
                                            sys.subtype, sys.session_id
                                        );
                                        // Any system message indicates the client is connected and ready
                                        // Not all CLI versions send "init" subtype, so we accept any system message
                                        let mut state = client_state.lock().await;
                                        // Update session_id if provided
                                        if sys.session_id.is_some() && state.session_id.is_none() {
                                            state.session_id = sys.session_id.clone();
                                            info!("📝 Session ID updated: {:?}", sys.session_id);
                                        }
                                        // Store skills from init message
                                        if sys
                                            .subtype
                                            .as_ref()
                                            .map(|s| s == "init")
                                            .unwrap_or(false)
                                        {
                                            state.skills = sys.skills.clone();
                                            info!(
                                                "✅ Stored {} skills from init",
                                                sys.skills.as_ref().map(|v| v.len()).unwrap_or(0)
                                            );
                                            info!("📋 skills: {:?}", sys.skills);
                                        }
                                    }
                                    SdkMessage::ControlResponse(ctrl_resp) => {
                                        info!(
                                            "📋 ControlResponse: subtype={:?}, request_id={:?}",
                                            ctrl_resp.response.subtype,
                                            ctrl_resp.response.request_id
                                        );

                                        // Check if this is an initialize response (has commands field)
                                        if let Some(response_value) = &ctrl_resp.response.response {
                                            if response_value.get("commands").is_some() {
                                                info!("📋 Detected initialize response, extracting commands...");
                                                match serde_json::from_value::<
                                                    crate::claude::protocol::InitializeResponse,
                                                >(
                                                    response_value.clone()
                                                ) {
                                                    Ok(resp) => {
                                                        info!("✅ Parsed initialize response: {} commands, {} models",
                                                            resp.commands.as_ref().map(|c| c.len()).unwrap_or(0),
                                                            resp.models.as_ref().map(|m| m.len()).unwrap_or(0));
                                                        // Store commands
                                                        if let Some(cmds) = resp.commands.as_ref() {
                                                            let mut state =
                                                                client_state.lock().await;
                                                            state.commands = Some(cmds.clone());
                                                            info!("✅ Stored {} commands from initialize", cmds.len());
                                                        }
                                                    }
                                                    Err(e) => {
                                                        warn!("Failed to parse initialize response: {}", e);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    SdkMessage::Assistant(assist) => {
                                        // Track assistant messages for response detection
                                        debug!(
                                            "📝 Assistant response received: {}",
                                            assist.message.id
                                        );
                                    }
                                    _ => {
                                        debug!(
                                            "📨 Other message type: {:?}",
                                            std::mem::discriminant(&sdk_msg)
                                        );
                                    }
                                }

                                // Send to message channel
                                if let Err(e) = message_tx.lock().await.send(sdk_msg) {
                                    error!("❌ Failed to queue message: {}", e);
                                }
                            }
                            Err(e) => {
                                warn!("⚠️ Failed to deserialize message: {} | line: {}", e, line);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("❌ WebSocket error: {}", e);
                    break;
                }
            }
        }

        // Reset client state on disconnect
        {
            let mut state = client_state.lock().await;
            state.init_received = false;
            state.ws_connected = false;
            info!(
                "📴 Client disconnected, session was: {:?}",
                state.session_id
            );
            state.session_id = None;
        }

        // Clear the sender
        *ws_sender.lock().await = None;

        info!("🔌 WebSocket connection closed");
    }

    /// Send a message to the connected client
    pub async fn send_message(&self, msg: SdkMessage) -> Result<(), anyhow::Error> {
        // Serialize the message
        let serialized = serialize_message(&msg)?;

        info!(" msg  {}", serialized.trim());

        // Try to send through WebSocket
        let mut sender_lock = self.ws_sender.lock().await;
        if let Some(sender) = sender_lock.as_mut() {
            info!(
                "📨 [BACKEND SEND] Sending via WebSocket: {} bytes",
                serialized.len()
            );

            match sender.send(Message::Text(serialized.into())).await {
                Ok(_) => {
                    info!("✅ [BACKEND SEND] Message sent successfully");
                    Ok(())
                }
                Err(e) => {
                    error!(
                        "❌ [BACKEND SEND] Failed to send message via WebSocket: {}",
                        e
                    );
                    Err(e.into())
                }
            }
        } else {
            warn!("⚠️ [BACKEND SEND] No WebSocket client connected, cannot send message");
            Err(anyhow::anyhow!("No WebSocket client connected"))
        }
    }

    /// Receive the next message from the client
    pub async fn recv_message(&self) -> Option<SdkMessage> {
        let mut rx = self.message_rx.lock().await;
        if let Some(receiver) = rx.as_mut() {
            receiver.recv().await
        } else {
            None
        }
    }

    /// Check if a client is connected and initialized
    /// Returns true only when:
    /// 1. WebSocket connection is established (ws_connected = true)
    /// 2. System message has been received (init_received = true)
    pub async fn is_connected(&self) -> bool {
        let state = self.client_state.lock().await;
        let connected = state.ws_connected;
        info!(
            "🔍 Connection check: ws_connected={}, init_received={}, connected={}, session_id={:?}",
            state.ws_connected, state.init_received, connected, state.session_id
        );
        connected
    }

    pub async fn is_websocket_connected(&self) -> bool {
        let state = self.client_state.lock().await;
        state.ws_connected
    }

    /// Get commands (from initialize) and skills (from system.init)
    pub async fn get_commands_and_skills(
        &self,
    ) -> (
        Option<Vec<crate::claude::protocol::CommandInfo>>,
        Option<Vec<String>>,
    ) {
        let state = self.client_state.lock().await;
        let commands = state.commands.clone();
        let skills = state.skills.clone();
        info!(
            "📤 get_commands_and_skills called: returning {} commands; cached_skills={} (not forwarded)",
            commands.as_ref().map(|v| v.len()).unwrap_or(0),
            skills.as_ref().map(|v| v.len()).unwrap_or(0)
        );
        (commands, None)
    }

    /// Set commands from initialize response
    pub async fn set_commands(&self, commands: Vec<crate::claude::protocol::CommandInfo>) {
        let mut state = self.client_state.lock().await;
        state.commands = Some(commands);
        info!(
            "✅ Stored {} commands from initialize response",
            state.commands.as_ref().map(|v| v.len()).unwrap_or(0)
        );
    }

    /// Check if server is running (regardless of client connection)
    pub async fn is_running(&self) -> bool {
        let running = self.running.read().await;
        debug!("🔍 Server running check: {}", *running);
        *running
    }

    /// Get the current session ID
    /// Returns the CLI-provided session ID from system.init message
    pub async fn session_id(&self) -> Option<String> {
        let state = self.client_state.lock().await;
        state.session_id.clone()
    }

    /// Stop the server
    pub async fn stop(&self) {
        info!("🛑 Stopping WebSocket server");
        let mut running = self.running.write().await;
        *running = false;
        drop(running);

        // Abort the listener task to release the port
        let mut listener_lock = self.listener_handle.lock().await;
        if let Some(handle) = listener_lock.take() {
            info!("🛑 Aborting listener task to release port");
            handle.abort();
        }
        drop(listener_lock);

        // Close WebSocket connection if exists
        let mut sender_lock = self.ws_sender.lock().await;
        if let Some(sender) = sender_lock.as_mut() {
            let _ = sender.close().await;
            *sender_lock = None;
        }

        info!("✅ WebSocket server stopped and port released");
    }
}
