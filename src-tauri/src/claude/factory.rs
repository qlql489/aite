// Claude Transport Factory
//
// This module provides factory methods for creating transport and process
// manager instances based on configuration.

use crate::claude::transport::{ClaudeCliProcess, ClaudeTransport, ProcessError, TransportError};
use crate::claude::transports::{HttpTransport, WebSocketTransport};
use crate::models::ConnectionMode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

// ============================================================================
// Transport Type
// ============================================================================

/// Available transport types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    /// WebSocket transport (default)
    WebSocket,

    /// HTTP transport
    Http,

    /// Stdin/Stdout transport
    Stdio,
}

impl Default for TransportType {
    fn default() -> Self {
        Self::Stdio // Default to stdin/stdout
    }
}

impl From<ConnectionMode> for TransportType {
    fn from(mode: ConnectionMode) -> Self {
        match mode {
            ConnectionMode::WebSocket => Self::WebSocket,
            ConnectionMode::Stdio => Self::Stdio,
        }
    }
}

// ============================================================================
// Configuration Types
// ============================================================================

/// WebSocket server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// Host address to bind to
    pub host: String,

    /// Port to listen on
    pub port: u16,

    /// Optional authentication token
    pub auth_token: Option<String>,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8765,
            auth_token: None,
        }
    }
}

/// HTTP transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// Base URL for HTTP API
    pub base_url: String,

    /// API key for authentication
    pub api_key: Option<String>,

    /// Request timeout in seconds
    pub timeout_secs: u64,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8765".to_string(),
            api_key: None,
            timeout_secs: 30,
        }
    }
}

/// Complete Claude configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeConfig {
    /// Transport type to use
    #[serde(default)]
    pub transport_type: TransportType,

    /// WebSocket configuration
    #[serde(default)]
    pub ws_config: WebSocketConfig,

    /// HTTP configuration
    #[serde(default)]
    pub http_config: HttpConfig,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            transport_type: TransportType::default(),
            ws_config: WebSocketConfig::default(),
            http_config: HttpConfig::default(),
        }
    }
}

// ============================================================================
// Transport Factory
// ============================================================================

/// Factory for creating transport instances
pub struct TransportFactory;

impl TransportFactory {
    /// Create a transport instance based on configuration
    pub fn create_transport(
        config: &ClaudeConfig,
    ) -> Result<Arc<dyn ClaudeTransport>, TransportError> {
        match &config.transport_type {
            TransportType::WebSocket => {
                use crate::claude::server::ServerConfig;

                let server_config = ServerConfig {
                    host: config.ws_config.host.clone(),
                    port: config.ws_config.port,
                    auth_token: config.ws_config.auth_token.clone(),
                };

                WebSocketTransport::new(server_config)
                    .map(|t| Arc::new(t) as Arc<dyn ClaudeTransport>)
            }
            TransportType::Http => HttpTransport::new(config.http_config.clone())
                .map(|t| Arc::new(t) as Arc<dyn ClaudeTransport>),
            TransportType::Stdio => {
                // Stdin transport doesn't need pre-creation
                // It will be attached when the process starts
                Err(TransportError::Configuration(
                    "Stdio transport is created dynamically by the process manager".to_string(),
                ))
            }
        }
    }

    /// Create a process manager instance
    ///
    /// The process manager is independent of transport type,
    /// as it manages the CLI process lifecycle.
    pub fn create_process_manager(
        transport: Arc<dyn ClaudeTransport>,
    ) -> Result<Arc<dyn ClaudeCliProcess>, ProcessError> {
        use crate::claude::process::CliProcessManager;

        Ok(Arc::new(CliProcessManager::new(transport)))
    }
}

// ============================================================================
// Mock Factory (for testing)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ClaudeConfig::default();
        assert_eq!(config.transport_type, TransportType::Stdio);
    }

    #[test]
    fn test_config_serialization() {
        let config = ClaudeConfig {
            transport_type: TransportType::Http,
            ws_config: WebSocketConfig {
                port: 9000,
                ..Default::default()
            },
            http_config: HttpConfig {
                base_url: "http://localhost:8080".to_string(),
                ..Default::default()
            },
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("http"));

        let decoded: ClaudeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.transport_type, TransportType::Http);
    }

    #[test]
    fn test_connection_mode_conversion() {
        assert_eq!(
            TransportType::from(ConnectionMode::WebSocket),
            TransportType::WebSocket
        );
        assert_eq!(
            TransportType::from(ConnectionMode::Stdio),
            TransportType::Stdio
        );
    }
}
