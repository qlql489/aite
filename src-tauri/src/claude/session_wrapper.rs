// Session Wrapper - Enum based wrapper for WebSocket and Stdio session managers
//
// This module provides an enum-based wrapper for the two session manager types,
// avoiding the complexity of trait objects.

use crate::claude::session::ClaudeSessionManager;
use crate::claude::stdin_session::StdinSessionManager;
use crate::models::Message;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::warn;

/// Callback type for streaming data chunks
pub type StreamDataCallback = Box<dyn Fn(String) + Send + Sync>;
/// Callback type for stream completion
pub type StreamCompleteCallback = Box<dyn Fn(Message) + Send + Sync>;
/// Callback type for streaming messages (new - supports multiple message types)
pub type StreamMessageCallback = Box<dyn Fn(Message) + Send + Sync>;

/// Session manager wrapper enum
pub enum SessionManagerWrapper {
    WebSocket(Arc<ClaudeSessionManager>),
    Stdio(Arc<StdinSessionManager>),
}

impl SessionManagerWrapper {
    /// Create a new WebSocket session manager
    pub fn new_websocket(manager: ClaudeSessionManager) -> Self {
        Self::WebSocket(Arc::new(manager))
    }

    /// Create a new Stdio session manager
    pub fn new_stdio(manager: StdinSessionManager) -> Self {
        Self::Stdio(Arc::new(manager))
    }

    /// Get the external session ID
    pub async fn get_external_session_id(&self) -> Option<String> {
        match self {
            Self::WebSocket(m) => m.get_external_session_id().await,
            Self::Stdio(m) => m.get_external_session_id().await,
        }
    }

    /// Set the external session ID
    pub async fn set_external_session_id(&self, session_id: String) {
        match self {
            Self::WebSocket(m) => m.set_external_session_id(session_id).await,
            Self::Stdio(m) => m.set_external_session_id(session_id).await,
        }
    }

    /// Get the resume session ID
    pub async fn get_resume_session_id(&self) -> Option<String> {
        match self {
            Self::WebSocket(m) => m.get_resume_session_id().await,
            Self::Stdio(m) => m.get_resume_session_id().await,
        }
    }

    /// Set the resume session ID
    pub async fn set_resume_session_id(&self, session_id: String) {
        match self {
            Self::WebSocket(m) => m.set_resume_session_id(session_id).await,
            Self::Stdio(m) => m.set_resume_session_id(session_id).await,
        }
    }

    /// Set the thinking level for the session CLI
    pub async fn set_thinking_level(&self, level: String) {
        match self {
            Self::WebSocket(m) => m.set_thinking_level(level).await,
            Self::Stdio(m) => m.set_thinking_level(level).await,
        }
    }

    pub async fn set_model(&self, model: Option<String>) {
        match self {
            Self::WebSocket(_) => {}
            Self::Stdio(m) => m.set_model(model).await,
        }
    }

    pub async fn set_provider_env(&self, provider_env: Option<crate::models::SessionProviderEnv>) {
        match self {
            Self::WebSocket(_) => {}
            Self::Stdio(m) => m.set_provider_env(provider_env).await,
        }
    }

    /// Get the permission mode
    pub async fn get_permission_mode(&self) -> crate::claude::protocol::PermissionMode {
        match self {
            Self::WebSocket(m) => m.get_permission_mode().await,
            Self::Stdio(m) => m.get_permission_mode().await,
        }
    }

    /// Set the permission mode
    pub async fn set_permission_mode(&self, mode: crate::claude::protocol::PermissionMode) {
        match self {
            Self::WebSocket(m) => m.set_permission_mode(mode).await,
            Self::Stdio(m) => m.set_permission_mode(mode).await,
        }
    }

    pub async fn rewind_files(&self, user_message_id: String) -> Result<(), String> {
        match self {
            Self::WebSocket(m) => m.rewind_files(user_message_id).await,
            Self::Stdio(m) => m.rewind_files(user_message_id).await,
        }
    }

    /// Set the AppHandle
    pub async fn set_app_handle(&self, app: tauri::AppHandle) {
        match self {
            Self::WebSocket(m) => m.set_app_handle(app).await,
            Self::Stdio(m) => m.set_app_handle(app).await,
        }
    }

    /// Launch CLI
    pub async fn launch_cli(&self) -> Result<(), String> {
        match self {
            Self::WebSocket(m) => m.launch_cli().await,
            Self::Stdio(m) => m.launch_cli().await,
        }
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        match self {
            Self::WebSocket(m) => m.server.is_connected().await,
            Self::Stdio(m) => m.is_connected().await,
        }
    }

    /// Get CLI session ID
    pub async fn get_cli_session_id(&self) -> Option<String> {
        match self {
            Self::WebSocket(m) => m.client.get_cli_session_id().await,
            Self::Stdio(m) => m.get_cli_session_id().await,
        }
    }

    /// Stop the session
    pub async fn stop(&self) -> Result<(), String> {
        match self {
            Self::WebSocket(m) => m.stop().await,
            Self::Stdio(m) => m.stop().await,
        }
    }

    /// Set working directory
    pub async fn set_working_directory(&self, path: PathBuf) -> Result<(), String> {
        match self {
            Self::WebSocket(m) => {
                m.set_working_directory(path.to_string_lossy().to_string())
                    .await
            }
            Self::Stdio(m) => m.set_working_directory(path).await,
        }
    }

    /// Set working directory from String (convenience method)
    pub async fn set_working_directory_str(&self, path: String) -> Result<(), String> {
        self.set_working_directory(PathBuf::from(path)).await
    }

    /// Send permission decision
    pub async fn send_permission_decision(
        &self,
        request_id: String,
        decision: crate::claude::session::PermissionDecision,
    ) -> Result<(), String> {
        match self {
            Self::WebSocket(m) => m.send_permission_decision(request_id, decision).await,
            Self::Stdio(m) => {
                // Convert to Stdio type decision
                let converted = match decision {
                    crate::claude::session::PermissionDecision::Approve {
                        updated_input,
                        allow_tools,
                    } => crate::claude::stdin_session::PermissionDecision::Approve {
                        updated_input,
                        allow_tools,
                    },
                    crate::claude::session::PermissionDecision::Reject { reason, suggestion } => {
                        crate::claude::stdin_session::PermissionDecision::Reject {
                            reason,
                            suggestion,
                        }
                    }
                    crate::claude::session::PermissionDecision::ChangeMode { mode } => {
                        crate::claude::stdin_session::PermissionDecision::ChangeMode { mode }
                    }
                };
                m.send_permission_decision(request_id, converted).await
            }
        }
    }

    pub async fn remember_permission_rule(
        &self,
        request_id: String,
    ) -> Result<Option<String>, String> {
        match self {
            Self::WebSocket(m) => m.remember_permission_rule(&request_id).await,
            Self::Stdio(m) => m.remember_permission_rule(&request_id).await,
        }
    }

    /// Send a message to Claude (WebSocket mode)
    pub async fn send_message(
        &self,
        session_id: String,
        content: String,
    ) -> Result<Message, String> {
        match self {
            Self::WebSocket(m) => m.send_message(session_id, content).await,
            Self::Stdio(m) => {
                // For Stdio mode, send message and wait for response
                // This is a simplified version - the actual implementation would need
                // to handle the response and return a Message
                m.send_message(content).await?;
                // Return a placeholder message for now
                Ok(Message::new(
                    session_id,
                    crate::models::MessageRole::Assistant,
                    crate::models::MessageContent::Text("Message sent".to_string()),
                ))
            }
        }
    }

    /// Send message with streaming (not fully implemented for Stdio mode)
    pub async fn send_message_streaming(
        &self,
        session_id: String,
        content: String,
        data_callback: StreamDataCallback,
        complete_callback: StreamCompleteCallback,
    ) -> Result<Message, String> {
        match self {
            Self::WebSocket(m) => {
                m.send_message_streaming(session_id, content, data_callback, complete_callback)
                    .await
            }
            Self::Stdio(m) => {
                // For Stdio mode, implement simple streaming
                m.send_message(content).await?;
                // TODO: Implement proper streaming for Stdio mode
                Ok(Message::new(
                    session_id,
                    crate::models::MessageRole::Assistant,
                    crate::models::MessageContent::Text(
                        "Streaming not fully implemented".to_string(),
                    ),
                ))
            }
        }
    }

    /// Send message with multi-message streaming (not fully implemented for Stdio mode)
    pub async fn send_message_streaming_with_messages(
        &self,
        session_id: String,
        content: String,
        content_blocks: Option<Vec<crate::claude::protocol::ContentBlock>>,
        data_callback: StreamDataCallback,
        message_callback: StreamMessageCallback,
        complete_callback: StreamCompleteCallback,
    ) -> Result<Message, String> {
        match self {
            Self::WebSocket(m) => {
                m.send_message_streaming_with_messages(
                    session_id,
                    content,
                    content_blocks,
                    data_callback,
                    message_callback,
                    complete_callback,
                )
                .await
            }
            Self::Stdio(m) => {
                // Use the new streaming implementation
                m.send_message_streaming_with_messages(
                    session_id,
                    content,
                    content_blocks,
                    data_callback,
                    message_callback,
                    complete_callback,
                )
                .await
            }
        }
    }

    /// Stop streaming
    pub async fn stop_streaming(&self, session_id: String) -> Result<(), String> {
        match self {
            Self::WebSocket(m) => m.stop_streaming(session_id).await,
            Self::Stdio(m) => m.stop_streaming(session_id).await,
        }
    }

    /// Get commands and skills
    pub async fn get_commands_and_skills(
        &self,
    ) -> Result<
        (
            Option<Vec<crate::claude::protocol::CommandInfo>>,
            Option<Vec<String>>,
        ),
        String,
    > {
        match self {
            Self::WebSocket(m) => m.get_commands_and_skills().await,
            Self::Stdio(m) => {
                // For Stdio mode, use the new get_commands_and_skills method
                let (commands, skills) = m.get_commands_and_skills().await;
                Ok((commands, skills))
            }
        }
    }

    /// Get working directory (only for WebSocket mode)
    pub async fn get_working_directory(&self) -> Option<PathBuf> {
        match self {
            Self::WebSocket(m) => m.get_working_directory().await,
            Self::Stdio(m) => None,
        }
    }

    /// Check if this is a WebSocket session
    pub fn is_websocket(&self) -> bool {
        matches!(self, Self::WebSocket(_))
    }

    /// Check if this is a Stdio session
    pub fn is_stdio(&self) -> bool {
        matches!(self, Self::Stdio(_))
    }

    /// Get the WebSocket manager if this is a WebSocket session
    pub fn as_websocket(&self) -> Option<&Arc<ClaudeSessionManager>> {
        match self {
            Self::WebSocket(m) => Some(m),
            Self::Stdio(_) => None,
        }
    }

    /// Get the Stdio manager if this is a Stdio session
    pub fn as_stdio(&self) -> Option<&Arc<StdinSessionManager>> {
        match self {
            Self::WebSocket(_) => None,
            Self::Stdio(m) => Some(m),
        }
    }
}

impl Clone for SessionManagerWrapper {
    fn clone(&self) -> Self {
        match self {
            Self::WebSocket(m) => Self::WebSocket(Arc::clone(m)),
            Self::Stdio(m) => Self::Stdio(Arc::clone(m)),
        }
    }
}
