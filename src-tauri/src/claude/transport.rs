// Claude Transport Layer Abstraction
//
// This module defines the core traits for Claude SDK communication,
// allowing different transport implementations (WebSocket, HTTP, etc.)

use crate::claude::protocol::SdkMessage;
use async_trait::async_trait;
use std::path::PathBuf;
use thiserror::Error;

// ============================================================================
// Error Types
// ============================================================================

/// Transport layer errors
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("Not connected")]
    NotConnected,

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Receive failed: {0}")]
    ReceiveFailed(String),

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Timeout")]
    Timeout,

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// Process management errors
#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("Process not found")]
    NotFound,

    #[error("Launch failed: {0}")]
    LaunchFailed(String),

    #[error("Stop failed: {0}")]
    StopFailed(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Process already running")]
    AlreadyRunning,

    #[error("Process not running")]
    NotRunning,

    #[error("Unknown error: {0}")]
    Unknown(String),
}

// ============================================================================
// Transport Trait
// ============================================================================

/// Claude transport layer abstraction
///
/// This trait defines the interface for communicating with Claude Code CLI.
/// Implementations can use WebSocket, HTTP, gRPC, or any other protocol.
#[async_trait]
pub trait ClaudeTransport: Send + Sync {
    /// Start the transport layer
    async fn start(&self) -> Result<(), TransportError>;

    /// Stop the transport layer
    async fn stop(&self) -> Result<(), TransportError>;

    /// Send a message to Claude CLI
    async fn send_message(&self, message: SdkMessage) -> Result<(), TransportError>;

    /// Receive the next message from Claude CLI (blocking)
    async fn recv_message(&self) -> Option<SdkMessage>;

    /// Check if transport is connected and ready
    async fn is_connected(&self) -> bool;

    /// Get the current session ID
    async fn session_id(&self) -> Option<String>;

    /// Subscribe to broadcast channel for receiving messages
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SdkMessage>;
}

// ============================================================================
// Process Management Trait
// ============================================================================

/// Claude CLI process management abstraction
///
/// This trait defines the interface for managing the Claude Code CLI process.
/// Implementations can handle process spawning, lifecycle, and working directory.
#[async_trait]
pub trait ClaudeCliProcess: Send + Sync {
    /// Start the CLI process
    async fn start(&self) -> Result<(), ProcessError>;

    /// Stop the CLI process
    async fn stop(&self) -> Result<(), ProcessError>;

    /// Check if the CLI process is running
    async fn is_running(&self) -> bool;

    /// Set the working directory for the CLI process
    async fn set_working_directory(&self, path: PathBuf) -> Result<(), ProcessError>;

    /// Find the Claude CLI executable
    async fn find_cli(&self) -> Result<PathBuf, ProcessError>;
}

// ============================================================================
// Helper Types
// ============================================================================

/// Result type for transport operations
pub type TransportResult<T> = Result<T, TransportError>;

/// Result type for process operations
pub type ProcessResult<T> = Result<T, ProcessError>;
