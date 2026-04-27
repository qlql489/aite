// Claude Session Manager (Stdin/Stdout)
//
// This module manages sessions with Claude Code CLI using stdin/stdout communication.

use crate::claude::message_normalizer::{normalize_cli_message, NormalizedMessageKind};
use crate::claude::permission_rules::{build_allow_rule, matches_allow_rule};
use crate::claude::process::stdin_process::StdinProcessManager;
use crate::claude::protocol::*;
use crate::claude::transport::ClaudeTransport;
use crate::claude::transports::stdin_transport::StdinTransport;
use crate::models::{
    ContentBlock as ModelContentBlock, Message, MessageContent, MessageRole, Session, TokenUsage,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
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

const EVENT_SUBAGENT_TOOL_USE: &str = "claude:subagent_tool_use";
const EVENT_SUBAGENT_TOOL_INPUT_DELTA: &str = "claude:subagent_tool_input_delta";
const EVENT_SUBAGENT_TOOL_RESULT_START: &str = "claude:subagent_tool_result_start";
const EVENT_SUBAGENT_TOOL_RESULT_DELTA: &str = "claude:subagent_tool_result_delta";
const EVENT_SUBAGENT_TOOL_RESULT_COMPLETE: &str = "claude:subagent_tool_result_complete";

#[derive(Debug, Clone, Default)]
struct SubagentStreamState {
    tool_use_ids_by_index: HashMap<(String, u64), String>,
    tool_result_ids_by_index: HashMap<(String, u64), String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SubagentToolUsePayload {
    session_id: String,
    parent_tool_use_id: String,
    tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed_time_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SubagentToolInputDeltaPayload {
    session_id: String,
    parent_tool_use_id: String,
    tool_use_id: String,
    delta: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SubagentToolResultStartPayload {
    session_id: String,
    parent_tool_use_id: String,
    tool_use_id: String,
    content: String,
    is_error: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SubagentToolResultDeltaPayload {
    session_id: String,
    parent_tool_use_id: String,
    tool_use_id: String,
    delta: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SubagentToolResultCompletePayload {
    session_id: String,
    parent_tool_use_id: String,
    tool_use_id: String,
}

#[derive(Debug, Clone, PartialEq)]
enum FrontendSubagentEvent {
    ToolUse(SubagentToolUsePayload),
    ToolInputDelta(SubagentToolInputDeltaPayload),
    ToolResultStart(SubagentToolResultStartPayload),
    ToolResultDelta(SubagentToolResultDeltaPayload),
    ToolResultComplete(SubagentToolResultCompletePayload),
}

/// Pending permission request waiting for user decision
pub struct PendingPermissionRequest {
    pub tool_name: String,
    pub input: serde_json::Value,
    pub tool_use_id: String,
    pub request_id: String,
    pub description: Option<String>,
    pub blocked_path: Option<String>,
    pub decision_reason: Option<String>,
    pub tx: Option<oneshot::Sender<PermissionDecision>>,
}

/// Permission decision from user
pub enum PermissionDecision {
    Approve {
        updated_input: Option<serde_json::Value>,
        allow_tools: Option<Vec<String>>,
    },
    Reject {
        reason: String,
        suggestion: Option<String>,
    },
    ChangeMode {
        mode: PermissionMode,
    },
}

/// Session manager for Claude Code CLI (stdin/stdout mode)
#[derive(Clone)]
pub struct StdinSessionManager {
    pub(crate) transport: Arc<StdinTransport>,
    pub(crate) process: Arc<StdinProcessManager>,
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
    /// Tool calls that were rejected (tool_use_id -> true)
    rejected_tool_calls: Arc<RwLock<std::collections::HashSet<String>>>,
    /// Whether the SDK has been initialized with initialize control request
    sdk_initialized: Arc<Mutex<bool>>,
    /// Background task handle
    message_handler_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Skills from system.init message
    skills: Arc<Mutex<Option<Vec<String>>>>,
    /// Commands from initialize response
    commands: Arc<Mutex<Option<Vec<crate::claude::protocol::CommandInfo>>>>,
}

impl StdinSessionManager {
    fn is_tool_result_user_message(user: &UserMessage) -> bool {
        match &user.message {
            UserMessageContent::Structured { content, .. } => content
                .iter()
                .any(|block| matches!(block, ContentBlock::ToolResult { .. })),
            UserMessageContent::Simple { .. } => false,
        }
    }

    async fn emit_commands_and_skills_updated(&self) {
        let Some(app) = self.get_app_handle().await else {
            return;
        };

        let Some(session_id) = self.get_external_session_id().await else {
            return;
        };

        let commands = self.commands.lock().await.clone();
        let skills = self.skills.lock().await.clone();

        info!(
            "📣 Emitting commands update to frontend: commands={}, cached_skills={} (not forwarded)",
            commands.as_ref().map(|v| v.len()).unwrap_or(0),
            skills.as_ref().map(|v| v.len()).unwrap_or(0)
        );

        let _ = app.emit(
            crate::commands::claude::EVENT_COMMANDS_UPDATED,
            serde_json::json!({
                "sessionId": session_id,
                "commands": commands,
            }),
        );
    }

    /// Create a new session manager
    pub fn new() -> Self {
        let transport = Arc::new(StdinTransport::new());
        let process = Arc::new(StdinProcessManager::new(transport.clone()));

        Self {
            transport,
            process,
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
            rejected_tool_calls: Arc::new(RwLock::new(std::collections::HashSet::new())),
            sdk_initialized: Arc::new(Mutex::new(false)),
            message_handler_handle: Arc::new(Mutex::new(None)),
            skills: Arc::new(Mutex::new(None)),
            commands: Arc::new(Mutex::new(None)),
        }
    }

    /// Start background message handler
    /// Continuously receives all messages from CLI and pushes them to frontend
    pub async fn start_message_handler(&self) -> Result<(), String> {
        info!("🚀 Starting background message handler for Stdio mode");

        let manager = self.clone();
        let handle = tokio::spawn(async move {
            info!("📥 Background message handler started");
            let mut subagent_stream_state = SubagentStreamState::default();

            loop {
                match manager.transport.recv_message().await {
                    Some(msg) => {
                        match &msg {
                            // === Internal messages ===
                            SdkMessage::System(sys) => {
                                info!(
                                    "⚙️ System message: subtype={:?}, session_id={:?}",
                                    sys.subtype, sys.session_id
                                );

                                // Store skills
                                if sys.skills.is_some() {
                                    *manager.skills.lock().await = sys.skills.clone();
                                    info!(
                                        "✅ Stored {} skills from init",
                                        sys.skills.as_ref().map(|v| v.len()).unwrap_or(0)
                                    );
                                    manager.emit_commands_and_skills_updated().await;
                                }

                                // Handle session ID update
                                if let Some(cli_session_id) = &sys.session_id {
                                    let external_id = manager.get_external_session_id().await;
                                    if let Some(ext_id) = external_id {
                                        if ext_id != *cli_session_id {
                                            let registry = crate::claude::session_registry::get_session_registry();
                                            if let Some(old_id) = registry
                                                .update_session_id(&ext_id, cli_session_id)
                                                .await
                                            {
                                                manager
                                                    .set_external_session_id(
                                                        cli_session_id.to_string(),
                                                    )
                                                    .await;
                                                if let Some(app) = manager.get_app_handle().await {
                                                    let _ = app.emit(
                                                        "session-id-updated",
                                                        serde_json::json!({
                                                            "oldSessionId": old_id,
                                                            "newSessionId": cli_session_id
                                                        }),
                                                    );
                                                    let _ = app.emit(
                                                        "claude:connection_status",
                                                        serde_json::json!({
                                                            "sessionId": cli_session_id,
                                                            "status": "connected"
                                                        }),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }

                                if let Some(app) = manager.get_app_handle().await {
                                    let session_id = match &sys.session_id {
                                        Some(session_id) => Some(session_id.clone()),
                                        None => manager.get_external_session_id().await,
                                    };

                                    if let Some(session_id) = session_id {
                                        match sys.subtype.as_deref() {
                                            Some("status") => {
                                                let status = match sys.status.as_deref() {
                                                    Some("compacting") => "compacting",
                                                    Some("running") => "running",
                                                    Some("idle") => "idle",
                                                    _ => "idle",
                                                };

                                                let _ = app.emit(
                                                    crate::commands::claude::EVENT_SESSION_STATUS,
                                                    serde_json::json!({
                                                        "sessionId": session_id,
                                                        "status": status,
                                                    }),
                                                );
                                            }
                                            Some("compact_boundary") => {
                                                let _ = app.emit(
                                                    crate::commands::claude::EVENT_SESSION_STATUS,
                                                    serde_json::json!({
                                                        "sessionId": session_id,
                                                        "status": "idle",
                                                    }),
                                                );
                                            }
                                            _ => {}
                                        }

                                        if let Some(display_text) = sys.display_text() {
                                            let message = Message::new(
                                                session_id.clone(),
                                                MessageRole::System,
                                                MessageContent::Text(display_text),
                                            );
                                            let _ = app.emit(
                                                "claude:message",
                                                serde_json::to_value(&message).ok(),
                                            );
                                        }
                                    }
                                }
                            }

                            SdkMessage::ControlResponse(ctrl_resp) => {
                                info!(
                                    "✅ Control response: request_id={}, subtype={:?}",
                                    ctrl_resp.response.request_id, ctrl_resp.response.subtype
                                );

                                // Store commands from initialize response
                                if matches!(
                                    ctrl_resp.response.subtype,
                                    ControlResponseType::Success
                                ) {
                                    if let Some(response_value) =
                                        ctrl_resp.response.response.clone()
                                    {
                                        if response_value.get("commands").is_some() {
                                            match serde_json::to_string_pretty(&response_value) {
                                                Ok(pretty_json) => {
                                                    info!(
                                                        "📋 Initialize raw response:\n{}",
                                                        pretty_json
                                                    );
                                                }
                                                Err(e) => {
                                                    warn!(
                                                        "Failed to pretty print initialize response: {}",
                                                        e
                                                    );
                                                    info!(
                                                        "📋 Initialize raw response (compact): {}",
                                                        response_value
                                                    );
                                                }
                                            }
                                            match serde_json::from_value::<InitializeResponse>(
                                                response_value,
                                            ) {
                                                Ok(resp) => {
                                                    let cmd_count =
                                                        resp.commands.as_ref().map(|c| c.len());
                                                    if let Some(cmds) = resp.commands {
                                                        *manager.commands.lock().await = Some(cmds);
                                                    }
                                                    info!(
                                                        "✅ Stored {} commands from initialize",
                                                        cmd_count.unwrap_or(0)
                                                    );
                                                    manager
                                                        .emit_commands_and_skills_updated()
                                                        .await;
                                                }
                                                Err(e) => {
                                                    warn!(
                                                        "Failed to parse InitializeResponse: {}",
                                                        e
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            SdkMessage::ControlRequest(ctrl_req) => {
                                info!(
                                    "🎛️ Control request: subtype={:?}",
                                    std::mem::discriminant(&ctrl_req.request)
                                );

                                // Handle can_use_tool permission requests
                                if let ControlRequestPayload::CanUseTool(_) = &ctrl_req.request {
                                    if let Err(e) = manager.emit_permission_request(&ctrl_req).await
                                    {
                                        error!(
                                            "Failed to emit permission request to frontend: {}",
                                            e
                                        );
                                    }
                                }
                            }

                            // === User-facing messages - push to frontend ===
                            SdkMessage::Assistant(assist) => {
                                info!("🤖 Assistant message");
                                if let Some(app) = manager.get_app_handle().await {
                                    // Use message's own session_id to ensure correct routing
                                    let message = manager
                                        .create_assistant_message(&assist.session_id, assist);
                                    let _ = app.emit(
                                        "claude:message",
                                        serde_json::to_value(&message).ok(),
                                    );
                                }
                            }

                            SdkMessage::User(user) => {
                                info!("👤 User message (tool result)");
                                if let Some(app) = manager.get_app_handle().await {
                                    if user.uuid.is_some()
                                        && !Self::is_tool_result_user_message(user)
                                    {
                                        let _ = app.emit(
                                            "claude:user_checkpoint",
                                            serde_json::json!({
                                                "sessionId": user.session_id,
                                                "checkpointUuid": user.uuid.clone(),
                                            }),
                                        );
                                    }

                                    // Use message's own session_id to ensure correct routing
                                    if let Some(message) =
                                        manager.create_user_message(&user.session_id, user)
                                    {
                                        let _ = app.emit(
                                            "claude:message",
                                            serde_json::to_value(&message).ok(),
                                        );
                                    }
                                }
                            }

                            SdkMessage::StreamEvent(se) => {
                                // Stream events are pushed via claude:stream_data event
                                // Use the session_id from the message itself, not from manager state
                                if let Some(app) = manager.get_app_handle().await {
                                    for frontend_event in Self::translate_subagent_stream_event(
                                        se,
                                        &mut subagent_stream_state,
                                    ) {
                                        match frontend_event {
                                            FrontendSubagentEvent::ToolUse(payload) => {
                                                let _ = app.emit(EVENT_SUBAGENT_TOOL_USE, payload);
                                            }
                                            FrontendSubagentEvent::ToolInputDelta(payload) => {
                                                let _ = app.emit(EVENT_SUBAGENT_TOOL_INPUT_DELTA, payload);
                                            }
                                            FrontendSubagentEvent::ToolResultStart(payload) => {
                                                let _ = app.emit(EVENT_SUBAGENT_TOOL_RESULT_START, payload);
                                            }
                                            FrontendSubagentEvent::ToolResultDelta(payload) => {
                                                let _ = app.emit(EVENT_SUBAGENT_TOOL_RESULT_DELTA, payload);
                                            }
                                            FrontendSubagentEvent::ToolResultComplete(payload) => {
                                                let _ = app.emit(EVENT_SUBAGENT_TOOL_RESULT_COMPLETE, payload);
                                            }
                                        }
                                    }

                                    if let Some(text_delta) = Self::extract_text_delta(&se.event) {
                                        // Use message's own session_id to ensure correct routing
                                        let payload = serde_json::json!({
                                            "sessionId": se.session_id,
                                            "data": text_delta
                                        });
                                        let _ = app.emit("claude:stream_data", payload);
                                    }

                                    if let Some(usage) =
                                        Self::extract_message_delta_usage(&se.event)
                                    {
                                        let payload = serde_json::json!({
                                            "sessionId": se.session_id,
                                            "usage": usage,
                                        });
                                        let _ = app.emit("claude:stream_usage", payload);
                                    }
                                }
                            }

                            SdkMessage::ToolProgress(progress) => {
                                if let Some(app) = manager.get_app_handle().await {
                                    if let Some(payload) =
                                        Self::translate_subagent_tool_progress(progress)
                                    {
                                        let _ = app.emit(EVENT_SUBAGENT_TOOL_USE, payload);
                                    }
                                }
                            }

                            SdkMessage::Result(res) => {
                                info!("✅ Result message: success={}", res.subtype);
                                if let Some(app) = manager.get_app_handle().await {
                                    // Use message's own session_id to ensure correct routing
                                    let payload = serde_json::json!({
                                        "sessionId": res.session_id,
                                        "message": res
                                    });
                                    let _ = app.emit("claude:stream_end", payload);
                                }
                            }

                            _ => {
                                debug!("Other message type: {:?}", std::mem::discriminant(&msg));
                            }
                        }
                    }
                    None => {
                        warn!("⚠️ Transport disconnected, background handler exiting");
                        break;
                    }
                }
            }

            info!("📥 Background message handler ended");
        });

        *self.message_handler_handle.lock().await = Some(handle);
        Ok(())
    }

    pub async fn set_thinking_level(&self, level: String) {
        self.process.set_thinking_level(level).await;
    }

    pub async fn set_model(&self, model: Option<String>) {
        self.process.set_model(model).await;
    }

    pub async fn set_provider_env(&self, provider_env: Option<crate::models::SessionProviderEnv>) {
        self.process.set_provider_env(provider_env).await;
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

    /// Get the resume session ID
    pub async fn get_resume_session_id(&self) -> Option<String> {
        self.resume_session_id.lock().await.clone()
    }

    /// Set the permission mode
    pub async fn set_permission_mode(&self, mode: PermissionMode) {
        info!("Setting permission mode: {:?}", mode);
        *self.permission_mode.lock().await = mode;

        let msg = SdkMessage::ControlRequest(ControlRequest {
            request_id: Uuid::new_v4().to_string(),
            request: ControlRequestPayload::SetPermissionMode {
                mode: mode.as_str().to_string(),
            },
        });
        if let Err(e) = self.transport.send_message(msg).await {
            warn!("Failed to send set_permission_mode to CLI: {}", e);
        }
    }

    pub async fn rewind_files(&self, user_message_id: String) -> Result<(), String> {
        let msg = SdkMessage::ControlRequest(ControlRequest {
            request_id: Uuid::new_v4().to_string(),
            request: ControlRequestPayload::RewindFiles { user_message_id },
        });

        self.transport
            .send_message(msg)
            .await
            .map_err(|e| format!("Failed to send rewind_files request: {}", e))
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
        info!(
            "🔐 [PERMISSION] handle_permission_request called for request_id: {}",
            ctrl_req.request_id
        );

        if let ControlRequestPayload::CanUseTool(tool_req) = &ctrl_req.request {
            if self
                .is_permission_rule_allowed(&tool_req.tool_name, &tool_req.input)
                .await
            {
                info!(
                    "🔐 [PERMISSION] Auto-approving remembered permission: tool={} request_id={}",
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
                        tx: Some(tx),
                    },
                );
            }

            // Get external session ID
            let session_id = self.get_external_session_id().await.ok_or_else(|| {
                error!("❌ [PERMISSION] External session ID not set");
                "External session ID not set".to_string()
            })?;

            info!("✅ [PERMISSION] External session ID: {}", session_id);

            // Get AppHandle
            let app_handle = self.get_app_handle().await.ok_or_else(|| {
                error!("❌ [PERMISSION] App handle not set");
                "App handle not set".to_string()
            })?;

            info!("✅ [PERMISSION] App handle obtained");

            // Build permission request data
            let permission_data = serde_json::json!({
                "sessionId": session_id,
                "requestId": ctrl_req.request_id,
                "permission": {
                    "request_id": ctrl_req.request_id,
                    "type": "tool_use",
                    "description": tool_req.description.clone().unwrap_or_else(|| format!("Use tool: {}", tool_req.tool_name)),
                    "tool_use_id": tool_req.tool_use_id,  // 顶层添加 tool_use_id 以便前端关联工具调用
                    "params": {
                        "tool_name": tool_req.tool_name,
                        "input": tool_req.input,
                        "tool_use_id": tool_req.tool_use_id,
                        "blocked_path": tool_req.blocked_path,
                        "decision_reason": tool_req.decision_reason,
                    }
                }
            });

            info!(
                "📤 [PERMISSION] Emitting permission-request event to frontend: {}",
                ctrl_req.request_id
            );

            // Emit event to frontend
            app_handle
                .emit("permission-request", permission_data)
                .map_err(|e| {
                    error!("❌ [PERMISSION] Failed to emit permission request: {}", e);
                    format!("Failed to emit permission request: {}", e)
                })?;

            info!(
                "✅ [PERMISSION] Permission request event emitted successfully for {}",
                ctrl_req.request_id
            );

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
            info!(
                "🔐 [PERMISSION] Remembered permission rule for session: {}",
                rule
            );
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

    pub async fn emit_permission_request(&self, ctrl_req: &ControlRequest) -> Result<(), String> {
        info!(
            "🔐 [PERMISSION] emit_permission_request called for request_id: {}",
            ctrl_req.request_id
        );

        let ControlRequestPayload::CanUseTool(tool_req) = &ctrl_req.request else {
            return Err("Not a can_use_tool request".to_string());
        };

        if self
            .is_permission_rule_allowed(&tool_req.tool_name, &tool_req.input)
            .await
        {
            info!(
                "🔐 [PERMISSION] Auto-approving remembered permission: tool={} request_id={}",
                tool_req.tool_name, ctrl_req.request_id
            );
            return self
                .send_permission_decision(
                    ctrl_req.request_id.clone(),
                    PermissionDecision::Approve {
                        updated_input: None,
                        allow_tools: None,
                    },
                )
                .await;
        }

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
                    tx: None,
                },
            );
        }

        let session_id = self.get_external_session_id().await.ok_or_else(|| {
            error!("❌ [PERMISSION] External session ID not set");
            "External session ID not set".to_string()
        })?;

        let app_handle = self.get_app_handle().await.ok_or_else(|| {
            error!("❌ [PERMISSION] App handle not set");
            "App handle not set".to_string()
        })?;

        let permission_data = serde_json::json!({
            "sessionId": session_id,
            "requestId": ctrl_req.request_id,
            "permission": {
                "request_id": ctrl_req.request_id,
                "type": "tool_use",
                "description": tool_req.description.clone().unwrap_or_else(|| format!("Use tool: {}", tool_req.tool_name)),
                "tool_use_id": tool_req.tool_use_id,
                "params": {
                    "tool_name": tool_req.tool_name,
                    "input": tool_req.input,
                    "tool_use_id": tool_req.tool_use_id,
                    "blocked_path": tool_req.blocked_path,
                    "decision_reason": tool_req.decision_reason,
                }
            }
        });

        app_handle
            .emit("permission-request", permission_data)
            .map_err(|e| {
                error!("❌ [PERMISSION] Failed to emit permission request: {}", e);
                format!("Failed to emit permission request: {}", e)
            })?;

        info!(
            "✅ [PERMISSION] Permission request event emitted successfully for {}",
            ctrl_req.request_id
        );

        Ok(())
    }

    /// Send ControlResponse to Claude CLI based on user decision
    pub async fn resolve_permission_request(
        &self,
        request_id: String,
        decision: PermissionDecision,
    ) -> Result<(), String> {
        let tx = {
            let mut pending = self.pending_permissions.write().await;
            let entry = pending
                .get_mut(&request_id)
                .ok_or_else(|| format!("Permission request not found: {}", request_id))?;

            entry
                .tx
                .take()
                .ok_or_else(|| format!("Permission request already resolved: {}", request_id))?
        };

        tx.send(decision)
            .map_err(|_| format!("Permission request channel closed: {}", request_id))?;

        info!(
            "Permission decision sent through channel for {}",
            request_id
        );

        Ok(())
    }

    /// Send ControlResponse to Claude CLI based on user decision
    pub async fn send_permission_decision(
        &self,
        request_id: String,
        decision: PermissionDecision,
    ) -> Result<(), String> {
        info!("Sending permission decision to CLI for {}", request_id);

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

        // Remove the pending request and get the original input and tool_use_id
        let (original_input, tool_use_id) = {
            let mut pending = self.pending_permissions.write().await;
            let entry = pending.remove(&request_id);
            entry
                .map(|p| (p.input, p.tool_use_id))
                .ok_or_else(|| format!("Permission request not found: {}", request_id))?
        };

        // If this is a reject decision, track the rejected tool call
        if matches!(decision, PermissionDecision::Reject { .. }) {
            self.rejected_tool_calls
                .write()
                .await
                .insert(tool_use_id.clone());
            info!("🚫 Tool call marked as rejected: {}", tool_use_id);
        }

        // Build and send the ControlResponse
        let response = match decision_for_response {
            PermissionDecision::Approve {
                updated_input,
                allow_tools,
            } => {
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
                // 构建详细的拒绝消息，类似 happy-cli 的格式
                let base_message = "The user doesn't want to proceed with this tool use. The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). STOP what you are doing and wait for the user to tell you how to proceed.";

                let message = if let Some(sug) = suggestion {
                    format!("{}. {}. {}", base_message, reason, sug)
                } else if !reason.is_empty() && reason != "Rejected by user" {
                    format!("{}. {}", base_message, reason)
                } else {
                    base_message.to_string()
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
        self.transport
            .send_message(response)
            .await
            .map_err(|e| format!("Failed to send control response: {}", e))?;

        info!("Permission decision sent to CLI for {}", request_id);
        Ok(())
    }

    /// Launch Claude CLI
    pub async fn launch_cli(&self) -> Result<(), String> {
        info!("Launching Claude CLI (stdin/stdout mode)...");

        // Get resume session ID if set
        let resume_id = self.get_resume_session_id().await;

        // Launch CLI with or without --resume parameter
        if let Some(session_id) = resume_id {
            info!("🔄 Launching CLI with --resume: {}", session_id);
            self.process
                .launch_with_resume(session_id)
                .await
                .map_err(|e| format!("Failed to launch CLI with --resume: {}", e))?;
        } else {
            info!("🚀 Launching CLI (new session)");
            self.process
                .launch()
                .await
                .map_err(|e| format!("Failed to launch CLI: {}", e))?;
        }

        info!("Claude CLI launched (stdin/stdout mode)");

        // Start background message handler first so init/system messages are consumed
        // as soon as the CLI begins emitting them.
        self.start_message_handler().await?;

        // Wait a bit for the process to start
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Send initialize control request
        info!("📤 Sending initialize control request...");
        self.send_initialize_request().await?;

        Ok(())
    }

    /// Send initialize control request to CLI
    /// Note: Response is handled asynchronously in send_message_streaming_with_messages
    async fn send_initialize_request(&self) -> Result<(), String> {
        // Check if already initialized
        {
            let initialized = self.sdk_initialized.lock().await;
            if *initialized {
                info!("ℹ️ SDK already initialized, skipping");
                return Ok(());
            }
        }

        let init_request = SdkMessage::ControlRequest(ControlRequest {
            request_id: Uuid::new_v4().to_string(),
            request: ControlRequestPayload::Initialize(InitializeRequest {
                hooks: None,
                sdk_mcp_servers: None,
                json_schema: None,
                system_prompt: None,
                append_system_prompt: None,
                agents: None,
            }),
        });

        self.transport
            .send_message(init_request)
            .await
            .map_err(|e| format!("Failed to send initialize request: {}", e))?;

        // Mark as initialized and return immediately (response handled asynchronously)
        *self.sdk_initialized.lock().await = true;
        info!("Initialize request sent");
        Ok(())
    }

    /// Send a message to Claude CLI
    pub async fn send_message(&self, content: String) -> Result<(), String> {
        let external_id = self
            .get_external_session_id()
            .await
            .ok_or_else(|| "No external session ID set".to_string())?;

        // Get CLI's internal session ID for the message
        let cli_session_id = self
            .transport
            .session_id()
            .await
            .ok_or_else(|| "CLI not connected yet".to_string())?;

        let user_msg = SdkMessage::User(UserMessage {
            message: UserMessageContent::Simple {
                role: "user".to_string(),
                content,
            },
            parent_tool_use_id: None,
            session_id: cli_session_id,
            uuid: Some(Uuid::new_v4().to_string()),
            is_synthetic: None,
        });

        self.transport
            .send_message(user_msg)
            .await
            .map_err(|e| format!("Failed to send message: {}", e))?;

        // Mark stream as active
        self.active_streams
            .write()
            .await
            .insert(external_id.clone());

        Ok(())
    }

    /// Check if connected to CLI
    pub async fn is_connected(&self) -> bool {
        self.transport.is_connected().await
    }

    /// Get CLI session ID
    pub async fn get_cli_session_id(&self) -> Option<String> {
        self.transport.session_id().await
    }

    /// Stop the CLI process
    pub async fn stop(&self) -> Result<(), String> {
        info!("Stopping session manager...");
        self.process
            .stop()
            .await
            .map_err(|e| format!("Failed to stop process: {}", e))?;
        info!("Session manager stopped");
        Ok(())
    }

    /// Stop streaming for a session
    pub async fn stop_streaming(&self, session_id: String) -> Result<(), String> {
        info!("Stopping streaming for session: {}", session_id);

        // Send interrupt control request to CLI
        let interrupt_msg = SdkMessage::ControlRequest(ControlRequest {
            request_id: Uuid::new_v4().to_string(),
            request: ControlRequestPayload::Interrupt,
        });

        self.transport
            .send_message(interrupt_msg)
            .await
            .map_err(|e| format!("Failed to send interrupt request: {}", e))?;

        info!("Sent interrupt request to CLI");

        // Remove from active streams
        self.active_streams.write().await.remove(&session_id);

        info!("Streaming stopped");
        Ok(())
    }

    /// Send message with streaming (similar to WebSocket mode)
    /// Note: Response messages are handled by the background message handler (start_message_handler)
    /// This method only sends the user message to the CLI.
    pub async fn send_message_streaming_with_messages(
        &self,
        session_id: String,
        content: String,
        content_blocks: Option<Vec<ContentBlock>>,
        _data_callback: StreamDataCallback,
        _message_callback: StreamMessageCallback,
        _complete_callback: StreamCompleteCallback,
    ) -> Result<Message, String> {
        info!("🚀 send_message_streaming_with_messages (Stdio mode)");
        info!(
            "📋 Session ID: {}, Content length: {}",
            session_id,
            content.len()
        );

        let external_id = self.get_external_session_id().await.ok_or_else(|| {
            error!("❌ No external session ID set");
            "No external session ID set".to_string()
        })?;

        // Get CLI's internal session ID
        let cli_session_id = self.transport.session_id().await;
        let cli_session_id = cli_session_id.unwrap_or_else(|| {
            warn!("⚠️ CLI session ID not set yet, using external ID as fallback");
            external_id.clone()
        });

        // Create and send user message
        let user_message_content = match content_blocks {
            Some(blocks) if !blocks.is_empty() => UserMessageContent::Structured {
                role: "user".to_string(),
                content: blocks,
            },
            _ => UserMessageContent::Simple {
                role: "user".to_string(),
                content,
            },
        };

        let user_msg = SdkMessage::User(UserMessage {
            message: user_message_content,
            parent_tool_use_id: None,
            session_id: cli_session_id,
            uuid: Some(Uuid::new_v4().to_string()),
            is_synthetic: None,
        });

        self.transport
            .send_message(user_msg)
            .await
            .map_err(|e| format!("Failed to send message: {}", e))?;

        // Mark stream as active
        self.active_streams
            .write()
            .await
            .insert(external_id.clone());

        // Note: Response handling is done by background message handler
        // Stream events and messages are pushed to frontend via events

        // Return a placeholder message (actual response will come via events)
        Ok(Message::new(
            external_id,
            MessageRole::Assistant,
            MessageContent::Text("Message sent. Response will arrive via events.".to_string()),
        ))
    }

    /// Extract text delta from stream event JSON
    /// Event format: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"xxx"}}
    fn extract_text_delta(event: &serde_json::Value) -> Option<String> {
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

    fn translate_subagent_tool_progress(
        progress: &ToolProgressMessage,
    ) -> Option<SubagentToolUsePayload> {
        Some(SubagentToolUsePayload {
            session_id: progress.session_id.clone(),
            parent_tool_use_id: progress.parent_tool_use_id.clone()?,
            tool_use_id: progress.tool_use_id.clone(),
            tool_name: Some(progress.tool_name.clone()),
            input: None,
            elapsed_time_seconds: Some(progress.elapsed_time_seconds),
        })
    }

    fn translate_subagent_stream_event(
        stream_event: &StreamEventMessage,
        state: &mut SubagentStreamState,
    ) -> Vec<FrontendSubagentEvent> {
        let Some(parent_tool_use_id) = stream_event.parent_tool_use_id.clone() else {
            return Vec::new();
        };

        let event_type = stream_event
            .event
            .get("type")
            .and_then(|value| value.as_str());
        let index = stream_event
            .event
            .get("index")
            .and_then(|value| value.as_u64());

        match event_type {
            Some("content_block_start") => {
                Self::translate_subagent_content_block_start(
                    &stream_event.session_id,
                    &parent_tool_use_id,
                    index,
                    stream_event.event.get("content_block"),
                    state,
                )
            }
            Some("content_block_delta") => Self::translate_subagent_content_block_delta(
                &stream_event.session_id,
                &parent_tool_use_id,
                index,
                stream_event.event.get("delta"),
                state,
            ),
            Some("content_block_stop") => Self::translate_subagent_content_block_stop(
                &stream_event.session_id,
                &parent_tool_use_id,
                index,
                state,
            ),
            _ => Vec::new(),
        }
    }

    fn translate_subagent_content_block_start(
        session_id: &str,
        parent_tool_use_id: &str,
        index: Option<u64>,
        content_block: Option<&Value>,
        state: &mut SubagentStreamState,
    ) -> Vec<FrontendSubagentEvent> {
        let Some(index) = index else {
            return Vec::new();
        };
        let Some(content_block) = content_block else {
            return Vec::new();
        };
        let Some(block_type) = content_block.get("type").and_then(|value| value.as_str()) else {
            return Vec::new();
        };

        match block_type {
            "tool_use" => {
                let Some(tool_use_id) = content_block
                    .get("id")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string()) else {
                    return Vec::new();
                };
                let tool_name = content_block
                    .get("name")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());
                let input = content_block.get("input").cloned();

                state.tool_use_ids_by_index.insert(
                    (session_id.to_string(), index),
                    tool_use_id.clone(),
                );

                vec![FrontendSubagentEvent::ToolUse(SubagentToolUsePayload {
                    session_id: session_id.to_string(),
                    parent_tool_use_id: parent_tool_use_id.to_string(),
                    tool_use_id,
                    tool_name,
                    input,
                    elapsed_time_seconds: None,
                })]
            }
            block_type if Self::is_tool_result_block_type(block_type) => {
                let Some(tool_use_id) = content_block
                    .get("tool_use_id")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string()) else {
                    return Vec::new();
                };
                state.tool_result_ids_by_index.insert(
                    (session_id.to_string(), index),
                    tool_use_id.clone(),
                );

                let content = Self::tool_result_content_to_string(content_block.get("content"));
                if content.is_empty() {
                    return Vec::new();
                }

                let is_error = content_block
                    .get("is_error")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);

                vec![FrontendSubagentEvent::ToolResultStart(
                    SubagentToolResultStartPayload {
                        session_id: session_id.to_string(),
                        parent_tool_use_id: parent_tool_use_id.to_string(),
                        tool_use_id,
                        content,
                        is_error,
                    },
                )]
            }
            _ => Vec::new(),
        }
    }

    fn translate_subagent_content_block_delta(
        session_id: &str,
        parent_tool_use_id: &str,
        index: Option<u64>,
        delta: Option<&Value>,
        state: &mut SubagentStreamState,
    ) -> Vec<FrontendSubagentEvent> {
        let Some(index) = index else {
            return Vec::new();
        };
        let Some(delta) = delta else {
            return Vec::new();
        };
        let Some(delta_type) = delta.get("type").and_then(|value| value.as_str()) else {
            return Vec::new();
        };

        match delta_type {
            "input_json_delta" => {
                let Some(tool_use_id) = state
                    .tool_use_ids_by_index
                    .get(&(session_id.to_string(), index))
                    .cloned() else {
                    return Vec::new();
                };
                let Some(partial_json) = delta
                    .get("partial_json")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string()) else {
                    return Vec::new();
                };

                vec![FrontendSubagentEvent::ToolInputDelta(
                    SubagentToolInputDeltaPayload {
                        session_id: session_id.to_string(),
                        parent_tool_use_id: parent_tool_use_id.to_string(),
                        tool_use_id,
                        delta: partial_json,
                    },
                )]
            }
            "text_delta" => {
                let Some(tool_use_id) = state
                    .tool_result_ids_by_index
                    .get(&(session_id.to_string(), index))
                    .cloned() else {
                    return Vec::new();
                };
                let Some(text) = delta
                    .get("text")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string()) else {
                    return Vec::new();
                };

                vec![FrontendSubagentEvent::ToolResultDelta(
                    SubagentToolResultDeltaPayload {
                        session_id: session_id.to_string(),
                        parent_tool_use_id: parent_tool_use_id.to_string(),
                        tool_use_id,
                        delta: text,
                    },
                )]
            }
            _ => Vec::new(),
        }
    }

    fn translate_subagent_content_block_stop(
        session_id: &str,
        parent_tool_use_id: &str,
        index: Option<u64>,
        state: &mut SubagentStreamState,
    ) -> Vec<FrontendSubagentEvent> {
        let Some(index) = index else {
            return Vec::new();
        };

        let key = (session_id.to_string(), index);
        if let Some(tool_use_id) = state.tool_result_ids_by_index.remove(&key) {
            return vec![FrontendSubagentEvent::ToolResultComplete(
                SubagentToolResultCompletePayload {
                    session_id: session_id.to_string(),
                    parent_tool_use_id: parent_tool_use_id.to_string(),
                    tool_use_id,
                },
            )];
        }

        Vec::new()
    }

    fn is_tool_result_block_type(block_type: &str) -> bool {
        matches!(
            block_type,
            "tool_result"
                | "web_search_tool_result"
                | "web_fetch_tool_result"
                | "code_execution_tool_result"
                | "bash_code_execution_tool_result"
                | "text_editor_code_execution_tool_result"
                | "mcp_tool_result"
        )
    }

    fn tool_result_content_to_string(content: Option<&Value>) -> String {
        match content {
            Some(Value::String(value)) => value.clone(),
            Some(Value::Null) | None => String::new(),
            Some(other) => serde_json::to_string(other).unwrap_or_default(),
        }
    }

    /// Extract usage payload from message_delta stream events.
    fn extract_message_delta_usage(event: &serde_json::Value) -> Option<crate::models::TokenUsage> {
        if event.get("type").and_then(|t| t.as_str()) != Some("message_delta") {
            return None;
        }

        let usage = event.get("usage")?;
        serde_json::from_value::<crate::models::TokenUsage>(usage.clone()).ok()
    }

    /// Create a Message from an AssistantMessage
    fn create_assistant_message(&self, session_id: &str, assist: &AssistantMessage) -> Message {
        // Extract content blocks
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
                    // Store complete tool_use info including id, name and input
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

        // Combine text content
        let text_content = if text_parts.is_empty() {
            format!("[Assistant response: {}]", assist.message.id)
        } else {
            text_parts.join("\n")
        };

        let (text_content, role) = normalize_cli_message(&text_content)
            .map(|normalized| normalized.role_or(MessageRole::Assistant))
            .unwrap_or((text_content, MessageRole::Assistant));

        // Create Message
        let mut msg = Message::new(
            session_id.to_string(),
            role,
            MessageContent::Text(text_content),
        );

        // Add structured content blocks
        if !blocks.is_empty() {
            msg.content = MessageContent::Blocks(blocks);
        }

        msg.parent_tool_use_id = assist.parent_tool_use_id.clone();

        // Add model and token usage info
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
        // Check if this is a tool result message
        match &user.message {
            UserMessageContent::Structured { role: _, content } => {
                // Find tool_result blocks
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
                            msg.parent_tool_use_id = user.parent_tool_use_id.clone();

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
                            msg.parent_tool_use_id = user.parent_tool_use_id.clone();
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

    /// Set working directory
    pub async fn set_working_directory(&self, path: PathBuf) -> Result<(), String> {
        self.process.set_working_directory(path).await;
        Ok(())
    }

    /// Receive messages from CLI (for background processing)
    pub async fn recv_message(&self) -> Option<SdkMessage> {
        self.transport.recv_message().await
    }

    /// Get commands (from initialize) and skills (from system.init)
    pub async fn get_commands_and_skills(
        &self,
    ) -> (
        Option<Vec<crate::claude::protocol::CommandInfo>>,
        Option<Vec<String>>,
    ) {
        let commands = self.commands.lock().await.clone();
        let skills = self.skills.lock().await.clone();

        info!(
            "📤 get_commands_and_skills called (Stdio mode): returning {} commands; cached_skills={} (not forwarded)",
            commands.as_ref().map(|v| v.len()).unwrap_or(0),
            skills.as_ref().map(|v| v.len()).unwrap_or(0)
        );
        (commands, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_stream_event(
        session_id: &str,
        parent_tool_use_id: Option<&str>,
        event: Value,
    ) -> StreamEventMessage {
        StreamEventMessage {
            event,
            parent_tool_use_id: parent_tool_use_id.map(|value| value.to_string()),
            uuid: "uuid_1".to_string(),
            session_id: session_id.to_string(),
        }
    }

    #[test]
    fn translates_subagent_tool_use_event() {
        let mut state = SubagentStreamState::default();
        let stream_event = make_stream_event(
            "session_1",
            Some("task_1"),
            json!({
                "type": "content_block_start",
                "index": 2,
                "content_block": {
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "Read",
                    "input": { "file_path": "src/main.ts" }
                }
            }),
        );

        let translated =
            StdinSessionManager::translate_subagent_stream_event(&stream_event, &mut state);

        assert_eq!(
            translated,
            vec![FrontendSubagentEvent::ToolUse(SubagentToolUsePayload {
                session_id: "session_1".to_string(),
                parent_tool_use_id: "task_1".to_string(),
                tool_use_id: "toolu_1".to_string(),
                tool_name: Some("Read".to_string()),
                input: Some(json!({ "file_path": "src/main.ts" })),
                elapsed_time_seconds: None,
            })]
        );
    }

    #[test]
    fn translates_subagent_tool_result_lifecycle() {
        let mut state = SubagentStreamState::default();
        let start_event = make_stream_event(
            "session_1",
            Some("task_1"),
            json!({
                "type": "content_block_start",
                "index": 4,
                "content_block": {
                    "type": "tool_result",
                    "tool_use_id": "toolu_1",
                    "content": "partial result",
                    "is_error": false
                }
            }),
        );

        let start =
            StdinSessionManager::translate_subagent_stream_event(&start_event, &mut state);
        assert_eq!(
            start,
            vec![FrontendSubagentEvent::ToolResultStart(
                SubagentToolResultStartPayload {
                    session_id: "session_1".to_string(),
                    parent_tool_use_id: "task_1".to_string(),
                    tool_use_id: "toolu_1".to_string(),
                    content: "partial result".to_string(),
                    is_error: false,
                },
            )]
        );

        let delta_event = make_stream_event(
            "session_1",
            Some("task_1"),
            json!({
                "type": "content_block_delta",
                "index": 4,
                "delta": {
                    "type": "text_delta",
                    "text": " + more"
                }
            }),
        );

        let delta =
            StdinSessionManager::translate_subagent_stream_event(&delta_event, &mut state);
        assert_eq!(
            delta,
            vec![FrontendSubagentEvent::ToolResultDelta(
                SubagentToolResultDeltaPayload {
                    session_id: "session_1".to_string(),
                    parent_tool_use_id: "task_1".to_string(),
                    tool_use_id: "toolu_1".to_string(),
                    delta: " + more".to_string(),
                },
            )]
        );

        let stop_event = make_stream_event(
            "session_1",
            Some("task_1"),
            json!({
                "type": "content_block_stop",
                "index": 4
            }),
        );

        let complete =
            StdinSessionManager::translate_subagent_stream_event(&stop_event, &mut state);
        assert_eq!(
            complete,
            vec![FrontendSubagentEvent::ToolResultComplete(
                SubagentToolResultCompletePayload {
                    session_id: "session_1".to_string(),
                    parent_tool_use_id: "task_1".to_string(),
                    tool_use_id: "toolu_1".to_string(),
                },
            )]
        );
    }

    #[test]
    fn assistant_message_preserves_parent_tool_use_id() {
        let manager = StdinSessionManager::new();
        let assist = AssistantMessage {
            message: AssistantContent {
                id: "msg_1".to_string(),
                content_type: "message".to_string(),
                role: "assistant".to_string(),
                model: "claude-test".to_string(),
                content: vec![ContentBlock::Text {
                    text: "done".to_string(),
                }],
                stop_reason: Some("end_turn".to_string()),
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 2,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                },
            },
            parent_tool_use_id: Some("task_1".to_string()),
            error: None,
            uuid: "uuid_1".to_string(),
            session_id: "session_1".to_string(),
        };

        let message = manager.create_assistant_message("session_1", &assist);

        assert_eq!(message.parent_tool_use_id.as_deref(), Some("task_1"));
    }
}
