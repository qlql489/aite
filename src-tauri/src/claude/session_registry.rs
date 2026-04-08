// Session Registry - Manage multiple independent Claude sessions
//
// Each session has its own:
// - Claude CLI process (WebSocket or Stdio mode based on config)
// - Message history
// - Working directory

use crate::claude::server::ServerConfig;
use crate::claude::session::ClaudeSessionManager;
use crate::claude::session_wrapper::SessionManagerWrapper;
use crate::claude::stdin_session::StdinSessionManager;
use crate::models::{get_aite_config_dir, AppConfig, ConnectionMode, AITE_APP_CONFIG_FILE};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Port allocator for managing WebSocket server ports (only used in WebSocket mode)
#[derive(Debug, Clone)]
pub struct PortAllocator {
    base_port: u16,
    max_ports: u16,
}

impl PortAllocator {
    /// Create a new port allocator
    pub fn new(base_port: u16, max_ports: u16) -> Self {
        Self {
            base_port,
            max_ports,
        }
    }

    /// Get the base port
    pub fn base_port(&self) -> u16 {
        self.base_port
    }

    /// Calculate port for a given session index
    pub fn port_for_index(&self, index: usize) -> Option<u16> {
        if index >= self.max_ports as usize {
            None
        } else {
            Some(self.base_port + index as u16)
        }
    }

    /// Calculate max number of ports
    pub fn max_sessions(&self) -> usize {
        self.max_ports as usize
    }
}

impl Default for PortAllocator {
    fn default() -> Self {
        Self::new(8765, 100) // Ports 8765-8864
    }
}

/// Session entry in the registry
#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub session_id: String,
    pub port: Option<u16>, // Only used in WebSocket mode
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// The CLI's internal session ID (from system.init), used for --resume
    pub cli_session_id: Option<String>,
}

/// Registry for managing multiple independent Claude sessions
pub struct SessionRegistry {
    /// Map of session_id to SessionManagerWrapper
    sessions: Arc<RwLock<HashMap<String, SessionManagerWrapper>>>,
    /// Map of session_id to metadata
    metadata: Arc<RwLock<HashMap<String, SessionEntry>>>,
    /// Aliases from temporary session IDs to remapped CLI session IDs
    session_aliases: Arc<RwLock<HashMap<String, String>>>,
    /// Port allocator (only used in WebSocket mode)
    port_allocator: PortAllocator,
    /// Used ports tracking (only used in WebSocket mode)
    used_ports: Arc<RwLock<std::collections::HashSet<u16>>>,
}

impl SessionRegistry {
    /// Create a new session registry
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            session_aliases: Arc::new(RwLock::new(HashMap::new())),
            port_allocator: PortAllocator::default(),
            used_ports: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
    }

    async fn resolve_session_id(&self, session_id: &str) -> String {
        let aliases = self.session_aliases.read().await;
        let mut resolved = session_id.to_string();

        for _ in 0..8 {
            let Some(next_id) = aliases.get(&resolved) else {
                break;
            };

            if next_id == &resolved {
                break;
            }

            resolved = next_id.clone();
        }

        if resolved != session_id {
            info!("🔀 Resolved session alias: {} -> {}", session_id, resolved);
        }

        resolved
    }

    /// Read the application configuration to determine connection mode
    fn read_app_config() -> AppConfig {
        let config_path = get_aite_config_dir().join(AITE_APP_CONFIG_FILE);

        if !config_path.exists() {
            debug!("应用配置文件不存在，使用默认配置: {:?}", config_path);
            return AppConfig::default();
        }

        match fs::read_to_string(&config_path) {
            Ok(content) if !content.trim().is_empty() => {
                match serde_json::from_str::<AppConfig>(&content) {
                    Ok(config) => {
                        info!("📋 应用配置: connection_mode={}", config.connection_mode);
                        config
                    }
                    Err(e) => {
                        warn!("解析应用配置失败: {}, 使用默认配置", e);
                        AppConfig::default()
                    }
                }
            }
            _ => AppConfig::default(),
        }
    }

    /// Create a new independent session with its own CLI
    pub async fn create_session(&self) -> Result<(String, SessionManagerWrapper), String> {
        self.create_session_with_id(None, None, None, None, None)
            .await
    }

    /// Create a new independent session with its own CLI
    /// If session_id is provided, use it for --resume; otherwise generate a new one
    /// If project_path is provided, set working directory BEFORE launching CLI
    pub async fn create_session_with_id(
        &self,
        session_id: Option<String>,
        project_path: Option<String>,
        thinking_level: Option<String>,
        model: Option<String>,
        provider_env: Option<crate::models::SessionProviderEnv>,
    ) -> Result<(String, SessionManagerWrapper), String> {
        // Check if this is a resume (session_id was provided) or new session (None)
        let is_resuming = session_id.is_some();

        // Use provided session ID (for --resume) or generate a new one
        let session_id = session_id.unwrap_or_else(|| Uuid::new_v4().to_string());

        // Read configuration to determine which mode to use
        let config = Self::read_app_config();
        let connection_mode = config.connection_mode;

        info!("📋 Connection mode: {:?}", connection_mode);

        // Create the appropriate session manager based on connection mode
        let manager = match connection_mode {
            ConnectionMode::WebSocket => {
                info!("📝 Using WebSocket mode");

                // Allocate a port for this session
                let port = self.allocate_port().await?;

                if is_resuming {
                    info!(
                        "📝 Resuming existing session: {} on port {}",
                        session_id, port
                    );
                } else {
                    info!("📝 Creating new session: {} on port {}", session_id, port);
                }

                // Create server config with allocated port
                let server_config = ServerConfig {
                    host: "127.0.0.1".to_string(),
                    port,
                    auth_token: None,
                };

                // Create a new SessionManager for this session
                let mgr = ClaudeSessionManager::with_config(server_config);

                // Set the external session ID
                mgr.set_external_session_id(session_id.clone()).await;

                // If this is a resume (history session), set the resume_session_id
                if is_resuming {
                    mgr.set_resume_session_id(session_id.clone()).await;
                }

                if let Some(level) = &thinking_level {
                    mgr.set_thinking_level(level.clone()).await;
                }

                // Start the WebSocket server
                if let Err(e) = mgr.start().await {
                    error!("❌ Failed to start session manager: {}", e);
                    self.release_port(port).await;
                    return Err(format!("Failed to start session manager: {}", e));
                }

                // Store metadata with port
                {
                    let mut metadata = self.metadata.write().await;
                    metadata.insert(
                        session_id.clone(),
                        SessionEntry {
                            session_id: session_id.clone(),
                            port: Some(port),
                            created_at: chrono::Utc::now(),
                            cli_session_id: if is_resuming {
                                Some(session_id.clone())
                            } else {
                                None
                            },
                        },
                    );
                }

                SessionManagerWrapper::new_websocket(mgr)
            }
            ConnectionMode::Stdio => {
                info!("📝 Using Stdio (stdin/stdout) mode");

                if is_resuming {
                    info!("📝 Resuming existing session: {}", session_id);
                } else {
                    info!("📝 Creating new session: {}", session_id);
                }

                // Create a new StdinSessionManager for this session
                let mgr = StdinSessionManager::new();

                // Set the external session ID
                mgr.set_external_session_id(session_id.clone()).await;

                // If this is a resume (history session), set the resume_session_id
                if is_resuming {
                    mgr.set_resume_session_id(session_id.clone()).await;
                }

                if let Some(level) = &thinking_level {
                    mgr.set_thinking_level(level.clone()).await;
                }

                if let Some(model_name) = &model {
                    mgr.set_model(Some(model_name.clone())).await;
                }

                if let Some(env) = &provider_env {
                    mgr.set_provider_env(Some(env.clone())).await;
                }

                // No server start needed for stdio mode

                // Store metadata without port
                {
                    let mut metadata = self.metadata.write().await;
                    metadata.insert(
                        session_id.clone(),
                        SessionEntry {
                            session_id: session_id.clone(),
                            port: None,
                            created_at: chrono::Utc::now(),
                            cli_session_id: if is_resuming {
                                Some(session_id.clone())
                            } else {
                                None
                            },
                        },
                    );
                }

                SessionManagerWrapper::new_stdio(mgr)
            }
        };

        // Set working directory BEFORE launch so CLI starts in project directory
        if let Some(ref path) = project_path {
            if let Err(e) = manager
                .set_working_directory(std::path::PathBuf::from(path))
                .await
            {
                error!("❌ Failed to set working directory: {}", e);
                let _ = manager.stop().await;
                // Clean up metadata and potentially release port
                {
                    let mut metadata = self.metadata.write().await;
                    metadata.remove(&session_id);
                }
                if let Some(port) = self.get_port_for_session(&session_id).await {
                    self.release_port(port).await;
                }
                return Err(format!("Failed to set working directory: {}", e));
            }
            info!("📁 Working directory set to {} before CLI launch", path);
        }

        // Launch Claude CLI
        if let Err(e) = manager.launch_cli().await {
            error!("❌ Failed to launch CLI: {}", e);
            // Clean up
            let _ = manager.stop().await;
            {
                let mut metadata = self.metadata.write().await;
                metadata.remove(&session_id);
            }
            if let Some(port) = self.get_port_for_session(&session_id).await {
                self.release_port(port).await;
            }
            return Err(format!("Failed to launch CLI: {}", e));
        }

        // Store the session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), manager.clone());
        }

        info!("✅ Session created successfully: {}", session_id);
        Ok((session_id, manager))
    }

    /// Get the port for a session (only used in WebSocket mode)
    async fn get_port_for_session(&self, session_id: &str) -> Option<u16> {
        let metadata = self.metadata.read().await;
        metadata.get(session_id)?.port
    }

    /// Get a session manager by session ID
    pub async fn get(&self, session_id: &str) -> Option<SessionManagerWrapper> {
        let resolved_id = self.resolve_session_id(session_id).await;
        let sessions = self.sessions.read().await;
        sessions.get(&resolved_id).cloned()
    }

    /// Check if a session exists
    pub async fn exists(&self, session_id: &str) -> bool {
        let resolved_id = self.resolve_session_id(session_id).await;
        let sessions = self.sessions.read().await;
        sessions.contains_key(&resolved_id)
    }

    /// Remove and cleanup a session
    pub async fn remove_session(&self, session_id: &str) -> Result<(), String> {
        let resolved_id = self.resolve_session_id(session_id).await;
        info!(
            "🗑️  Removing session: {} (requested {})",
            resolved_id, session_id
        );

        // Get the port before removing (only for WebSocket mode)
        let port = {
            let metadata = self.metadata.read().await;
            metadata.get(&resolved_id).and_then(|m| m.port)
        };

        // Get worktree mapping before removing session
        let worktree_mapping = crate::commands::worktree::get_worktree_mapping(resolved_id.clone())
            .ok()
            .flatten();

        // Remove the session manager
        {
            let mut sessions = self.sessions.write().await;
            if let Some(manager) = sessions.remove(&resolved_id) {
                // Stop the manager
                if let Err(e) = manager.stop().await {
                    warn!("⚠️  Failed to stop session manager: {}", e);
                }
            }
        }

        // Remove metadata
        {
            let mut metadata = self.metadata.write().await;
            metadata.remove(&resolved_id);
        }

        {
            let mut aliases = self.session_aliases.write().await;
            aliases.remove(session_id);
            aliases.remove(&resolved_id);
            aliases.retain(|alias, target| alias != &resolved_id && target != &resolved_id);
        }

        // Release the port (only for WebSocket mode)
        if let Some(port) = port {
            self.release_port(port).await;
        }

        // Cleanup worktree if no other sessions are using it
        if let Some(mapping) = worktree_mapping {
            let is_in_use = crate::commands::worktree::is_worktree_in_use(
                mapping.worktree_path.clone(),
                Some(resolved_id.clone()),
            )
            .unwrap_or(false);

            if !is_in_use {
                // Check if worktree is dirty
                let is_dirty = std::process::Command::new("git")
                    .args(["status", "--porcelain"])
                    .current_dir(&mapping.worktree_path)
                    .output()
                    .map(|o| !o.stdout.is_empty())
                    .unwrap_or(false);

                if !is_dirty {
                    // Auto-remove clean worktree
                    info!(
                        "🗑️  Auto-removing clean worktree: {}",
                        mapping.worktree_path
                    );
                    let _ = crate::commands::worktree::remove_worktree(
                        mapping.repo_root,
                        mapping.worktree_path,
                        Some(false),
                    );
                } else {
                    info!("🛡️  Keeping dirty worktree: {}", mapping.worktree_path);
                }
            }

            // Remove worktree tracking
            let _ = crate::commands::worktree::untrack_worktree(resolved_id.clone());
        }

        info!("✅ Session removed: {}", resolved_id);
        Ok(())
    }

    /// Get all active session IDs
    pub async fn list_sessions(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }

    /// Get session metadata
    pub async fn get_metadata(&self, session_id: &str) -> Option<SessionEntry> {
        let metadata = self.metadata.read().await;
        metadata.get(session_id).cloned()
    }

    /// Set the CLI's internal session ID for a session (from system.init message)
    pub async fn set_cli_session_id(&self, session_id: &str, cli_session_id: String) {
        let mut metadata = self.metadata.write().await;
        if let Some(entry) = metadata.get_mut(session_id) {
            entry.cli_session_id = Some(cli_session_id);
            info!(
                "✅ Set CLI session ID for {}: {:?}",
                session_id, entry.cli_session_id
            );
        }
    }

    /// Get the CLI's internal session ID for a session
    pub async fn get_cli_session_id(&self, session_id: &str) -> Option<String> {
        let metadata = self.metadata.read().await;
        metadata.get(session_id)?.cli_session_id.clone()
    }

    /// Update a session's ID from temporary UUID to real CLI session ID
    /// This is called when we receive the real session_id from system.init
    /// Returns the old session_id if the session was found and updated, None otherwise
    pub async fn update_session_id(&self, old_id: &str, new_id: &str) -> Option<String> {
        info!("🔄 Updating session ID: {} -> {}", old_id, new_id);

        // Remove session from old ID
        let (manager, metadata) = {
            let mut sessions = self.sessions.write().await;
            let manager = sessions.remove(old_id)?;

            let mut metadata_map = self.metadata.write().await;
            let metadata = metadata_map.remove(old_id)?;

            (manager, metadata)
        };

        // Update metadata with new session ID
        let updated_metadata = SessionEntry {
            session_id: new_id.to_string(),
            port: metadata.port,
            created_at: metadata.created_at,
            cli_session_id: Some(new_id.to_string()), // This is the real CLI session ID
        };

        // Re-insert with new ID
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(new_id.to_string(), manager.clone());
        }

        {
            let mut metadata_map = self.metadata.write().await;
            metadata_map.insert(new_id.to_string(), updated_metadata);
        }

        {
            let mut aliases = self.session_aliases.write().await;
            aliases.insert(old_id.to_string(), new_id.to_string());
        }

        info!(
            "✅ Session ID updated successfully: {} -> {}",
            old_id, new_id
        );
        Some(old_id.to_string())
    }

    /// Relaunch a CLI process for an existing session with --resume to restore conversation context
    pub async fn relaunch_session(&self, session_id: &str) -> Result<(), String> {
        info!("🔄 Relaunching session: {}", session_id);

        // Get the session manager
        let manager = self
            .get(session_id)
            .await
            .ok_or_else(|| format!("Session not found: {}", session_id))?;

        // For Stdio mode, just call launch_cli which handles --resume
        // For WebSocket mode, the relaunch is handled by the client
        manager.launch_cli().await?;

        info!("✅ Session relaunched: {}", session_id);
        Ok(())
    }

    /// Allocate a port for a new session (only used in WebSocket mode)
    async fn allocate_port(&self) -> Result<u16, String> {
        let mut used_ports = self.used_ports.write().await;

        // Find next available port
        for i in 0..self.port_allocator.max_sessions() {
            if let Some(port) = self.port_allocator.port_for_index(i) {
                if !used_ports.contains(&port) {
                    used_ports.insert(port);
                    debug!("🔌 Allocated port: {}", port);
                    return Ok(port);
                }
            }
        }

        Err("No available ports for new session".to_string())
    }

    /// Release a port when session is removed (only used in WebSocket mode)
    async fn release_port(&self, port: u16) {
        let mut used_ports = self.used_ports.write().await;
        used_ports.remove(&port);
        debug!("🔌 Released port: {}", port);
    }

    /// Get the number of active sessions
    pub async fn session_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }

    /// Cleanup all sessions (for shutdown)
    pub async fn cleanup_all(&self) -> Result<(), String> {
        info!("🧹 Cleaning up all sessions...");

        let session_ids: Vec<String> = {
            let sessions = self.sessions.read().await;
            sessions.keys().cloned().collect()
        };

        for session_id in session_ids {
            self.remove_session(&session_id).await?;
        }

        info!("✅ All sessions cleaned up");
        Ok(())
    }
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// Global session registry
static GLOBAL_SESSION_REGISTRY: once_cell::sync::Lazy<Arc<SessionRegistry>> =
    once_cell::sync::Lazy::new(|| Arc::new(SessionRegistry::new()));

pub fn get_session_registry() -> Arc<SessionRegistry> {
    GLOBAL_SESSION_REGISTRY.clone()
}
