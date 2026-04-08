// Claude Session Manager
//
// This module manages sessions with Claude Code CLI.

use super::client::ClaudeClient;
use super::permission_rules::{build_allow_rule, matches_allow_rule};
use super::protocol::*;
use super::server::{ClaudeSdkServer, ServerConfig};
use crate::claude::message_normalizer::{normalize_cli_message, NormalizedMessageKind};
use crate::models::{Message, MessageContent, MessageRole, SdkStatus, Session, TokenUsage};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Callback type for streaming data chunks (deprecated, kept for compatibility)
pub type StreamDataCallback = Box<dyn Fn(String) + Send + Sync>;
/// Callback type for stream completion
pub type StreamCompleteCallback = Box<dyn Fn(Message) + Send + Sync>;
/// Callback type for streaming messages (new - supports multiple message types)
pub type StreamMessageCallback = Box<dyn Fn(Message) + Send + Sync>;

/// Pending permission request waiting for user decision
pub struct PendingPermissionRequest {
    pub tool_name: String,
    pub input: serde_json::Value,
    pub tool_use_id: String,
    pub request_id: String,
    pub description: Option<String>,
    pub blocked_path: Option<String>,
    pub decision_reason: Option<String>,
    pub tx: oneshot::Sender<PermissionDecision>,
}

/// Permission decision from user
pub enum PermissionDecision {
    Approve {
        updated_input: Option<serde_json::Value>,
        allow_tools: Option<Vec<String>>, // 允许的工具列表
    },
    Reject {
        reason: String,
        suggestion: Option<String>, // 给 Claude 的建议
    },
    /// 切换权限模式
    ChangeMode { mode: PermissionMode },
}

/// Session manager for Claude Code CLI
#[derive(Clone)]
pub struct ClaudeSessionManager {
    pub(crate) server: Arc<ClaudeSdkServer>,
    pub(crate) client: Arc<ClaudeClient>,
    /// External session ID (assigned by SessionRegistry)
    external_session_id: Arc<Mutex<Option<String>>>,
    current_session: Arc<Mutex<Option<Session>>>,
    message_queue: Arc<Mutex<Vec<Message>>>,
    /// Active streaming session IDs
    active_streams: Arc<RwLock<std::collections::HashSet<String>>>,
    /// Working directory for Claude CLI
    working_directory: Arc<Mutex<Option<PathBuf>>>,
    /// Resume session ID (from history, passed to --resume)
    resume_session_id: Arc<Mutex<Option<String>>>,
    /// Permission mode for tool calls
    permission_mode: Arc<Mutex<PermissionMode>>,
    /// AppHandle for sending events to frontend
    app_handle: Arc<Mutex<Option<AppHandle>>>,
    /// Pending permission requests (tx channels)
    pending_permissions: Arc<RwLock<std::collections::HashMap<String, PendingPermissionRequest>>>,
    /// 会话内“始终允许”规则
    allowed_permission_rules: Arc<RwLock<std::collections::HashSet<String>>>,
    /// Whether the SDK has been initialized with initialize control request
    sdk_initialized: Arc<Mutex<bool>>,
}

impl ClaudeSessionManager {
    /// Create a new session manager with default config
    pub fn new() -> Self {
        Self::with_config(ServerConfig::default())
    }

    /// Create a new session manager with custom config
    pub fn with_config(config: ServerConfig) -> Self {
        let server = Arc::new(ClaudeSdkServer::new(config));
        let client = Arc::new(ClaudeClient::new(server.clone()));

        Self {
            server,
            client,
            external_session_id: Arc::new(Mutex::new(None)),
            current_session: Arc::new(Mutex::new(None)),
            message_queue: Arc::new(Mutex::new(Vec::new())),
            active_streams: Arc::new(RwLock::new(std::collections::HashSet::new())),
            working_directory: Arc::new(Mutex::new(None)),
            resume_session_id: Arc::new(Mutex::new(None)),
            permission_mode: Arc::new(Mutex::new(PermissionMode::BypassPermissions)),
            app_handle: Arc::new(Mutex::new(None)),
            pending_permissions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            allowed_permission_rules: Arc::new(RwLock::new(std::collections::HashSet::new())),
            sdk_initialized: Arc::new(Mutex::new(false)),
        }
    }

    /// Set the external session ID (assigned by SessionRegistry)
    pub async fn set_external_session_id(&self, session_id: String) {
        *self.external_session_id.lock().await = Some(session_id);
    }

    /// Get the external session ID
    pub async fn get_external_session_id(&self) -> Option<String> {
        self.external_session_id.lock().await.clone()
    }

    /// Set the resume session ID (from history, passed to --resume)
    pub async fn set_resume_session_id(&self, session_id: String) {
        info!("📋 Setting resume session ID: {}", session_id);
        *self.resume_session_id.lock().await = Some(session_id);
    }

    pub async fn set_thinking_level(&self, level: String) {
        self.client.set_thinking_level(level).await;
    }

    /// Get the resume session ID
    pub async fn get_resume_session_id(&self) -> Option<String> {
        self.resume_session_id.lock().await.clone()
    }

    /// Set the permission mode（写入内存并下发到 CLI，供其生效）
    pub async fn set_permission_mode(&self, mode: PermissionMode) {
        info!("Setting permission mode: {:?}", mode);
        *self.permission_mode.lock().await = mode;

        let msg = SdkMessage::ControlRequest(ControlRequest {
            request_id: Uuid::new_v4().to_string(),
            request: ControlRequestPayload::SetPermissionMode {
                mode: mode.as_str().to_string(),
            },
        });
        if let Err(e) = self.server.send_message(msg).await {
            warn!("Failed to send set_permission_mode to CLI: {}", e);
        }
    }

    pub async fn rewind_files(&self, user_message_id: String) -> Result<(), String> {
        let msg = SdkMessage::ControlRequest(ControlRequest {
            request_id: Uuid::new_v4().to_string(),
            request: ControlRequestPayload::RewindFiles { user_message_id },
        });

        self.server
            .send_message(msg)
            .await
            .map_err(|e| format!("Failed to send rewind_files to CLI: {}", e))
    }

    /// Get the permission mode
    pub async fn get_permission_mode(&self) -> PermissionMode {
        *self.permission_mode.lock().await
    }

    /// Set the AppHandle for sending events to frontend
    pub async fn set_app_handle(&self, app: AppHandle) {
        info!("Setting app handle for session");
        *self.app_handle.lock().await = Some(app);
    }

    /// Get the AppHandle
    pub async fn get_app_handle(&self) -> Option<AppHandle> {
        self.app_handle.lock().await.clone()
    }

    /// Send permission request to frontend and wait for decision
    pub async fn handle_permission_request(
        &self,
        ctrl_req: &ControlRequest,
    ) -> Result<PermissionDecision, String> {
        if let ControlRequestPayload::CanUseTool(tool_req) = &ctrl_req.request {
            if self
                .is_permission_rule_allowed(&tool_req.tool_name, &tool_req.input)
                .await
            {
                info!(
                    "Auto-approving remembered permission: tool={} request_id={}",
                    tool_req.tool_name, ctrl_req.request_id
                );
                return Ok(PermissionDecision::Approve {
                    updated_input: None,
                    allow_tools: None,
                });
            }

            info!(
                "Tool use request: {} - {} - {:?}",
                tool_req.tool_name, tool_req.tool_use_id, ctrl_req.request_id
            );

            // Create channel for receiving user decision
            let (tx, rx) = oneshot::channel::<PermissionDecision>();

            // Store pending request with tx
            {
                let mut pending = self.pending_permissions.write().await;
                pending.insert(
                    ctrl_req.request_id.clone(),
                    PendingPermissionRequest {
                        tool_name: tool_req.tool_name.clone(),
                        input: tool_req.input.clone(),
                        tool_use_id: tool_req.tool_use_id.clone(),
                        request_id: ctrl_req.request_id.clone(),
                        description: tool_req.description.clone(),
                        blocked_path: tool_req.blocked_path.clone(),
                        decision_reason: tool_req.decision_reason.clone(),
                        tx,
                    },
                );
            }

            // Get external session ID
            let session_id = self
                .get_external_session_id()
                .await
                .ok_or_else(|| "External session ID not set".to_string())?;

            // Get AppHandle
            let app_handle = self
                .get_app_handle()
                .await
                .ok_or_else(|| "App handle not set".to_string())?;

            // Build permission request data
            let permission_data = serde_json::json!({
                "sessionId": session_id,
                "requestId": ctrl_req.request_id,
                "permission": {
                    "request_id": ctrl_req.request_id,
                    "type": "tool_use",
                    "description": tool_req.description.clone().unwrap_or_else(|| format!("Use tool: {}", tool_req.tool_name)),
                    "params": {
                        "tool_name": tool_req.tool_name,
                        "input": tool_req.input,
                        "tool_use_id": tool_req.tool_use_id,
                        "blocked_path": tool_req.blocked_path,
                        "decision_reason": tool_req.decision_reason,
                    }
                }
            });

            // Emit event to frontend
            app_handle
                .emit("permission-request", permission_data)
                .map_err(|e| format!("Failed to emit permission request: {}", e))?;

            info!(
                "Permission request sent to frontend for {}",
                ctrl_req.request_id
            );

            // Wait for user decision (with timeout)
            let timeout = tokio::time::Duration::from_secs(300); // 5 minutes
            match tokio::time::timeout(timeout, rx).await {
                Ok(Ok(decision)) => Ok(decision),
                Ok(Err(_)) => Err("Permission request channel closed".to_string()),
                Err(_) => Err("Permission request timeout".to_string()),
            }
        } else {
            Err("Not a can_use_tool request".to_string())
        }
    }

    pub async fn remember_permission_rule(
        &self,
        request_id: &str,
    ) -> Result<Option<String>, String> {
        let rule = {
            let pending = self.pending_permissions.read().await;
            let entry = pending
                .get(request_id)
                .ok_or_else(|| format!("Permission request not found: {}", request_id))?;
            build_allow_rule(&entry.tool_name, &entry.input)
        };

        let inserted = {
            let mut rules = self.allowed_permission_rules.write().await;
            rules.insert(rule.clone())
        };

        if inserted {
            info!("Remembered permission rule for session: {}", rule);
            Ok(Some(rule))
        } else {
            Ok(None)
        }
    }

    async fn is_permission_rule_allowed(&self, tool_name: &str, input: &serde_json::Value) -> bool {
        let rules = self.allowed_permission_rules.read().await;
        rules
            .iter()
            .any(|rule| matches_allow_rule(rule, tool_name, input))
    }

    /// Send ControlResponse to Claude CLI based on user decision
    pub async fn send_permission_decision(
        &self,
        request_id: String,
        decision: PermissionDecision,
    ) -> Result<(), String> {
        // Clone the decision for later use (since we need to consume it for the channel)
        let decision_for_response = match &decision {
            PermissionDecision::Approve {
                updated_input,
                allow_tools,
            } => PermissionDecision::Approve {
                updated_input: updated_input.clone(),
                allow_tools: allow_tools.clone(),
            },
            PermissionDecision::Reject { reason, suggestion } => PermissionDecision::Reject {
                reason: reason.clone(),
                suggestion: suggestion.clone(),
            },
            PermissionDecision::ChangeMode { mode } => {
                PermissionDecision::ChangeMode { mode: *mode }
            }
        };

        // Remove the pending request and get the original input and tx
        let (original_input, tx) = {
            let mut pending = self.pending_permissions.write().await;
            let entry = pending.remove(&request_id);
            entry
                .map(|p| (p.input, p.tx))
                .ok_or_else(|| format!("Permission request not found: {}", request_id))?
        };

        // Send the decision through the channel to wake up the waiting task
        let _ = tx.send(decision);
        info!(
            "Permission decision sent through channel for {}",
            request_id
        );

        // Build and send the ControlResponse
        let response = match decision_for_response {
            PermissionDecision::Approve {
                updated_input,
                allow_tools,
            } => {
                // Build allowTools list
                let allow_tools_list = allow_tools.unwrap_or_default();
                if !allow_tools_list.is_empty() {
                    info!("User allowed tools: {:?}", allow_tools_list);
                }

                // Protocol requires updatedInput to be present.
                // If not provided by user, use the original input from the request.
                let final_input = updated_input.unwrap_or_else(|| original_input.clone());

                SdkMessage::ControlResponse(ControlResponse {
                    response: ControlResponseInner {
                        subtype: ControlResponseType::Success,
                        request_id: request_id.clone(),
                        response: Some(serde_json::json!({
                            "behavior": "allow",
                            "updatedInput": final_input,
                            "updatedPermissions": if !allow_tools_list.is_empty() {
                                serde_json::to_value(allow_tools_list).unwrap_or_default()
                            } else {
                                serde_json::json!([])
                            }
                        })),
                        error: None,
                    },
                })
            }
            PermissionDecision::Reject { reason, suggestion } => {
                let message = if let Some(sug) = suggestion {
                    format!("{}. {}", reason, sug)
                } else {
                    reason.clone()
                };

                // Deny response only needs behavior and message (optional interrupt)
                SdkMessage::ControlResponse(ControlResponse {
                    response: ControlResponseInner {
                        subtype: ControlResponseType::Success,
                        request_id: request_id.clone(),
                        response: Some(serde_json::json!({
                            "behavior": "deny",
                            "message": message
                        })),
                        error: None,
                    },
                })
            }
            PermissionDecision::ChangeMode { mode } => {
                // For mode change, we approve the current request and change mode
                SdkMessage::ControlResponse(ControlResponse {
                    response: ControlResponseInner {
                        subtype: ControlResponseType::Success,
                        request_id: request_id.clone(),
                        response: Some(serde_json::json!({
                            "behavior": "allow",
                            "updatedInput": original_input
                        })),
                        error: None,
                    },
                })
            }
        };

        // Send response to CLI
        self.server
            .send_message(response)
            .await
            .map_err(|e| format!("Failed to send control response: {}", e))?;

        info!("Permission decision sent to CLI for {}", request_id);
        Ok(())
    }

    /// Start WebSocket server
    pub async fn start(&self) -> Result<(), String> {
        info!("Starting session manager...");
        self.server
            .start()
            .await
            .map_err(|e| format!("Failed to start server: {}", e))?;
        info!("Session manager started");
        Ok(())
    }

    /// Launch Claude CLI
    pub async fn launch_cli(&self) -> Result<(), String> {
        info!("Launching Claude CLI...");

        // First ensure server is started
        if !self.server.is_connected().await {
            info!("WebSocket not connected, starting server first...");
            self.start().await?;
        }

        // Wait a moment for server to be ready
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Get resume session ID if set
        let resume_id = self.get_resume_session_id().await;

        // Launch CLI with or without --resume parameter
        if let Some(session_id) = resume_id {
            info!("🔄 Launching CLI with --resume: {}", session_id);
            self.client
                .launch_with_resume(session_id)
                .await
                .map_err(|e| format!("Failed to launch CLI with --resume: {}", e))?;
        } else {
            info!("🚀 Launching CLI (new session)");
            self.client
                .launch()
                .await
                .map_err(|e| format!("Failed to launch CLI: {}", e))?;
        }

        info!("Claude CLI launched");

        // Wait for WebSocket connection to be established
        info!("⏳ Waiting for WebSocket connection...");
        let mut attempts = 0;
        let max_attempts = 200; // 10 seconds
        while attempts < max_attempts {
            if self.server.is_websocket_connected().await {
                info!("✅ WebSocket connection established");
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            attempts += 1;
        }

        if !self.server.is_websocket_connected().await {
            return Err("WebSocket connection timeout".to_string());
        }

        // Send initialize control request immediately after connection
        info!("📤 Sending initialize control request...");
        self.send_initialize_request().await?;

        // Start background task to capture CLI session ID when system.init arrives
        // This doesn't block the session creation
        let server = self.server.clone();
        let client = self.client.clone();
        let external_session_id = self.external_session_id.clone();
        tokio::spawn(async move {
            // Wait for system.init message
            let mut attempts = 0;
            let max_attempts = 100; // 5 seconds

            while attempts < max_attempts {
                if let Some(cli_session_id) = server.session_id().await {
                    info!("📋 Background: Captured CLI session ID: {}", cli_session_id);

                    // Save to client
                    client.set_cli_session_id(cli_session_id.clone()).await;

                    // Save to SessionRegistry
                    if let Some(external_id) = external_session_id.lock().await.as_ref() {
                        use crate::claude::session_registry::get_session_registry;
                        let registry = get_session_registry();
                        registry
                            .set_cli_session_id(external_id, cli_session_id.clone())
                            .await;
                        info!("✅ Background: Saved CLI session ID for {}", external_id);
                    }

                    return;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                attempts += 1;
            }
            warn!("⚠️ Background: Timeout waiting for CLI session ID");
        });

        Ok(())
    }

    /// Wait for system.init message and extract CLI session ID
    /// This should be called after launch_cli to capture the session ID for --resume
    pub async fn wait_for_init_and_save_session_id(&self) -> Result<String, String> {
        info!("Waiting for system.init message to extract CLI session ID...");

        let mut attempts = 0;
        let max_attempts = 100; // 5 seconds timeout

        while attempts < max_attempts {
            // Check if we already have a session ID from the server
            if let Some(session_id) = self.server.session_id().await {
                info!("✅ Extracted CLI session ID: {}", session_id);

                // Save to client for future --resume calls
                self.client.set_cli_session_id(session_id.clone()).await;

                // Save to SessionRegistry if we have an external session ID
                if let Some(external_id) = self.get_external_session_id().await {
                    use crate::claude::session_registry::get_session_registry;
                    let registry = get_session_registry();
                    registry
                        .set_cli_session_id(&external_id, session_id.clone())
                        .await;
                    info!(
                        "✅ Saved CLI session ID to SessionRegistry for {}",
                        external_id
                    );
                }

                return Ok(session_id);
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            attempts += 1;
        }

        // Timeout: Use external session ID instead (for new sessions)
        warn!("⚠️ Timeout waiting for system.init message, using external session ID");
        if let Some(external_id) = self.get_external_session_id().await {
            info!(
                "✅ Using external session ID as CLI session ID: {}",
                external_id
            );

            // Save to client for future --resume calls
            self.client.set_cli_session_id(external_id.clone()).await;

            // Save to SessionRegistry
            use crate::claude::session_registry::get_session_registry;
            let registry = get_session_registry();
            registry
                .set_cli_session_id(&external_id, external_id.clone())
                .await;

            return Ok(external_id);
        }

        Err("No session ID available".to_string())
    }

    /// Relaunch CLI with --resume to restore conversation context
    pub async fn relaunch_cli(&self) -> Result<(), String> {
        info!("🔄 Relaunching CLI with --resume...");
        self.client.relaunch().await
    }

    /// Stop CLI process
    pub async fn stop_cli(&self) -> Result<(), String> {
        info!("🛑 Stopping CLI...");
        self.client.stop().await
    }

    /// Create a new session
    pub async fn create_session(&self) -> Result<Session, String> {
        info!("Creating new session...");

        // Ensure server is running
        if !self.server.is_connected().await {
            info!("Server not connected, launching CLI...");
            self.launch_cli().await?;
        }

        // Wait for connection and init
        let mut attempts = 0;
        while attempts < 50 {
            if self.server.is_connected().await {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            attempts += 1;
        }

        if !self.server.is_connected().await {
            return Err("Claude CLI did not connect in time".to_string());
        }

        // Create session object
        let session = Session::new();
        info!("Created new session: {}", session.id);

        // Store session
        *self.current_session.lock().await = Some(session.clone());

        // Clear message queue
        self.message_queue.lock().await.clear();

        info!("Session ready: {}", session.id);
        Ok(session)
    }

    /// Send a message to Claude
    pub async fn send_message(
        &self,
        session_id: String,
        content: String,
    ) -> Result<Message, String> {
        info!(
            "send_message called: session_id={}, content_length={}",
            session_id,
            content.len()
        );

        // Verify session using external_session_id
        let external_id = self.external_session_id.lock().await;
        if external_id.as_ref() != Some(&session_id) {
            error!(
                "Session ID mismatch: expected={:?}, got={}",
                external_id.as_ref(),
                session_id
            );
            return Err("Session ID mismatch".to_string());
        }
        drop(external_id);

        // Get the CLI session_id
        let cli_session_id = self.server.session_id().await.unwrap_or_default();
        info!("CLI session_id: {:?}", cli_session_id);

        // Create user message
        let user_msg = SdkMessage::User(UserMessage {
            message: UserMessageContent::Simple {
                role: "user".to_string(),
                content: content.clone(),
            },
            parent_tool_use_id: None,
            session_id: cli_session_id.clone(),
            uuid: Some(Uuid::new_v4().to_string()),
            is_synthetic: None,
        });

        info!(
            "📤 Message content preview: {}...",
            &content[..content.len().min(200)]
        );

        self.server.send_message(user_msg).await.map_err(|e| {
            error!("Failed to send message: {}", e);
            format!("Failed to send message: {}", e)
        })?;

        // Wait for response
        let response = self.wait_for_response().await?;
        info!(
            "Response received: id={}, role={:?}",
            response.id, response.role
        );

        Ok(response)
    }

    /// Wait for a response from Claude
    async fn wait_for_response(&self) -> Result<Message, String> {
        info!("Waiting for response from Claude...");

        let mut message_content: Option<String> = None;
        let mut _tool_calls: Vec<String> = Vec::new();
        let mut is_complete = false;
        let mut attempts = 0;
        let max_attempts = 600; // 30 seconds timeout (50ms * 600)

        // Process incoming messages
        while !is_complete && attempts < max_attempts {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            attempts += 1;

            // Try to receive message from server
            match self.server.recv_message().await {
                Some(msg) => {
                    info!(
                        "Received message from server: {:?}",
                        std::mem::discriminant(&msg)
                    );

                    match &msg {
                        SdkMessage::Assistant(assist) => {
                            info!("Assistant message received: id={}", assist.message.id);
                            // For now, just mark as complete when we get any assistant message
                            // In real implementation, we'd accumulate streaming content
                            message_content =
                                Some(format!("Assistant response: {}", assist.message.id));
                            is_complete = true;
                        }
                        SdkMessage::System(sys) => {
                            info!("System message: subtype={:?}", sys.subtype);
                        }
                        SdkMessage::User(_user) => {
                            info!("User message echoed back");
                        }
                        _ => {
                            debug!(
                                "Other message type received: {:?}",
                                std::mem::discriminant(&msg)
                            );
                        }
                    }
                }
                None => {
                    // No message yet, continue waiting
                    if attempts % 20 == 0 {
                        // Log every second
                        debug!(
                            "   Still waiting... (attempt {}/{})",
                            attempts, max_attempts
                        );
                    }
                }
            }
        }

        if !is_complete {
            warn!("Timeout waiting for response after {} attempts", attempts);
            return Err("Timeout waiting for response".to_string());
        }

        // Get current session
        let session = self.current_session.lock().await;
        let session_id = session.as_ref().map(|s| s.id.clone()).unwrap_or_default();

        drop(session);

        // Create response message
        let response = Message::new(
            session_id,
            MessageRole::Assistant,
            MessageContent::Text(message_content.unwrap_or_else(|| "No response".to_string())),
        );

        info!("Response ready: id={}", response.id);
        Ok(response)
    }

    /// Get SDK status
    pub async fn get_status(&self) -> Result<SdkStatus, String> {
        let cli_path = self
            .client
            .find_cli()
            .await
            .ok()
            .map(|p| p.to_string_lossy().to_string());

        let initialized = self.server.is_connected().await;

        info!(
            "SDK Status: initialized={}, cli_path={:?}",
            initialized, cli_path
        );

        Ok(SdkStatus {
            initialized,
            cli_path,
            error: None,
        })
    }

    /// Stop session manager
    pub async fn stop(&self) -> Result<(), String> {
        info!("Stopping session manager...");
        self.client.stop().await?;
        self.server.stop().await;
        info!("Session manager stopped");
        Ok(())
    }

    /// Send a message with streaming support
    pub async fn send_message_streaming(
        &self,
        session_id: String,
        content: String,
        data_callback: StreamDataCallback,
        complete_callback: StreamCompleteCallback,
    ) -> Result<Message, String> {
        info!("send_message_streaming called: session_id={}", session_id);

        // Use new message-based streaming internally
        let message_callback: StreamMessageCallback = Box::new(move |_msg| {
            // For backward compatibility, ignore message callbacks when using old API
        });

        self.send_message_streaming_with_messages(
            session_id,
            content,
            None,
            data_callback,
            message_callback,
            complete_callback,
        )
        .await
    }

    /// Send a message with streaming support and message callbacks
    pub async fn send_message_streaming_with_messages(
        &self,
        session_id: String,
        content: String,
        content_blocks: Option<Vec<ContentBlock>>,
        data_callback: StreamDataCallback,
        message_callback: StreamMessageCallback,
        complete_callback: StreamCompleteCallback,
    ) -> Result<Message, String> {
        info!(
            "send_message_streaming_with_messages called: session_id={}",
            session_id
        );

        // Add to active streams
        self.active_streams.write().await.insert(session_id.clone());

        // Verify session using external_session_id
        let external_id = self.external_session_id.lock().await;
        if external_id.as_ref() != Some(&session_id) {
            self.active_streams.write().await.remove(&session_id);
            error!(
                "Session ID mismatch in streaming: expected={:?}, got={}",
                external_id.as_ref(),
                session_id
            );
            return Err("Session ID mismatch".to_string());
        }
        drop(external_id);

        // Get the CLI session_id
        let cli_session_id = self.server.session_id().await.unwrap_or_default();

        // Create user message
        let user_message_content = match content_blocks {
            Some(blocks) if !blocks.is_empty() => UserMessageContent::Structured {
                role: "user".to_string(),
                content: blocks,
            },
            _ => UserMessageContent::Simple {
                role: "user".to_string(),
                content: content.clone(),
            },
        };

        let user_msg = SdkMessage::User(UserMessage {
            message: user_message_content,
            parent_tool_use_id: None,
            session_id: cli_session_id,
            uuid: Some(Uuid::new_v4().to_string()),
            is_synthetic: None,
        });

        info!("📤 Sending streaming message...");
        info!(
            "📄 Content preview: {}...",
            &content[..content.len().min(200)]
        );

        // Send to CLI
        info!("🚀 Sending message via WebSocket to CLI...");
        self.server.send_message(user_msg).await.map_err(|e| {
            error!("Failed to send streaming message: {}", e);
            format!("Failed to send message: {}", e)
        })?;

        info!("Streaming message sent, waiting for response...");

        // Wait for response with streaming
        let response = self
            .wait_for_response_streaming_with_messages(
                &session_id,
                data_callback,
                message_callback,
                complete_callback,
            )
            .await?;

        // Remove from active streams
        self.active_streams.write().await.remove(&session_id);

        info!("Streaming response complete");
        Ok(response)
    }

    /// Wait for a response from Claude with streaming
    async fn wait_for_response_streaming(
        &self,
        _session_id: &str,
        data_callback: StreamDataCallback,
        complete_callback: StreamCompleteCallback,
    ) -> Result<Message, String> {
        info!("Waiting for streaming response...");

        let mut final_result: Option<String> = None;
        let mut is_complete = false;
        let mut attempts = 0;
        let max_attempts = 1200; // 增加超时时间，因为可能需要等待多个回合
                                 // 跟踪是否收到了流式数据，用于区分流式模式和直接响应模式
        let mut has_streaming_data = false;
        let mut final_usage: Option<crate::claude::protocol::Usage> = None; // 保存最终的 usage 数据

        // Process incoming messages
        while !is_complete && attempts < max_attempts {
            match self.server.recv_message().await {
                Some(msg) => {
                    // 重置空闲计数器
                    attempts = 0;
                    match &msg {
                        SdkMessage::Assistant(assist) => {
                            info!(
                                "Assistant message in streaming: id={}, model={}",
                                assist.message.id, assist.message.model
                            );

                            // 只有在没有流式数据的情况下才发送 Assistant 消息内容
                            // 这样可以同时支持流式模式和直接响应模式
                            if !has_streaming_data {
                                // 提取并发送内容到前端进行流式显示
                                let content = Self::extract_assistant_content(&assist.message);

                                // 如果有文本内容，发送到前端
                                if !content.is_empty()
                                    && !content.starts_with("[Assistant response:")
                                {
                                    info!(
                                        "Streaming content to frontend (direct mode): {} chars",
                                        content.len()
                                    );
                                    data_callback(content);
                                }

                                // 检查是否有工具调用
                                for block in &assist.message.content {
                                    if let ContentBlock::ToolUse { id, name, input: _ } = block {
                                        info!("Tool use detected: {} - {}", name, id);
                                        // 可以在这里发送工具调用信息到前端
                                    }
                                }
                            } else {
                                info!("Skipping assistant message content (streaming mode, data already sent via StreamEvent)");
                            }
                        }
                        SdkMessage::System(sys) => {
                            info!("System message: subtype={:?}", sys.subtype);
                        }
                        SdkMessage::User(_user) => {
                            info!("User message (tool result) received");
                            // 处理 User 消息（工具结果）
                            // 注意：User 消息在流式过程中可能包含 tool_result
                        }
                        SdkMessage::ControlResponse(_resp) => {
                            info!("Control response received");
                        }
                        SdkMessage::ControlRequest(ctrl_req) => {
                            info!(
                                "ControlRequest received: subtype={:?}",
                                std::mem::discriminant(&ctrl_req.request)
                            );

                            // 转发 can_use_tool 请求到前端，让用户审批
                            if let ControlRequestPayload::CanUseTool(tool_req) = &ctrl_req.request {
                                info!(
                                    "Tool use request: {} - {} - {:?}",
                                    tool_req.tool_name, tool_req.tool_use_id, ctrl_req.request_id
                                );

                                // 克隆 ctrl_req 以便在 tokio::spawn 中使用
                                let ctrl_req_clone = ctrl_req.clone();
                                let manager_clone = self.clone();
                                let request_id = ctrl_req.request_id.clone();

                                tokio::spawn(async move {
                                    match manager_clone
                                        .handle_permission_request(&ctrl_req_clone)
                                        .await
                                    {
                                        Ok(decision) => {
                                            // 发送用户决定到 Claude CLI
                                            if let Err(e) = manager_clone
                                                .send_permission_decision(
                                                    request_id.clone(),
                                                    decision,
                                                )
                                                .await
                                            {
                                                error!("Failed to send permission decision: {}", e);
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to handle permission request: {}", e);
                                            // 超时或其他错误时，默认拒绝
                                            let fallback_response =
                                                SdkMessage::ControlResponse(ControlResponse {
                                                    response: ControlResponseInner {
                                                        subtype: ControlResponseType::Error,
                                                        request_id: request_id.clone(),
                                                        response: None,
                                                        error: Some(format!(
                                                            "Permission request failed: {}",
                                                            e
                                                        )),
                                                    },
                                                });
                                            let _ = manager_clone
                                                .server
                                                .send_message(fallback_response)
                                                .await;
                                        }
                                    }
                                });
                            }
                        }
                        SdkMessage::ControlCancelRequest { .. } => {
                            info!("Control cancel request received");
                        }
                        SdkMessage::StreamEvent(stream_evt) => {
                            // 处理流式事件，提取文本增量并发送到前端
                            if let Some(text_delta) = Self::extract_text_delta(&stream_evt.event) {
                                info!("StreamEvent text delta: {} chars", text_delta.len());
                                // 标记为流式模式
                                has_streaming_data = true;
                                // 发送到前端进行流式显示
                                data_callback(text_delta);
                            }
                        }
                        SdkMessage::Result(res) => {
                            info!(
                                "Result message received: subtype={}, is_error={}",
                                res.subtype, res.is_error
                            );

                            // 获取最终结果
                            if let Some(result) = &res.result {
                                final_result = Some(result.clone());
                                info!("Final result received: {} chars", result.len());
                            }

                            // 保存 usage 数据（用于最终消息）
                            if let Some(usage) = &res.usage {
                                final_usage = Some(usage.clone());
                                info!("Result usage data saved: {:?}", final_usage);
                            }

                            // Result 消息表示查询完成
                            is_complete = true;
                        }
                        _ => {
                            debug!(
                                "Other message type in streaming: {:?}",
                                std::mem::discriminant(&msg)
                            );
                        }
                    }
                }
                None => {
                    // No message yet, continue waiting
                    if attempts % 20 == 0 {
                        debug!(
                            "   Still waiting for streaming response... (attempt {}/{})",
                            attempts, max_attempts
                        );
                    }
                }
            }
        }

        if !is_complete {
            warn!("Timeout in streaming response after {} attempts", attempts);
            return Err("Timeout waiting for streaming response".to_string());
        }

        // Get current session
        let session = self.current_session.lock().await;
        let session_id = session.as_ref().map(|s| s.id.clone()).unwrap_or_default();

        drop(session);

        // 使用最终结果作为响应内容
        let response_content = final_result.unwrap_or_else(|| {
            warn!("No final result from Result message");
            "No response".to_string()
        });

        // Create response message
        let response = Message::new(
            session_id,
            MessageRole::Assistant,
            MessageContent::Text(response_content.clone()),
        );

        info!(
            "Calling complete_callback with final response: {} chars",
            response_content.len()
        );
        complete_callback(response.clone());

        Ok(response)
    }

    /// Wait for a response from Claude with streaming and message callbacks
    async fn wait_for_response_streaming_with_messages(
        &self,
        session_id: &str,
        data_callback: StreamDataCallback, // 用于发送流式文本增量
        message_callback: StreamMessageCallback,
        complete_callback: StreamCompleteCallback,
    ) -> Result<Message, String> {
        info!("Waiting for streaming response with message callbacks...");

        let mut final_result: Option<String> = None;
        let mut is_complete = false;
        let mut attempts = 0;
        let max_attempts = 1200;
        // 跟踪是否收到了流式数据，用于区分流式模式和直接响应模式
        let mut has_streaming_data = false;
        let mut final_usage: Option<crate::claude::protocol::Usage> = None; // 保存最终的 usage 数据

        // Process incoming messages
        while !is_complete && attempts < max_attempts {
            match self.server.recv_message().await {
                Some(msg) => {
                    // 重置空闲计数器
                    attempts = 0;
                    match &msg {
                        SdkMessage::Assistant(assist) => {
                            info!(
                                "Assistant message in streaming: id={}, model={}",
                                assist.message.id, assist.message.model
                            );

                            // 只有在没有流式数据的情况下才发送 Assistant 消息
                            // 这样可以同时支持：
                            // 1. 流式模式：StreamEvent 发送数据，不发送 Assistant 消息
                            // 2. 直接响应模式：没有 StreamEvent，发送完整的 Assistant 消息
                            if !has_streaming_data {
                                let message = self.create_assistant_message(session_id, assist);

                                let blocks_count = match &message.content {
                                    MessageContent::Blocks(blocks) => blocks.len(),
                                    MessageContent::Text(_) => 1,
                                };
                                info!("Sending assistant message to frontend (direct mode): {} blocks", blocks_count);
                                message_callback(message);
                            } else {
                                info!("Skipping assistant message (streaming mode, data already sent via StreamEvent)");
                            }
                        }
                        SdkMessage::System(sys) => {
                            info!("System message: subtype={:?}", sys.subtype);
                        }
                        SdkMessage::User(user) => {
                            info!("User message (tool result) received");

                            // 为 User 消息（工具结果）创建 Message 并推送到前端
                            if let Some(message) = self.create_user_message(session_id, user) {
                                info!("Sending user message to frontend");
                                message_callback(message);
                            }
                        }
                        SdkMessage::ControlResponse(_resp) => {
                            info!("Control response received");
                        }
                        SdkMessage::ControlRequest(ctrl_req) => {
                            info!(
                                "ControlRequest received: subtype={:?}",
                                std::mem::discriminant(&ctrl_req.request)
                            );

                            if let ControlRequestPayload::CanUseTool(tool_req) = &ctrl_req.request {
                                info!(
                                    "Tool use request: {} - {} - {:?}",
                                    tool_req.tool_name, tool_req.tool_use_id, ctrl_req.request_id
                                );

                                let ctrl_req_clone = ctrl_req.clone();
                                let manager_clone = self.clone();
                                let request_id = ctrl_req.request_id.clone();

                                tokio::spawn(async move {
                                    match manager_clone
                                        .handle_permission_request(&ctrl_req_clone)
                                        .await
                                    {
                                        Ok(decision) => {
                                            if let Err(e) = manager_clone
                                                .send_permission_decision(
                                                    request_id.clone(),
                                                    decision,
                                                )
                                                .await
                                            {
                                                error!("Failed to send permission decision: {}", e);
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to handle permission request: {}", e);
                                            let fallback_response =
                                                SdkMessage::ControlResponse(ControlResponse {
                                                    response: ControlResponseInner {
                                                        subtype: ControlResponseType::Error,
                                                        request_id: request_id.clone(),
                                                        response: None,
                                                        error: Some(format!(
                                                            "Permission request failed: {}",
                                                            e
                                                        )),
                                                    },
                                                });
                                            let _ = manager_clone
                                                .server
                                                .send_message(fallback_response)
                                                .await;
                                        }
                                    }
                                });
                            }
                        }
                        SdkMessage::ControlCancelRequest { .. } => {
                            info!("Control cancel request received");
                        }
                        SdkMessage::StreamEvent(stream_evt) => {
                            // 处理流式事件，提取文本增量并发送到前端
                            if let Some(text_delta) = Self::extract_text_delta(&stream_evt.event) {
                                info!("StreamEvent text delta: {} chars", text_delta.len());
                                // 标记为流式模式
                                has_streaming_data = true;
                                // 发送到前端进行流式显示
                                data_callback(text_delta);
                            }
                        }
                        SdkMessage::Result(res) => {
                            info!(
                                "Result message received: subtype={}, is_error={}",
                                res.subtype, res.is_error
                            );

                            if let Some(result) = &res.result {
                                final_result = Some(result.clone());
                                info!("Final result received: {} chars", result.len());
                            }

                            // 保存 usage 数据（用于最终消息）
                            if let Some(usage) = &res.usage {
                                final_usage = Some(usage.clone());
                                info!("Result usage data saved: {:?}", final_usage);
                            }

                            is_complete = true;
                        }
                        _ => {
                            debug!(
                                "Other message type in streaming: {:?}",
                                std::mem::discriminant(&msg)
                            );
                        }
                    }
                }
                None => {
                    // 没有消息，短暂等待后继续
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    attempts += 1;
                    if attempts % 100 == 0 {
                        // 每秒记录一次
                        debug!(
                            "   Still waiting for streaming response... (attempt {}/{})",
                            attempts, max_attempts
                        );
                    }
                }
            }
        }

        if !is_complete {
            warn!("Timeout in streaming response after {} attempts", attempts);
            return Err("Timeout waiting for streaming response".to_string());
        }

        let session = self.current_session.lock().await;
        let session_id = session.as_ref().map(|s| s.id.clone()).unwrap_or_default();
        drop(session);

        let response_content = final_result.unwrap_or_else(|| {
            warn!("No final result from Result message");
            "No response".to_string()
        });

        let mut response = Message::new(
            session_id,
            MessageRole::Assistant,
            MessageContent::Text(response_content.clone()),
        );

        // 将 Result 消息中的 usage 数据附加到响应消息
        if let Some(usage) = final_usage {
            response.usage = Some(crate::models::TokenUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_creation_input_tokens: usage.cache_creation_input_tokens,
                cache_read_input_tokens: usage.cache_read_input_tokens,
            });
            info!(
                "✅ Attached usage to final response: in={}, out={}",
                usage.input_tokens, usage.output_tokens
            );
        }

        info!(
            "Calling complete_callback with final response: {} chars",
            response_content.len()
        );
        complete_callback(response.clone());

        Ok(response)
    }

    /// Create a Message from an AssistantMessage
    fn create_assistant_message(&self, session_id: &str, assist: &AssistantMessage) -> Message {
        use crate::models::ContentBlock as ModelContentBlock;

        // 提取内容块
        let mut blocks = Vec::new();
        let mut text_parts = Vec::new();

        for block in &assist.message.content {
            match block {
                ContentBlock::Text { text } => {
                    text_parts.push(text.clone());
                    blocks.push(ModelContentBlock {
                        r#type: "text".to_string(),
                        content: text.clone(),
                        media_source: None,
                    });
                }
                ContentBlock::Image { source } => {
                    blocks.push(ModelContentBlock {
                        r#type: "image".to_string(),
                        content: String::new(),
                        media_source: Some(crate::models::MediaSource {
                            r#type: source.r#type.clone(),
                            media_type: Some(source.media_type.clone()),
                            data: source.data.clone(),
                        }),
                    });
                }
                ContentBlock::Thinking { thinking, .. } => {
                    blocks.push(ModelContentBlock {
                        r#type: "thinking".to_string(),
                        content: thinking.clone(),
                        media_source: None,
                    });
                }
                ContentBlock::ToolUse { id, name, input } => {
                    // 保存完整的 tool_use 信息，包括 id、name 和 input
                    let tool_use_info = serde_json::json!({
                        "id": id,
                        "name": name,
                        "input": input
                    });
                    blocks.push(ModelContentBlock {
                        r#type: "tool_use".to_string(),
                        content: tool_use_info.to_string(),
                        media_source: None,
                    });
                }
                ContentBlock::ToolResult {
                    tool_use_id: _,
                    content,
                    is_error: _,
                } => {
                    let content_str = match content {
                        ToolResultContent::Single(s) => s.clone(),
                        ToolResultContent::Multiple(blocks) => {
                            serde_json::to_string(blocks).unwrap_or_default()
                        }
                    };
                    blocks.push(ModelContentBlock {
                        r#type: "tool_result".to_string(),
                        content: content_str,
                        media_source: None,
                    });
                }
            }
        }

        // 组合文本内容
        let text_content = if text_parts.is_empty() {
            format!("[Assistant response: {}]", assist.message.id)
        } else {
            text_parts.join("\n")
        };

        let (text_content, role) = normalize_cli_message(&text_content)
            .map(|normalized| normalized.role_or(MessageRole::Assistant))
            .unwrap_or((text_content, MessageRole::Assistant));

        // 创建 Message
        let mut msg = Message::new(
            session_id.to_string(),
            role,
            MessageContent::Text(text_content),
        );

        // 添加结构化内容块
        if !blocks.is_empty() {
            msg.content = MessageContent::Blocks(blocks);
        }

        // 添加模型和token使用信息
        msg.model = Some(assist.message.model.clone());
        msg.usage = Some(TokenUsage {
            input_tokens: assist.message.usage.input_tokens,
            output_tokens: assist.message.usage.output_tokens,
            cache_creation_input_tokens: assist.message.usage.cache_creation_input_tokens,
            cache_read_input_tokens: assist.message.usage.cache_read_input_tokens,
        });

        msg
    }

    /// Create a Message from a UserMessage (for tool results)
    fn create_user_message(&self, session_id: &str, user: &UserMessage) -> Option<Message> {
        use crate::models::ContentBlock as ModelContentBlock;

        // 检查是否为工具结果消息
        match &user.message {
            UserMessageContent::Structured { role: _, content } => {
                // 查找 tool_result 块
                for block in content {
                    match block {
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } => {
                            let content_str = match content {
                                ToolResultContent::Single(s) => s.clone(),
                                ToolResultContent::Multiple(blocks) => {
                                    serde_json::to_string(blocks).unwrap_or_default()
                                }
                            };

                            let tool_result_data = serde_json::json!({
                                "type": "tool_result",
                                "tool_use_id": tool_use_id,
                                "content": content_str,
                                "is_error": is_error.unwrap_or(false)
                            });

                            let mut blocks = Vec::new();
                            blocks.push(ModelContentBlock {
                                r#type: "tool_result".to_string(),
                                content: tool_result_data.to_string(),
                                media_source: None,
                            });

                            let mut msg = Message::new(
                                session_id.to_string(),
                                MessageRole::User,
                                MessageContent::Text(content_str.clone()),
                            );
                            msg.content = MessageContent::Blocks(blocks);

                            info!(
                                "Created tool_result message with tool_use_id: {:?}",
                                tool_use_id
                            );
                            return Some(msg);
                        }
                        ContentBlock::Image { source } => {
                            let mut msg = Message::new(
                                session_id.to_string(),
                                MessageRole::User,
                                MessageContent::Text(String::new()),
                            );
                            msg.content = MessageContent::Blocks(vec![ModelContentBlock {
                                r#type: "image".to_string(),
                                content: String::new(),
                                media_source: Some(crate::models::MediaSource {
                                    r#type: source.r#type.clone(),
                                    media_type: Some(source.media_type.clone()),
                                    data: source.data.clone(),
                                }),
                            }]);
                            return Some(msg);
                        }
                        _ => {}
                    }
                }
                None
            }
            UserMessageContent::Simple { role: _, content } => {
                match normalize_cli_message(content) {
                    Some(normalized) if normalized.kind == NormalizedMessageKind::System => {
                        Some(Message::new(
                            session_id.to_string(),
                            MessageRole::System,
                            MessageContent::Text(normalized.text),
                        ))
                    }
                    _ => None,
                }
            }
        }
    }

    /// Get commands (from initialize) and skills (from system.init)
    pub async fn get_commands_and_skills(
        &self,
    ) -> Result<(Option<Vec<CommandInfo>>, Option<Vec<String>>), String> {
        Ok(self.server.get_commands_and_skills().await)
    }

    /// Stop streaming for a session
    pub async fn stop_streaming(&self, session_id: String) -> Result<(), String> {
        info!("Stopping streaming for session: {}", session_id);

        // Remove from active streams
        self.active_streams.write().await.remove(&session_id);

        info!("Streaming stopped");
        Ok(())
    }

    /// Set working directory for Claude CLI
    pub async fn set_working_directory(&self, path: String) -> Result<(), String> {
        info!("set_working_directory called: {}", path);

        let path_buf = PathBuf::from(&path);

        // Verify directory exists
        if !path_buf.exists() {
            error!("Directory does not exist: {}", path);
            return Err(format!("Directory does not exist: {}", path));
        }

        if !path_buf.is_dir() {
            error!("Path is not a directory: {}", path);
            return Err(format!("Path is not a directory: {}", path));
        }

        // Store working directory
        *self.working_directory.lock().await = Some(path_buf.clone());

        // Update client's working directory
        self.client.set_working_directory(path_buf).await;

        info!("Working directory set to: {}", path);
        Ok(())
    }

    /// Get current working directory
    pub async fn get_working_directory(&self) -> Option<PathBuf> {
        let dir = self.working_directory.lock().await.clone();
        debug!("get_working_directory: {:?}", dir);
        dir
    }

    /// Send initialize control request to CLI
    /// This must be called before the first user message
    pub async fn send_initialize_request(&self) -> Result<Option<InitializeResponse>, String> {
        info!("📤 Sending initialize control request...");

        // Check if already initialized
        {
            let initialized = self.sdk_initialized.lock().await;
            if *initialized {
                info!("ℹ️ SDK already initialized, skipping");
                return Ok(None);
            }
        }

        // Wait for CLI connection
        let mut attempts = 0;
        let max_attempts = 100; // 5 seconds
        while attempts < max_attempts {
            if self.server.is_websocket_connected().await {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            attempts += 1;
        }

        if !self.server.is_websocket_connected().await {
            return Err("CLI not connected, cannot send initialize request".to_string());
        }

        // Generate request ID
        let request_id = Uuid::new_v4().to_string();

        // Create initialize request with empty parameters
        let init_msg = SdkMessage::ControlRequest(ControlRequest {
            request_id: request_id.clone(),
            request: ControlRequestPayload::Initialize(InitializeRequest {
                hooks: None,
                sdk_mcp_servers: None,
                json_schema: None,
                system_prompt: None,
                append_system_prompt: None,
                agents: None,
            }),
        });

        // Send initialize request
        self.server
            .send_message(init_msg)
            .await
            .map_err(|e| format!("Failed to send initialize request: {}", e))?;

        info!("✅ Initialize request sent, waiting for response...");

        // Wait for control response with timeout
        let timeout = tokio::time::Duration::from_secs(10);
        let start_time = std::time::Instant::now();

        while start_time.elapsed() < timeout {
            if let Some(msg) = self.server.recv_message().await {
                match msg {
                    SdkMessage::ControlResponse(ctrl_resp) => {
                        if ctrl_resp.response.request_id == request_id {
                            match ctrl_resp.response.subtype {
                                ControlResponseType::Success => {
                                    info!("✅ Initialize request succeeded");

                                    // Mark as initialized
                                    *self.sdk_initialized.lock().await = true;

                                    // Try to parse the response data
                                    info!(
                                        "📋 response.response is_some: {:?}",
                                        ctrl_resp.response.response.is_some()
                                    );
                                    if let Some(response_value) = ctrl_resp.response.response {
                                        // InitializeResponse has fields: commands, output_style, models, account
                                        // ControlResponseData had fields: behavior, updated_input, etc.
                                        // Now response is Value, so we can directly check for 'commands' field

                                        info!(
                                            "📋 Response value has 'commands' field: {}",
                                            response_value.get("commands").is_some()
                                        );

                                        // Check if this looks like an InitializeResponse by checking for 'commands' field
                                        if response_value.get("commands").is_some() {
                                            info!(
                                                "📋 Attempting to parse as InitializeResponse..."
                                            );
                                            match serde_json::from_value::<InitializeResponse>(
                                                response_value.clone(),
                                            ) {
                                                Ok(resp) => {
                                                    info!("📋 Initialize response: commands={:?}, models={:?}",
                                                        resp.commands.as_ref().map(|c: &Vec<_>| c.len()),
                                                        resp.models.as_ref().map(|m: &Vec<_>| m.len()));

                                                    // Store commands in server state
                                                    if let Some(cmds) = resp.commands.as_ref() {
                                                        self.server
                                                            .set_commands(cmds.clone())
                                                            .await;
                                                    }

                                                    return Ok(Some(resp));
                                                }
                                                Err(e) => {
                                                    warn!("Failed to parse InitializeResponse despite having commands field: {}", e);
                                                    warn!("Response value was: {}", response_value);
                                                }
                                            }
                                        } else {
                                            warn!("Response does not have 'commands' field, value: {}", response_value);
                                        }
                                    } else {
                                        warn!("response.response is None!");
                                    }

                                    return Ok(None);
                                }
                                ControlResponseType::Error => {
                                    let error_msg = ctrl_resp
                                        .response
                                        .error
                                        .unwrap_or_else(|| "Unknown error".to_string());
                                    error!("❌ Initialize request failed: {}", error_msg);
                                    return Err(format!("Initialize failed: {}", error_msg));
                                }
                            }
                        }
                    }
                    SdkMessage::ControlRequest(ctrl_req) => {
                        // Handle can_use_tool requests that might come during init
                        if let ControlRequestPayload::CanUseTool(_) = &ctrl_req.request {
                            info!("⚠️ Received can_use_tool request during initialize, auto-deny");
                            let fallback_response = SdkMessage::ControlResponse(ControlResponse {
                                response: ControlResponseInner {
                                    subtype: ControlResponseType::Error,
                                    request_id: ctrl_req.request_id.clone(),
                                    response: None,
                                    error: Some("Not initialized yet".to_string()),
                                },
                            });
                            let _ = self.server.send_message(fallback_response).await;
                        }
                    }
                    _ => {
                        debug!("Received other message type during initialize wait");
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }

        warn!("⚠️ Timeout waiting for initialize response, proceeding anyway");
        // Don't fail, just mark as initialized and continue
        *self.sdk_initialized.lock().await = true;
        Ok(None)
    }
}

impl Default for ClaudeSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeSessionManager {
    /// Extract text content from an AssistantContent message
    /// Note: Still used by wait_for_response_streaming (legacy API)
    #[allow(dead_code)]
    fn extract_assistant_content(assist: &AssistantContent) -> String {
        let mut text_parts = Vec::new();

        for block in &assist.content {
            if let ContentBlock::Text { text } = block {
                text_parts.push(text.clone());
            }
        }

        if text_parts.is_empty() {
            // If no text blocks, return a placeholder
            format!("[Assistant response: {}]", assist.id)
        } else {
            text_parts.join("\n")
        }
    }

    /// Extract text delta from a StreamEvent
    /// Parses the event JSON to extract text_delta content
    fn extract_text_delta(event: &serde_json::Value) -> Option<String> {
        // event format: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"xxx"}}
        if let Some(event_type) = event.get("type").and_then(|t| t.as_str()) {
            if event_type == "content_block_delta" {
                if let Some(delta) = event.get("delta") {
                    if let Some(delta_type) = delta.get("type").and_then(|t| t.as_str()) {
                        if delta_type == "text_delta" {
                            return delta
                                .get("text")
                                .and_then(|t| t.as_str())
                                .map(|s| s.to_string());
                        }
                    }
                }
            }
        }
        None
    }
}

// Global session manager (deprecated - use SessionRegistry instead)
#[allow(deprecated)]
static GLOBAL_SESSION_MANAGER: once_cell::sync::Lazy<Arc<ClaudeSessionManager>> =
    once_cell::sync::Lazy::new(|| Arc::new(ClaudeSessionManager::new()));

#[allow(deprecated)]
#[deprecated(note = "Use get_session_registry() instead")]
pub fn get_session_manager() -> Arc<ClaudeSessionManager> {
    GLOBAL_SESSION_MANAGER.clone()
}
