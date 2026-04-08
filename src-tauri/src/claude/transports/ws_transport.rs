// WebSocket Transport Implementation
//
// This module wraps the existing ClaudeSdkServer to implement the ClaudeTransport trait.

use crate::claude::protocol::SdkMessage;
use crate::claude::server::{ClaudeSdkServer, ServerConfig};
use crate::claude::transport::{ClaudeTransport, TransportError};
use async_trait::async_trait;
use std::sync::Arc;

/// WebSocket transport - wrapper around ClaudeSdkServer
pub struct WebSocketTransport {
    inner: Arc<ClaudeSdkServer>,
}

impl WebSocketTransport {
    /// Create a new WebSocket transport by wrapping the server
    pub fn new(config: ServerConfig) -> Result<Self, TransportError> {
        Ok(Self {
            inner: Arc::new(ClaudeSdkServer::new(config)),
        })
    }
}

#[async_trait]
impl ClaudeTransport for WebSocketTransport {
    async fn start(&self) -> Result<(), TransportError> {
        self.inner
            .start()
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))
    }

    async fn stop(&self) -> Result<(), TransportError> {
        self.inner.stop().await;
        Ok(())
    }

    async fn send_message(&self, msg: SdkMessage) -> Result<(), TransportError> {
        self.inner
            .send_message(msg)
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))
    }

    async fn recv_message(&self) -> Option<SdkMessage> {
        self.inner.recv_message().await
    }

    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }

    async fn session_id(&self) -> Option<String> {
        self.inner.session_id().await
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SdkMessage> {
        // Note: The original server uses unbounded mpsc, not broadcast
        // This is a limitation - for now we create a dummy receiver
        let (_tx, rx) = tokio::sync::broadcast::channel(1);
        rx
    }
}
