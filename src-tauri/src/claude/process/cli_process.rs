// CLI Process Manager
//
// This module handles launching and managing the Claude Code CLI process
// through the transport layer using stream-json I/O.
// Refactored from the original ClaudeClient.

use crate::claude::transport::{ClaudeCliProcess, ClaudeTransport, ProcessError};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// CLI process manager
pub struct CliProcessManager {
    transport: Arc<dyn ClaudeTransport>,
    child: Mutex<Option<tokio::process::Child>>,
    cli_path: Mutex<Option<PathBuf>>,
    working_directory: Mutex<Option<PathBuf>>,
}

impl CliProcessManager {
    /// Create a new CLI process manager
    pub fn new(transport: Arc<dyn ClaudeTransport>) -> Self {
        Self {
            transport,
            child: Mutex::new(None),
            cli_path: Mutex::new(None),
            working_directory: Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl ClaudeCliProcess for CliProcessManager {
    /// Set the working directory for the CLI process
    async fn set_working_directory(&self, path: PathBuf) -> Result<(), ProcessError> {
        info!("📁 Setting working directory: {}", path.display());
        *self.working_directory.lock().await = Some(path);
        Ok(())
    }

    /// Find the Claude Code CLI executable
    async fn find_cli(&self) -> Result<PathBuf, ProcessError> {
        // Check if we already found it
        {
            let path = self.cli_path.lock().await;
            if let Some(p) = path.as_ref() {
                debug!("Using cached CLI path: {}", p.display());
                return Ok(p.clone());
            }
        }

        let path = crate::commands::cli::resolve_claude_binary_path()
            .map_err(ProcessError::LaunchFailed)?;
        info!("✅ Found Claude CLI at: {}", path.display());
        *self.cli_path.lock().await = Some(path.clone());
        Ok(path)
    }

    /// Start the CLI process
    async fn start(&self) -> Result<(), ProcessError> {
        // Check if already running
        {
            let child = self.child.lock().await;
            if child.is_some() {
                info!("ℹ️ Claude CLI already running");
                return Ok(());
            }
        }

        // Ensure transport is running
        if !self.transport.is_connected().await {
            return Err(ProcessError::LaunchFailed(
                "Transport is not connected or not running".to_string(),
            ));
        }

        // Find CLI
        let cli_path = self.find_cli().await?;

        // Get working directory
        let working_dir = self.working_directory.lock().await.clone();

        info!("🚀 Launching Claude CLI...");
        info!("   CLI path: {}", cli_path.display());
        if let Some(ref dir) = working_dir {
            info!("   Working directory: {}", dir.display());
        }

        // Build command
        let runtime_env = crate::commands::cli::resolve_claude_runtime_env(&cli_path)
            .map_err(ProcessError::LaunchFailed)?;
        let mut cmd = build_windows_aware_claude_command(&cli_path);
        cmd.env("PATH", &runtime_env.path);
        cmd.env_remove("CLAUDECODE");

        if let Some(ref node_path) = runtime_env.node_path {
            info!("   Runtime Node.js: {}", node_path);
        }

        if let Some(git_bash_path) = crate::commands::cli::resolve_git_bash_for_runtime()
            .map_err(ProcessError::LaunchFailed)?
        {
            cmd.env("CLAUDE_CODE_GIT_BASH_PATH", &git_bash_path);
            info!("   Git Bash: {}", git_bash_path);
        }

        // Set working directory if specified
        if let Some(dir) = working_dir {
            if dir.exists() && dir.is_dir() {
                cmd.current_dir(&dir);
                debug!("   Set current_dir to: {}", dir.display());
            } else {
                warn!(
                    "⚠️ Working directory does not exist or is not a directory: {}",
                    dir.display()
                );
            }
        }

        // Required flags for stream-json stdio mode
        cmd.arg("--print")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--input-format")
            .arg("stream-json")
            .arg("--verbose")
            // Keep the empty prompt to preserve the existing CLI invocation shape.
            .arg("-p \"\"")
            .arg("--include-partial-messages");

        // Log full command
        info!(
            "📋 Full command: {} --print --output-format stream-json --input-format stream-json --verbose -p '' --include-partial-messages",
            cli_path.display()
        );

        // Set up stdout/stderr for logging
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        // Spawn process
        let child = cmd
            .spawn()
            .map_err(|e| ProcessError::LaunchFailed(e.to_string()))?;

        info!("✅ Claude CLI launched with PID: {:?}", child.id());

        // Store child process
        *self.child.lock().await = Some(child);

        Ok(())
    }

    /// Stop the CLI process
    async fn stop(&self) -> Result<(), ProcessError> {
        let mut child_guard = self.child.lock().await;

        if let Some(mut child) = child_guard.take() {
            info!("🛑 Stopping Claude CLI process (PID: {:?})...", child.id());

            // Try graceful shutdown first
            match child.kill().await {
                Ok(_) => match child.wait().await {
                    Ok(status) => {
                        info!("✅ Claude CLI exited with status: {}", status);
                    }
                    Err(e) => {
                        warn!("⚠️ Failed to wait for Claude CLI exit: {}", e);
                    }
                },
                Err(e) => {
                    warn!("⚠️ Failed to kill Claude CLI: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Check if the CLI process is running
    async fn is_running(&self) -> bool {
        let child = self.child.lock().await;
        if let Some(child) = child.as_ref() {
            // Try to get process ID
            let pid = child.id();
            debug!("CLI running check: PID = {:?}", pid);
            pid.is_some()
        } else {
            debug!("CLI running check: not running (no child process)");
            false
        }
    }
}

fn build_windows_aware_claude_command(cli_path: &std::path::Path) -> Command {
    #[cfg(target_os = "windows")]
    {
        let cli_str = cli_path.to_string_lossy().to_string();
        let needs_cmd = cli_str.ends_with(".cmd")
            || cli_str.ends_with(".bat")
            || (!cli_str.contains('\\') && !cli_str.contains('/') && !cli_str.contains('.'));

        let mut cmd = if needs_cmd {
            let mut wrapped = Command::new("cmd");
            wrapped.arg("/C").arg(&cli_str);
            wrapped
        } else {
            Command::new(cli_path)
        };
        cmd.creation_flags(CREATE_NO_WINDOW);
        return cmd;
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new(cli_path)
    }
}
