// HTTP Transport Implementation (Placeholder)
//
// This is a placeholder for HTTP-based transport implementation.
// Not yet implemented - currently only WebSocket transport is supported.

use crate::claude::factory::HttpConfig;
use crate::claude::protocol::SdkMessage;
use crate::claude::transport::{ClaudeTransport, TransportError};
use async_trait::async_trait;
use std::sync::Arc;

/// HTTP transport implementation (placeholder)
pub struct HttpTransport {
    _config: HttpConfig,
}

impl HttpTransport {
    /// Create a new HTTP transport instance
    pub fn new(config: HttpConfig) -> Result<Self, TransportError> {
        Ok(Self { _config: config })
    }
}

#[async_trait]
impl ClaudeTransport for HttpTransport {
    async fn start(&self) -> Result<(), TransportError> {
        Err(TransportError::Configuration(
            "HTTP transport is not yet implemented".to_string(),
        ))
    }

    async fn stop(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn send_message(&self, _msg: SdkMessage) -> Result<(), TransportError> {
        Err(TransportError::Configuration(
            "HTTP transport is not yet implemented".to_string(),
        ))
    }

    async fn recv_message(&self) -> Option<SdkMessage> {
        None
    }

    async fn is_connected(&self) -> bool {
        false
    }

    async fn session_id(&self) -> Option<String> {
        None
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SdkMessage> {
        let (_tx, rx) = tokio::sync::broadcast::channel(1);
        rx
    }
}
