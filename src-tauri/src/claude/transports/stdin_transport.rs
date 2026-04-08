// Stdin/Stdout Transport Implementation
//
// This module implements communication with Claude Code CLI via stdin/stdout
// using the NDJSON (newline-delimited JSON) protocol.

use crate::claude::protocol::{deserialize_message, serialize_message, SdkMessage};
use crate::claude::transport::{ClaudeTransport, TransportError};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

/// Stdin/Stdout transport for communicating with Claude CLI
pub struct StdinTransport {
    /// Sender for writing to CLI's stdin
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    /// Receiver channel for reading messages from CLI's stdout
    message_rx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<SdkMessage>>>>,
    /// Connected state
    connected: Arc<RwLock<bool>>,
    /// CLI's internal session ID (from system.init)
    session_id: Arc<Mutex<Option<String>>>,
    /// Handle to the reader task
    reader_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl StdinTransport {
    fn should_skip_stdout_log(msg: &SdkMessage) -> bool {
        matches!(
            msg,
            SdkMessage::StreamEvent(stream_event)
                if stream_event
                    .event
                    .get("type")
                    .and_then(|value| value.as_str())
                    == Some("content_block_delta")
        )
    }

    /// Create a new stdin/stdout transport
    pub fn new() -> Self {
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();

        Self {
            stdin: Arc::new(Mutex::new(None)),
            message_rx: Arc::new(Mutex::new(Some(rx))),
            connected: Arc::new(RwLock::new(false)),
            session_id: Arc::new(Mutex::new(None)),
            reader_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the stdin/stdout handles and start reading
    pub async fn attach(&self, stdin: ChildStdin, stdout: ChildStdout) {
        info!("🔗 Attaching stdin/stdout handles");

        // Store stdin handle
        *self.stdin.lock().await = Some(stdin);

        // Start reader task
        let message_tx = {
            // Create a new channel for this connection
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            *self.message_rx.lock().await = Some(rx);
            tx
        };

        let session_id = self.session_id.clone();
        let connected = self.connected.clone();

        let handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            loop {
                line.clear();

                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        // EOF - CLI closed stdout
                        info!("📴 CLI stdout closed (EOF)");
                        *connected.write().await = false;
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        // Parse NDJSON message
                        match deserialize_message(trimmed) {
                            Ok(msg) => {
                                match &msg {
                                    SdkMessage::ControlResponse(control_response) => {
                                        // ControlResponse 只打印简略信息
                                        info!(
                                            "📥 [ControlResponse] subtype={:?} request_id={}",
                                            control_response.response.subtype,
                                            control_response.response.request_id
                                        );
                                    }
                                    SdkMessage::System(system_message) => {
                                        if let Some(summary) = system_message.display_text() {
                                            info!(
                                                "📥 [System:{}] {}",
                                                system_message
                                                    .subtype
                                                    .as_deref()
                                                    .unwrap_or("unknown"),
                                                summary
                                            );
                                        } else {
                                            info!(
                                                "📥 STDOUT (received {} bytes): {}",
                                                trimmed.len(),
                                                trimmed
                                            );
                                        }
                                    }
                                    _ => {
                                        if !Self::should_skip_stdout_log(&msg) {
                                            // 正常消息打印完整内容
                                            info!(
                                                "📥 STDOUT (received {} bytes): {}",
                                                trimmed.len(),
                                                trimmed
                                            );
                                        }
                                    }
                                }

                                // Update session_id from system messages
                                if let SdkMessage::System(sys) = &msg {
                                    if let Some(sid) = &sys.session_id {
                                        *session_id.lock().await = Some(sid.clone());
                                        info!("📋 [Transport] Session ID updated: {}", sid);
                                    }
                                }

                                // Send to message channel
                                if let Err(e) = message_tx.send(msg) {
                                    error!("❌ Failed to queue message: {}", e);
                                }
                            }
                            Err(e) => {
                                warn!(
                                    "⚠️ Failed to deserialize message: {} | line: {}",
                                    e, trimmed
                                );
                            }
                        }
                    }
                    Err(e) => {
                        error!("❌ Error reading from stdout: {}", e);
                        *connected.write().await = false;
                        break;
                    }
                }
            }

            info!("🔌 Stdin/Stdout reader task ended");
        });

        *self.reader_handle.lock().await = Some(handle);
        *self.connected.write().await = true;

        info!("✅ Stdin/Stdout transport attached and reader started");
    }

    /// Detach and stop the reader
    pub async fn detach(&self) {
        info!("🔌 Detaching stdin/stdout handles");

        // Stop reader task
        let mut handle_lock = self.reader_handle.lock().await;
        if let Some(handle) = handle_lock.take() {
            handle.abort();
        }
        drop(handle_lock);

        // Close stdin
        let mut stdin_lock = self.stdin.lock().await;
        *stdin_lock = None;

        *self.connected.write().await = false;

        info!("✅ Stdin/Stdout transport detached");
    }
}

impl Default for StdinTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ClaudeTransport for StdinTransport {
    async fn start(&self) -> Result<(), TransportError> {
        // Stdin transport doesn't need to "start" - it's passive
        // It will be activated when attach() is called
        debug!("StdinTransport start called (no-op)");
        Ok(())
    }

    async fn stop(&self) -> Result<(), TransportError> {
        self.detach().await;
        Ok(())
    }

    async fn send_message(&self, msg: SdkMessage) -> Result<(), TransportError> {
        let serialized =
            serialize_message(&msg).map_err(|e| TransportError::SendFailed(e.to_string()))?;

        // Print sent stdin data (with truncation for long messages)
        info!(
            "📤 STDIN (sending {} bytes): {}",
            serialized.len(),
            serialized.trim()
        );

        let mut stdin_lock = self.stdin.lock().await;
        if let Some(stdin) = stdin_lock.as_mut() {
            use tokio::io::AsyncWriteExt;
            stdin
                .write_all(serialized.as_bytes())
                .await
                .map_err(|e| TransportError::SendFailed(e.to_string()))?;
            stdin
                .flush()
                .await
                .map_err(|e| TransportError::SendFailed(e.to_string()))?;
            info!("✅ Message sent via stdin");
            Ok(())
        } else {
            Err(TransportError::ConnectionFailed(
                "No stdin handle available".to_string(),
            ))
        }
    }

    async fn recv_message(&self) -> Option<SdkMessage> {
        let mut rx = self.message_rx.lock().await;
        if let Some(receiver) = rx.as_mut() {
            receiver.recv().await
        } else {
            None
        }
    }

    async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }

    async fn session_id(&self) -> Option<String> {
        self.session_id.lock().await.clone()
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SdkMessage> {
        // Note: We use unbounded mpsc, not broadcast
        // This is a limitation - for now we create a dummy receiver
        let (_tx, rx) = tokio::sync::broadcast::channel(1);
        rx
    }
}
