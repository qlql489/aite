// Node.js process manager

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Manages the Node.js backend process
pub struct NodeProcess {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    message_tx: mpsc::UnboundedSender<Value>,
    response_rx: Arc<Mutex<mpsc::UnboundedReceiver<Value>>>,
}

impl NodeProcess {
    /// Create a new NodeProcess manager
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            child: None,
            stdin: None,
            message_tx: tx,
            response_rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// Start the Node.js backend process
    pub fn start(&mut self) -> Result<()> {
        if self.is_running() {
            info!("Node.js backend process already running");
            return Ok(());
        }

        info!("Starting Node.js backend process");

        let node_exe = Self::find_node_executable()?;
        let backend_entry = Self::find_backend_entry()?;
        let runtime_path = crate::commands::cli::build_enriched_path();

        if cfg!(debug_assertions) {
            info!("Development mode: using compiled JS at {:?}", backend_entry);
        } else {
            debug!("Found Node.js executable: {:?}", node_exe);
        }

        debug!("Backend entry point: {:?}", backend_entry);
        debug!("Executable: {:?}", node_exe);
        info!("Node bridge runtime PATH: {}", runtime_path);

        // Spawn the Node.js process
        let mut child = Command::new(&node_exe)
            .arg(&backend_entry)
            .env("CLAUDE_DESK_BRIDGE_PORT", "53686")
            .env("PATH", &runtime_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn Node.js process")?;

        info!("Node.js process spawned with PID: {:?}", child.id());

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to open stdin"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to open stdout"))?;

        // Take stderr for logging
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to open stderr"))?;

        self.stdin = Some(stdin);
        self.child = Some(child);

        // Start stderr reader in background
        let _ = thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(text) = line {
                    debug!("[Node stderr] {}", text);
                }
            }
        });

        // Start stdout reader
        self.start_stdout_reader(stdout)?;

        info!(
            "Node.js process started (PID: {:?})",
            self.child.as_ref().map(|c| c.id())
        );

        // Give the process a moment to initialize
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Verify the process is still running
        if !self.is_running() {
            return Err(anyhow::anyhow!(
                "Node.js process exited immediately after start"
            ));
        }

        info!("Node.js process verified running");
        Ok(())
    }

    /// Stop the Node.js backend process
    pub fn stop(&mut self) -> Result<()> {
        info!("Stopping Node.js process");

        if let Some(mut child) = self.child.take() {
            child.kill().context("Failed to kill Node.js process")?;
            info!("Node.js process stopped");
        }

        self.stdin = None;

        Ok(())
    }

    /// Check if the Node.js process is running
    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(None) => true,     // Still running
                Ok(Some(_)) => false, // Already exited
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Send a JSON-RPC request to the Node process
    pub fn send_request(&mut self, method: &str, params: Value) -> Result<String> {
        if !self.is_running() {
            return Err(anyhow::anyhow!("Node.js process is not running"));
        }

        let id = uuid::Uuid::new_v4().to_string();

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        debug!("Sending JSON-RPC request: {}", request);

        if let Some(stdin) = &mut self.stdin {
            writeln!(stdin, "{}", request).context("Failed to write to Node.js stdin")?;
        } else {
            return Err(anyhow::anyhow!("Node.js stdin not available"));
        }

        Ok(id)
    }

    /// Try to receive a message from the Node process (non-blocking)
    pub fn try_recv_message(&self) -> Option<Value> {
        self.response_rx.lock().ok()?.try_recv().ok()
    }

    /// Start the stdout reader task
    fn start_stdout_reader(&self, stdout: ChildStdout) -> Result<()> {
        let tx = self.message_tx.clone();

        thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(stdout);

            for line in reader.lines() {
                match line {
                    Ok(text) => {
                        debug!("Received from Node.js: {}", text);

                        match serde_json::from_str::<Value>(&text) {
                            Ok(value) => {
                                if let Err(e) = tx.send(value) {
                                    error!("Failed to send message to channel: {}", e);
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse JSON from Node.js: {} | Line: {}", e, text);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to read line from stdout: {}", e);
                        break;
                    }
                }
            }

            warn!("Node.js stdout reader terminated");
        });

        Ok(())
    }

    fn find_backend_entry() -> Result<PathBuf> {
        let current_dir = std::env::current_dir()?;
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        let mut candidates = vec![current_dir
            .join("node-backend")
            .join("build")
            .join("index.js")];

        if let Some(parent) = current_dir.parent() {
            candidates.push(parent.join("node-backend").join("build").join("index.js"));
        }

        candidates.push(
            manifest_dir
                .join("..")
                .join("node-backend")
                .join("build")
                .join("index.js"),
        );

        for candidate in candidates {
            if candidate.exists() {
                return Ok(candidate);
            }
        }

        Err(anyhow::anyhow!(
            "Compiled backend not found. Looked in: {:?}, {:?}, {:?}. Run: npm run build:node",
            current_dir
                .join("node-backend")
                .join("build")
                .join("index.js"),
            current_dir
                .parent()
                .map(|dir| dir.join("node-backend").join("build").join("index.js")),
            manifest_dir
                .join("..")
                .join("node-backend")
                .join("build")
                .join("index.js")
        ))
    }

    /// Find the Node.js executable
    fn find_node_executable() -> Result<PathBuf> {
        // Priority 1: Bundled Node.js (for production)
        let bundled = std::env::current_dir()?
            .join("resources")
            .join("node")
            .join(if cfg!(windows) { "node.exe" } else { "node" });

        if bundled.exists() {
            info!("Using bundled Node.js: {:?}", bundled);
            return Ok(bundled);
        }

        if let Some(local_node) =
            crate::commands::cli::get_local_node_executable_path().filter(|path| path.exists())
        {
            info!("Using local app Node.js: {:?}", local_node);
            return Ok(local_node);
        }

        let runtime_path = crate::commands::cli::build_enriched_path();

        // Priority 3: System Node.js from enriched PATH
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        match which::which_in("node", Some(&runtime_path), cwd) {
            Ok(path) => {
                info!("Using system Node.js: {:?}", path);
                Ok(path)
            }
            Err(e) => Err(anyhow::anyhow!(
                "Node.js not found. Please install Node.js or bundle it with the application. runtime PATH: {}. Error: {}",
                runtime_path,
                e
            )),
        }
    }
}

impl Default for NodeProcess {
    fn default() -> Self {
        Self::new()
    }
}

/// Global NodeProcess instance (lazy-initialized)
pub static GLOBAL_NODE_PROCESS: std::sync::LazyLock<std::sync::Mutex<NodeProcess>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(NodeProcess::new()));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_process_creation() {
        let process = NodeProcess::new();
        assert!(!process.is_running());
    }

    #[test]
    fn test_find_node_executable() {
        // This test assumes Node.js is installed in the system
        let result = NodeProcess::find_node_executable();
        assert!(
            result.is_ok(),
            "Node.js should be installed for development"
        );
    }
}
