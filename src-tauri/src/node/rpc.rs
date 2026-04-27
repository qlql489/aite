// JSON-RPC 2.0 Client for Node.js communication

use crate::models::*;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex as TokioMutex};
use tracing::{debug, warn};

/// Request timeout in seconds
const DEFAULT_TIMEOUT_SECONDS: u64 = 30;

/// Options for making RPC requests
#[derive(Debug, Clone)]
pub struct RequestOptions {
    pub method: String,
    pub timeout_seconds: u64,
    pub created_at: std::time::Instant,
}

impl RequestOptions {
    pub fn new(method: String) -> Self {
        Self {
            method,
            timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
            created_at: std::time::Instant::now(),
        }
    }

    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout_seconds = timeout;
        self
    }
}

/// JSON-RPC notification (one-way message)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

impl RpcNotification {
    pub fn new(method: String, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method,
            params,
        }
    }
}

/// Response callback for pending requests
type ResponseCallback = oneshot::Sender<Result<Value>>;

/// JSON-RPC 2.0 Client
pub struct RpcClient {
    /// Pending requests (id -> callback)
    pending_requests: Arc<TokioMutex<HashMap<String, ResponseCallback>>>,
    /// Reference to the Node process (via global)
    _process_ref: (),
}

impl RpcClient {
    /// Create a new RPC client
    pub fn new() -> Self {
        Self {
            pending_requests: Arc::new(TokioMutex::new(HashMap::new())),
            _process_ref: (),
        }
    }

    /// Call a JSON-RPC method with a timeout
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.call_with_timeout(method, params, DEFAULT_TIMEOUT_SECONDS)
            .await
    }

    /// Call a JSON-RPC method with custom timeout
    pub async fn call_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout_secs: u64,
    ) -> Result<Value> {
        let request_id = uuid::Uuid::new_v4().to_string();

        debug!("RPC call: {} (id: {})", method, request_id);

        // Create channel for response
        let (tx, rx) = oneshot::channel();

        // Register the pending request
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(request_id.clone(), tx);
        }

        // Send the request via Node process
        {
            let mut process = crate::node::process::GLOBAL_NODE_PROCESS
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to lock Node process: {}", e))?;

            process
                .send_request(method, params)
                .context("Failed to send request to Node process")?;
        }

        // Wait for response with timeout
        let timeout_duration = std::time::Duration::from_secs(timeout_secs);
        let result = tokio::time::timeout(timeout_duration, rx).await;

        // Remove pending request
        {
            let mut pending = self.pending_requests.lock().await;
            pending.remove(&request_id);
        }

        match result {
            Ok(Ok(response)) => response,
            Ok(Err(e)) => Err(anyhow::anyhow!("Response channel closed: {}", e)),
            Err(_) => {
                warn!("RPC call timed out: {} (id: {})", method, request_id);
                Err(anyhow::anyhow!(
                    "RPC call timed out after {} seconds",
                    timeout_secs
                ))
            }
        }
    }

    /// Send a notification (one-way message, no response expected)
    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        debug!("RPC notify: {}", method);

        let mut process = crate::node::process::GLOBAL_NODE_PROCESS
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock Node process: {}", e))?;

        // For notifications, we don't expect a response
        process.send_request(method, params)?;

        Ok(())
    }

    /// Handle an incoming JSON-RPC response
    pub async fn handle_response(&self, response: RpcResponse) {
        debug!("Handling RPC response for id: {}", response.id);

        let mut pending = self.pending_requests.lock().await;

        if let Some(tx) = pending.remove(&response.id) {
            if let Some(error) = response.error {
                let _ = tx.send(Err(anyhow::anyhow!(
                    "RPC error {}: {}",
                    error.code,
                    error.message
                )));
            } else if let Some(result) = response.result {
                let _ = tx.send(Ok(result));
            } else {
                let _ = tx.send(Err(anyhow::anyhow!(
                    "Response has neither result nor error"
                )));
            }
        } else {
            warn!("Received response for unknown request id: {}", response.id);
        }
    }

    /// Handle an incoming JSON-RPC notification
    pub fn handle_notification(&self, notification: RpcNotification) -> NotificationHandler {
        NotificationHandler {
            method: notification.method,
            params: notification.params,
        }
    }
}

impl Default for RpcClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for incoming notifications
pub struct NotificationHandler {
    method: String,
    params: Value,
}

impl NotificationHandler {
    /// Get the notification method
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Get the notification params
    pub fn params(&self) -> &Value {
        &self.params
    }

    /// Check if this is a specific method
    pub fn is_method(&self, method: &str) -> bool {
        self.method == method
    }

    /// Try to parse params as a specific type
    pub fn parse_params<T>(&self) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_value(self.params.clone()).context("Failed to parse notification params")
    }
}

/// Global RPC client instance
pub static GLOBAL_RPC_CLIENT: std::sync::LazyLock<RpcClient> =
    std::sync::LazyLock::new(RpcClient::new);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_request_options_creation() {
        let opts = RequestOptions::new("test.method".to_string());
        assert_eq!(opts.method, "test.method");
        assert_eq!(opts.timeout_seconds, DEFAULT_TIMEOUT_SECONDS);
    }

    #[test]
    fn test_request_options_with_timeout() {
        let opts = RequestOptions::new("test.method".to_string()).with_timeout(60);
        assert_eq!(opts.timeout_seconds, 60);
    }

    #[test]
    fn test_rpc_notification_creation() {
        let notification = RpcNotification::new("test.notify".to_string(), json!({"key": "value"}));
        assert_eq!(notification.jsonrpc, "2.0");
        assert_eq!(notification.method, "test.notify");
    }
}
