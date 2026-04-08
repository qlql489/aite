// Session Manager Trait
//
// This module defines a common trait for both WebSocket and Stdio session managers,
// allowing the registry to work with either implementation.

use async_trait::async_trait;
use std::path::PathBuf;

/// Permission decision from user (shared between session managers)
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
        mode: crate::claude::protocol::PermissionMode,
    },
}

/// Common trait for session managers
#[async_trait]
pub trait SessionManager: Send + Sync {
    /// Set the external session ID
    async fn set_external_session_id(&self, session_id: String);

    /// Get the external session ID
    async fn get_external_session_id(&self) -> Option<String>;

    /// Set the resume session ID (for --resume)
    async fn set_resume_session_id(&self, session_id: String);

    /// Get the resume session ID
    async fn get_resume_session_id(&self) -> Option<String>;

    /// Set the permission mode
    async fn set_permission_mode(&self, mode: crate::claude::protocol::PermissionMode);

    /// Get the permission mode
    async fn get_permission_mode(&self) -> crate::claude::protocol::PermissionMode;

    /// Set the AppHandle for sending events to frontend
    async fn set_app_handle(&self, app: tauri::AppHandle);

    /// Send permission decision
    async fn send_permission_decision(
        &self,
        request_id: String,
        decision: PermissionDecision,
    ) -> Result<(), String>;

    /// Launch Claude CLI
    async fn launch_cli(&self) -> Result<(), String>;

    /// Check if connected to CLI
    async fn is_connected(&self) -> bool;

    /// Get CLI session ID
    async fn get_cli_session_id(&self) -> Option<String>;

    /// Stop the CLI process
    async fn stop(&self) -> Result<(), String>;

    /// Set working directory
    async fn set_working_directory(&self, path: String) -> Result<(), String>;
}

// Implement the trait for ClaudeSessionManager (WebSocket mode)
#[async_trait]
impl SessionManager for crate::claude::session::ClaudeSessionManager {
    async fn set_external_session_id(&self, session_id: String) {
        self.set_external_session_id(session_id).await;
    }

    async fn get_external_session_id(&self) -> Option<String> {
        self.get_external_session_id().await
    }

    async fn set_resume_session_id(&self, session_id: String) {
        self.set_resume_session_id(session_id).await;
    }

    async fn get_resume_session_id(&self) -> Option<String> {
        self.get_resume_session_id().await
    }

    async fn set_permission_mode(&self, mode: crate::claude::protocol::PermissionMode) {
        self.set_permission_mode(mode).await;
    }

    async fn get_permission_mode(&self) -> crate::claude::protocol::PermissionMode {
        self.get_permission_mode().await
    }

    async fn set_app_handle(&self, app: tauri::AppHandle) {
        self.set_app_handle(app).await;
    }

    async fn send_permission_decision(
        &self,
        request_id: String,
        decision: PermissionDecision,
    ) -> Result<(), String> {
        let converted = match decision {
            PermissionDecision::Approve {
                updated_input,
                allow_tools,
            } => crate::claude::session::PermissionDecision::Approve {
                updated_input,
                allow_tools,
            },
            PermissionDecision::Reject { reason, suggestion } => {
                crate::claude::session::PermissionDecision::Reject { reason, suggestion }
            }
            PermissionDecision::ChangeMode { mode } => {
                crate::claude::session::PermissionDecision::ChangeMode { mode }
            }
        };
        self.send_permission_decision(request_id, converted).await
    }

    async fn launch_cli(&self) -> Result<(), String> {
        self.launch_cli().await
    }

    async fn is_connected(&self) -> bool {
        self.is_connected().await
    }

    async fn get_cli_session_id(&self) -> Option<String> {
        self.get_cli_session_id().await
    }

    async fn stop(&self) -> Result<(), String> {
        self.stop().await
    }

    async fn set_working_directory(&self, path: String) -> Result<(), String> {
        self.set_working_directory(path).await
    }
}

// Implement the trait for StdinSessionManager (Stdio mode)
#[async_trait]
impl SessionManager for crate::claude::stdin_session::StdinSessionManager {
    async fn set_external_session_id(&self, session_id: String) {
        self.set_external_session_id(session_id).await;
    }

    async fn get_external_session_id(&self) -> Option<String> {
        self.get_external_session_id().await
    }

    async fn set_resume_session_id(&self, session_id: String) {
        self.set_resume_session_id(session_id).await;
    }

    async fn get_resume_session_id(&self) -> Option<String> {
        self.get_resume_session_id().await
    }

    async fn set_permission_mode(&self, mode: crate::claude::protocol::PermissionMode) {
        self.set_permission_mode(mode).await;
    }

    async fn get_permission_mode(&self) -> crate::claude::protocol::PermissionMode {
        self.get_permission_mode().await
    }

    async fn set_app_handle(&self, app: tauri::AppHandle) {
        self.set_app_handle(app).await;
    }

    async fn send_permission_decision(
        &self,
        request_id: String,
        decision: PermissionDecision,
    ) -> Result<(), String> {
        let converted = match decision {
            PermissionDecision::Approve {
                updated_input,
                allow_tools,
            } => crate::claude::stdin_session::PermissionDecision::Approve {
                updated_input,
                allow_tools,
            },
            PermissionDecision::Reject { reason, suggestion } => {
                crate::claude::stdin_session::PermissionDecision::Reject { reason, suggestion }
            }
            PermissionDecision::ChangeMode { mode } => {
                crate::claude::stdin_session::PermissionDecision::ChangeMode { mode }
            }
        };
        self.send_permission_decision(request_id, converted).await
    }

    async fn launch_cli(&self) -> Result<(), String> {
        self.launch_cli().await
    }

    async fn is_connected(&self) -> bool {
        self.is_connected().await
    }

    async fn get_cli_session_id(&self) -> Option<String> {
        self.get_cli_session_id().await
    }

    async fn stop(&self) -> Result<(), String> {
        self.stop().await
    }

    async fn set_working_directory(&self, path: String) -> Result<(), String> {
        self.set_working_directory(std::path::PathBuf::from(path))
            .await
    }
}
