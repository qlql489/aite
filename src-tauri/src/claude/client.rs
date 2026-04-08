// Claude Code CLI Process Management
//
// This module handles launching and managing the Claude Code CLI process
// with the --sdk-url flag to connect to our WebSocket server.

use crate::claude::server::ClaudeSdkServer;
use crate::models::{get_aite_config_dir, AppConfig, AITE_APP_CONFIG_FILE};
use std::fs;
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

/// 获取应用配置文件路径
fn get_app_config_path() -> PathBuf {
    get_aite_config_dir().join(AITE_APP_CONFIG_FILE)
}

/// 读取应用配置
fn read_app_config() -> AppConfig {
    let config_path = get_app_config_path();

    if !config_path.exists() {
        debug!("应用配置文件不存在，使用默认配置: {:?}", config_path);
        return AppConfig::default();
    }

    match fs::read_to_string(&config_path) {
        Ok(content) if !content.trim().is_empty() => {
            match serde_json::from_str::<AppConfig>(&content) {
                Ok(config) => {
                    debug!(
                        "成功读取应用配置: streaming_enabled={}",
                        config.streaming_enabled
                    );
                    config
                }
                Err(e) => {
                    debug!("解析应用配置失败: {}, 使用默认配置", e);
                    AppConfig::default()
                }
            }
        }
        _ => AppConfig::default(),
    }
}

/// Claude Code CLI process manager
pub struct ClaudeClient {
    server: Arc<ClaudeSdkServer>,
    child: Arc<Mutex<Option<tokio::process::Child>>>,
    cli_path: Mutex<Option<PathBuf>>,
    working_directory: Mutex<Option<PathBuf>>,
    /// CLI's internal session ID (from system.init), used for --resume
    cli_session_id: Arc<Mutex<Option<String>>>,
    thinking_level: Mutex<String>,
}

impl ClaudeClient {
    /// Create a new Claude client manager
    pub fn new(server: Arc<ClaudeSdkServer>) -> Self {
        Self {
            server,
            child: Arc::new(Mutex::new(None)),
            cli_path: Mutex::new(None),
            working_directory: Mutex::new(None),
            cli_session_id: Arc::new(Mutex::new(None)),
            thinking_level: Mutex::new("medium".to_string()),
        }
    }

    /// Set the working directory for the CLI process
    pub async fn set_working_directory(&self, path: PathBuf) {
        info!("📁 Setting working directory: {}", path.display());
        *self.working_directory.lock().await = Some(path);
    }

    pub async fn set_thinking_level(&self, level: String) {
        *self.thinking_level.lock().await = match level.as_str() {
            "off" | "low" | "medium" | "high" => level,
            _ => "medium".to_string(),
        };
    }

    /// Set the CLI's internal session ID (from system.init), used for --resume
    pub async fn set_cli_session_id(&self, cli_session_id: String) {
        info!("📋 Setting CLI session ID: {}", cli_session_id);
        *self.cli_session_id.lock().await = Some(cli_session_id);
    }

    /// Get the CLI's internal session ID
    pub async fn get_cli_session_id(&self) -> Option<String> {
        self.cli_session_id.lock().await.clone()
    }

    /// Find the Claude Code CLI executable
    pub async fn find_cli(&self) -> Result<PathBuf, String> {
        // Check if we already found it
        {
            let path = self.cli_path.lock().await;
            if let Some(p) = path.as_ref() {
                debug!("Using cached CLI path: {}", p.display());
                return Ok(p.clone());
            }
        }

        let path = crate::commands::cli::resolve_claude_binary_path()?;
        info!("✅ Found Claude CLI at: {}", path.display());
        *self.cli_path.lock().await = Some(path.clone());
        Ok(path)
    }

    /// Launch Claude Code CLI with WebSocket SDK mode
    pub async fn launch(&self) -> Result<(), String> {
        self.launch_internal(None).await
    }

    /// Launch Claude CLI with --resume to restore conversation context
    pub async fn launch_with_resume(&self, session_id: String) -> Result<(), String> {
        info!("🔄 Launching CLI with --resume: {}", session_id);
        self.launch_internal(Some(session_id)).await
    }

    /// Relaunch Claude CLI with --resume to restore conversation context
    pub async fn relaunch(&self) -> Result<(), String> {
        // Get the CLI session ID for resuming
        let cli_session_id = self.get_cli_session_id().await;
        info!("🔄 Relaunching CLI with --resume: {:?}", cli_session_id);
        self.launch_internal(cli_session_id).await
    }

    /// Internal launch method with optional --resume parameter
    async fn launch_internal(&self, resume_session_id: Option<String>) -> Result<(), String> {
        // Check if already running
        {
            let child = self.child.lock().await;
            if child.is_some() {
                info!("ℹ️ Claude CLI already running");
                return Ok(());
            }
        }

        // Ensure server is running (not necessarily connected yet)
        if !self.server.is_running().await {
            return Err("❌ WebSocket server is not running".to_string());
        }

        // Find CLI
        let cli_path = self.find_cli().await?;

        // Get server address
        let server_config = self.server.config.clone();
        // Build WebSocket URL without sessionId in path (session identified via --resume)
        let ws_url = format!("ws://{}:{}/ws/cli", server_config.host, server_config.port);

        // Get working directory
        let working_dir = self.working_directory.lock().await.clone();
        let thinking_level = self.thinking_level.lock().await.clone();

        info!("🚀 Launching Claude CLI...");
        info!("   CLI path: {}", cli_path.display());
        info!("   SDK URL: {}", ws_url);
        if let Some(ref dir) = working_dir {
            info!("   Working directory: {}", dir.display());
        }
        if let Some(ref id) = resume_session_id {
            info!("   Resume session ID: {}", id);
        }
        info!("   Thinking level: {}", thinking_level);

        // Build command
        let runtime_env = crate::commands::cli::resolve_claude_runtime_env(&cli_path)?;
        let mut cmd = build_windows_aware_claude_command(&cli_path);
        cmd.env("PATH", &runtime_env.path);
        cmd.env_remove("CLAUDECODE");

        if let Some(ref node_path) = runtime_env.node_path {
            info!("   Runtime Node.js: {}", node_path);
        }

        if let Some(git_bash_path) = crate::commands::cli::resolve_git_bash_for_runtime()? {
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

        // Build args dynamically based on whether we're resuming
        // NOTE: We don't use --print mode because we need a long-running CLI process
        // Instead, we use interactive mode with SDK for continuous message handling
        let mut args: Vec<String> = vec![
            "--sdk-url".to_string(),
            ws_url,
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--replay-user-messages".to_string(),
            "--dangerously-skip-permissions".to_string(),
        ];

        let app_config = read_app_config();
        info!("📡 流式输出固定开启，添加 --include-partial-messages 参数");
        args.push("--include-partial-messages".to_string());

        let custom_cli_args =
            crate::claude::cli_args::sanitize_custom_cli_args(&app_config.claude_cli_extra_args);
        if !custom_cli_args.is_empty() {
            info!("🔧 应用 Claude CLI 自定义参数: {:?}", custom_cli_args);
            args.extend(custom_cli_args);
        }

        // Add --resume parameter if we have a CLI session ID
        if let Some(session_id) = resume_session_id {
            args.push("--resume".to_string());
            args.push(session_id);
        }

        args.push("--settings".to_string());
        if thinking_level == "off" {
            args.push(r#"{"alwaysThinkingEnabled":false}"#.to_string());
        } else {
            args.push(r#"{"alwaysThinkingEnabled":true}"#.to_string());
            cmd.env("CLAUDE_CODE_EFFORT_LEVEL", &thinking_level);
        }

        cmd.env("CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING", "1");

        // Add args to command
        for arg in &args {
            cmd.arg(arg);
        }

        // Log the actual full command (same args we pass to the process)
        let cmd_str = std::iter::once(cli_path.display().to_string())
            .chain(args.iter().map(|a| {
                if a.contains(' ') || a.contains('"') || a.is_empty() {
                    format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\""))
                } else {
                    a.clone()
                }
            }))
            .collect::<Vec<_>>()
            .join(" ");
        info!("📋 Full command: {}", cmd_str);

        // Set up stdout/stderr for logging - capture to detect errors
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        // Spawn the process
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("❌ Failed to launch Claude CLI: {}", e))?;

        let pid = child.id();
        info!("✅ Claude CLI launched with PID: {:?}", pid);

        // Capture stderr for error logging
        if let Some(stderr) = child.stderr.take() {
            let child_pid = pid;
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                    warn!(
                        "📤 Claude CLI stderr (PID {:?}): {}",
                        child_pid,
                        line.trim()
                    );
                    line.clear();
                }
            });
        }

        // Spawn a task to monitor the process and detect early exits
        let child_ref = Arc::clone(&self.child);
        tokio::spawn(async move {
            // Wait a bit to see if process exits immediately
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            let guard = child_ref.lock().await;
            if let Some(child) = guard.as_ref() {
                // Try to get the process ID - if None, process has exited
                let current_pid = child.id();
                if current_pid.is_none() {
                    warn!("⚠️ Claude CLI process exited shortly after launch");
                }
            }
        });

        // Store the child process
        *self.child.lock().await = Some(child);

        Ok(())
    }

    /// Stop the Claude CLI process
    pub async fn stop(&self) -> Result<(), String> {
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

    /// Check if the CLI is running
    pub async fn is_running(&self) -> bool {
        let child = self.child.lock().await;
        if let Some(child) = child.as_ref() {
            // Try to get the process ID
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
