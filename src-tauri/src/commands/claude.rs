// Claude-related Tauri Commands
//
// These commands communicate directly with Claude Code CLI
// using the configured connection mode, bypassing the Node.js backend.

use crate::claude::session_file::SessionFileReader;
use crate::claude::session_registry::get_session_registry;
use crate::commands::git::get_git_info;
use crate::models::*;
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Event names for streaming
pub const EVENT_STREAM_START: &str = "claude:stream_start";
pub const EVENT_STREAM_DATA: &str = "claude:stream_data";
pub const EVENT_STREAM_END: &str = "claude:stream_end";
pub const EVENT_STREAM_ERROR: &str = "claude:stream_error";
pub const EVENT_CONNECTION_STATUS: &str = "claude:connection_status";
pub const EVENT_SESSION_STATUS: &str = "claude:session_status";
pub const EVENT_SESSION_CREATED: &str = "claude:session_created";
pub const EVENT_COMMANDS_UPDATED: &str = "claude:commands_updated";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFileMetadata {
    pub exists: bool,
    pub modified_at_ms: Option<u64>,
    pub size: u64,
}

/// Create a new Claude session or connect to an existing one
/// If session_id is provided, use it for an existing session; otherwise create a new one
/// If project_path is provided, set working directory before launching CLI (so CLI starts in project dir)
/// If use_worktree is true and branch is provided, create a git worktree for the branch
#[tauri::command]
pub async fn create_session(
    session_id: Option<String>,
    project_path: Option<String>,
    thinking_level: Option<String>,
    _provider_id: Option<String>,
    model: Option<String>,
    provider_env: Option<SessionProviderEnv>,
    use_worktree: Option<bool>,
    branch: Option<String>,
    create_branch: Option<bool>,
    app: AppHandle,
) -> Result<Session, String> {
    if let Some(sid) = &session_id {
        info!("Creating Claude session with existing ID: {}", sid);
    } else {
        info!("Creating new Claude session");
    }

    // Handle worktree mode
    let mut final_project_path = project_path.clone();
    if use_worktree.unwrap_or(false) {
        if let Some(ref path) = project_path {
            if let Some(ref branch_name) = branch {
                info!("Worktree mode requested for branch: {}", branch_name);

                // Get repo info
                let repo_info = crate::commands::worktree::get_repo_info(path.clone())?;

                // Create or get worktree
                let result = crate::commands::worktree::ensure_worktree(
                    repo_info.repo_root.clone(),
                    branch_name.clone(),
                    Some(repo_info.default_branch),
                    create_branch,
                )?;

                let worktree_path_clone = result.worktree_path.clone();
                let worktree_path_for_mapping = result.worktree_path.clone();
                final_project_path = Some(result.worktree_path);

                // Track the worktree mapping (session_id will be updated after session is created)
                let repo_root_clone = repo_info.repo_root.clone();
                let branch_clone = branch_name.clone();

                // Store worktree info to track after session creation
                tokio::spawn(async move {
                    // Wait a bit for session to be created
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    // The session ID is now available, but we need to update the mapping
                    // For now, we'll track it with a placeholder and clean up later
                    info!("Worktree created at: {}", worktree_path_clone);
                });

                // Note: We'll update the mapping with the actual session ID after session creation
                let mapping = crate::commands::worktree::WorktreeMapping {
                    session_id: "".to_string(), // Will be updated
                    repo_root: repo_root_clone,
                    branch: branch_clone,
                    worktree_path: worktree_path_for_mapping,
                    created_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                };
                crate::commands::worktree::track_worktree(mapping)?;
            }
        }
    }

    if let Some(ref path) = final_project_path {
        info!(
            "Project path (working dir) will be set before CLI launch: {}",
            path
        );
    }

    let registry = get_session_registry();

    // Create a new independent session using the configured connection mode.
    // If session_id is provided, use it to resume an existing session.
    // If project_path is provided, set working dir before launch so CLI starts in project directory.
    let (actual_session_id, manager) = registry
        .create_session_with_id(
            session_id,
            final_project_path.clone(),
            thinking_level,
            model,
            provider_env,
        )
        .await?;

    // Set the AppHandle for the session manager to enable frontend events
    manager.set_app_handle(app.clone()).await;

    // Send session created event to notify frontend that session is ready
    let _ = app.emit(
        EVENT_SESSION_CREATED,
        serde_json::json!({
            "sessionId": actual_session_id,
            "projectPath": final_project_path
        }),
    );
    info!(
        "✅ Sent session_created event for session {}",
        actual_session_id
    );

    // Send initial connecting status event AFTER creating the session
    // This ensures we use the correct session ID

    // Start background task to monitor connection and send connected event
    let manager_clone = manager.clone();
    let app_clone = app.clone();
    let session_id_clone = actual_session_id.clone();
    tokio::spawn(async move {
        // Wait for connection (WebSocket or Stdio)
        let mut attempts = 0;
        let max_attempts = 200; // 10 seconds

        while attempts < max_attempts {
            if manager_clone.is_connected().await {
                // Send connected status event
                let _ = app_clone.emit(
                    EVENT_CONNECTION_STATUS,
                    serde_json::json!({
                        "sessionId": session_id_clone,
                        "status": "connected"
                    }),
                );
                info!(
                    "✅ Sent connected status event for session {}",
                    session_id_clone
                );
                return;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            attempts += 1;
        }
        warn!(
            "⚠️ Timeout waiting for connection for session {}",
            session_id_clone
        );
    });

    Ok(Session::new_with_id(actual_session_id))
}

/// Send a message to a Claude session
#[tauri::command]
pub async fn send_message(session_id: String, content: String) -> Result<Message, String> {
    info!("Sending message to session: {}", session_id);

    let registry = get_session_registry();
    let manager = registry.get(&session_id).await.ok_or_else(|| {
        warn!(
            "⚠️ send_message_with_multi_stream could not find session in registry: {}",
            session_id
        );
        format!("Session not found: {}", session_id)
    })?;

    info!(
        "✅ Resolved manager for send_message_with_multi_stream: session_id={}",
        session_id
    );

    manager.send_message(session_id, content).await
}

/// Send a message to a Claude session with streaming support
#[tauri::command]
pub async fn send_message_to_session(
    session_id: String,
    content: String,
    app: AppHandle,
) -> Result<u32, String> {
    info!("Sending message to session {} with streaming", session_id);

    let registry = get_session_registry();
    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    // Clone AppHandle for use in callbacks
    let app1 = app.clone();
    let app2 = app.clone();
    let session_id_for_data = session_id.clone();
    let session_id_for_complete = session_id.clone();

    // Emit stream start event with session_id
    app.emit(EVENT_STREAM_START, session_id.clone())
        .map_err(|e| e.to_string())?;

    // Send message and stream response
    let _result = manager
        .send_message_streaming(
            session_id,
            content,
            Box::new(move |data| {
                // Emit streaming data to frontend with session_id
                let payload = serde_json::json!({
                    "sessionId": session_id_for_data,
                    "data": data
                });
                let _ = app1.emit(EVENT_STREAM_DATA, payload);
            }),
            Box::new(move |complete| {
                // Emit completion event with session_id
                let payload = serde_json::json!({
                    "sessionId": session_id_for_complete,
                    "message": complete
                });
                let _ = app2.emit(EVENT_STREAM_END, payload);
            }),
        )
        .await;

    // Return a listener ID (simplified)
    Ok(1)
}

/// Send a message to a Claude session with multi-message streaming support
#[tauri::command]
pub async fn send_message_with_multi_stream(
    session_id: String,
    content: String,
    content_blocks: Option<Vec<crate::claude::protocol::ContentBlock>>,
    app: AppHandle,
) -> Result<u32, String> {
    info!(
        "Sending message to session {} with multi-message streaming",
        session_id
    );

    let registry = get_session_registry();
    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    // Clone AppHandle for use in callbacks
    let app1 = app.clone();
    let app2 = app.clone();
    let app3 = app.clone();
    let session_id_for_data = session_id.clone();
    let session_id_for_complete = session_id.clone();

    // Emit stream start event with session_id
    app.emit(EVENT_STREAM_START, session_id.clone())
        .map_err(|e| e.to_string())?;

    // Create message callback to emit individual messages
    let message_callback = Box::new(move |msg: crate::models::Message| {
        // Emit each message to frontend
        let _ = app3.emit("claude:message", msg);
    });

    // Send message and stream response
    let _result = manager
        .send_message_streaming_with_messages(
            session_id,
            content,
            content_blocks,
            Box::new(move |data| {
                // Emit streaming data to frontend with session_id
                let payload = serde_json::json!({
                    "sessionId": session_id_for_data,
                    "data": data
                });
                let _ = app1.emit(EVENT_STREAM_DATA, payload);
            }),
            message_callback,
            Box::new(move |complete| {
                // Emit completion event with session_id
                let payload = serde_json::json!({
                    "sessionId": session_id_for_complete,
                    "message": complete
                });
                let _ = app2.emit(EVENT_STREAM_END, payload);
            }),
        )
        .await;

    // Return a listener ID (simplified)
    Ok(1)
}

/// Stop streaming response
#[tauri::command]
pub async fn stop_streaming(session_id: String) -> Result<bool, String> {
    info!("Stopping streaming for session: {}", session_id);

    let registry = get_session_registry();
    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    manager.stop_streaming(session_id).await?;

    Ok(true)
}

/// Get the current SDK status
#[tauri::command]
pub async fn get_sdk_status() -> Result<SdkStatus, String> {
    let registry = get_session_registry();
    let session_count = registry.session_count().await;

    if session_count > 0 {
        // Return initialized status if we have any active sessions
        Ok(SdkStatus {
            initialized: true,
            cli_path: Some(format!("{} active session(s)", session_count)),
            error: None,
        })
    } else {
        Ok(SdkStatus::not_initialized())
    }
}

/// Get commands (from initialize) and skills (from system.init) from the CLI
#[tauri::command]
pub async fn get_commands_and_skills(
    session_id: Option<String>,
) -> Result<
    (
        Option<Vec<crate::claude::protocol::CommandInfo>>,
        Option<Vec<String>>,
    ),
    String,
> {
    let registry = get_session_registry();

    // If session_id is provided, get from that specific session
    if let Some(sid) = session_id {
        if let Some(manager) = registry.get(&sid).await {
            return manager.get_commands_and_skills().await;
        }
        return Err(format!("Session not found: {}", sid));
    }

    // Otherwise, get from the first available session
    let session_ids = registry.list_sessions().await;
    if let Some(sid) = session_ids.first() {
        if let Some(manager) = registry.get(sid).await {
            return manager.get_commands_and_skills().await;
        }
    }

    // No sessions available
    Ok((None, None))
}

/// Start the WebSocket server
#[tauri::command]
pub async fn start_server() -> Result<bool, String> {
    info!("Starting Claude WebSocket server (legacy command, sessions are created on demand)");

    // Sessions are now created on demand with create_session
    // This command is kept for backward compatibility
    Ok(true)
}

/// Stop the WebSocket server and CLI
#[tauri::command]
pub async fn stop_server() -> Result<bool, String> {
    info!("Stopping all Claude WebSocket servers and sessions");

    let registry = get_session_registry();
    registry.cleanup_all().await?;

    Ok(false)
}

/// Check if the server is running
#[tauri::command]
pub async fn is_server_running() -> Result<bool, String> {
    let registry = get_session_registry();
    Ok(registry.session_count().await > 0)
}

/// Get all active session IDs (connected sessions)
#[tauri::command]
pub async fn get_active_sessions() -> Result<Vec<String>, String> {
    let registry = get_session_registry();
    let sessions = registry.list_sessions().await;
    info!("Active sessions: {:?}", sessions);
    Ok(sessions)
}

/// Close a specific session and stop its CLI process
#[tauri::command]
pub async fn close_session(session_id: String) -> Result<bool, String> {
    info!("Closing session: {}", session_id);

    let registry = get_session_registry();

    // Check if session exists
    if !registry.exists(&session_id).await {
        warn!("Session not found: {}", session_id);
        return Ok(false);
    }

    // Remove and stop the session (this will stop the CLI process)
    registry.remove_session(&session_id).await?;

    info!("Session closed successfully: {}", session_id);
    Ok(true)
}

#[tauri::command]
pub async fn rewind_files_to_checkpoint(
    session_id: String,
    checkpoint_uuid: String,
    cwd: String,
) -> Result<bool, String> {
    info!(
        "Rewinding files: session_id={}, checkpoint_uuid={}",
        session_id, checkpoint_uuid
    );

    let registry = get_session_registry();

    if let Some(manager) = registry.get(&session_id).await {
        match manager.rewind_files(checkpoint_uuid.clone()).await {
            Ok(()) => return Ok(true),
            Err(error) => warn!("Fast rewind_files failed, falling back to CLI: {}", error),
        }
    }

    run_rewind_files_cli(&session_id, &checkpoint_uuid, &cwd).await?;
    Ok(true)
}

/// Respond to a permission request from Claude CLI with three options
#[tauri::command]
pub async fn respond_to_permission(
    session_id: String,
    request_id: String,
    action: String, // "approve" | "approve_always" | "reject"
    reason: Option<String>,
    suggestion: Option<String>, // 给 Claude 的建议
    updated_input: Option<serde_json::Value>,
) -> Result<bool, String> {
    use crate::claude::session::PermissionDecision;

    info!(
        "Responding to permission request: session={}, request={}, action={}",
        session_id, request_id, action
    );

    let registry = get_session_registry();
    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    // 根据用户操作创建决定
    let decision = match action.as_str() {
        "approve" => {
            // 仅批准当前请求
            PermissionDecision::Approve {
                updated_input: updated_input.clone(),
                allow_tools: None,
            }
        }
        "approve_always" => {
            if let Some(rule) = manager.remember_permission_rule(request_id.clone()).await? {
                info!(
                    "Remembered permission rule for session {}: {}",
                    session_id, rule
                );
            }
            PermissionDecision::Approve {
                updated_input,
                allow_tools: None,
            }
        }
        "reject" => {
            // 拒绝并提供建议
            PermissionDecision::Reject {
                reason: reason.unwrap_or_else(|| "Rejected by user".to_string()),
                suggestion,
            }
        }
        _ => {
            return Err(format!("Unknown action: {}", action));
        }
    };

    // Send the decision through the wrapper
    manager
        .send_permission_decision(request_id.clone(), decision)
        .await?;
    Ok(true)
}

// Legacy Node.js backend commands (deprecated)
// These are kept for backward compatibility but are no-ops

#[tauri::command]
pub async fn start_node_backend() -> Result<bool, String> {
    tracing::warn!("start_node_backend is deprecated. Use start_server instead.");
    start_server().await
}

#[tauri::command]
pub async fn stop_node_backend() -> Result<bool, String> {
    tracing::warn!("stop_node_backend is deprecated. Use stop_server instead.");
    stop_server().await
}

#[tauri::command]
pub async fn is_node_backend_running() -> Result<bool, String> {
    is_server_running().await
}

/// Load messages from a Claude CLI session file
#[tauri::command]
pub async fn load_session_messages(
    project_path: String,
    session_id: String,
) -> Result<Vec<Message>, String> {
    info!(
        "Loading session messages: project={}, session={}",
        project_path, session_id
    );

    let reader =
        SessionFileReader::new().map_err(|e| format!("Failed to create session reader: {}", e))?;

    reader.load_messages(&project_path, &session_id)
}

/// Set the working directory for the Claude CLI
#[tauri::command]
pub async fn set_working_directory(
    project_path: String,
    session_id: Option<String>,
) -> Result<bool, String> {
    info!(
        "Setting working directory: {} for session: {:?}",
        project_path, session_id
    );

    let registry = get_session_registry();

    // If session_id is provided, set for that specific session
    if let Some(sid) = session_id {
        if let Some(manager) = registry.get(&sid).await {
            manager
                .set_working_directory(std::path::PathBuf::from(project_path))
                .await?;
            return Ok(true);
        }
        return Err(format!("Session not found: {}", sid));
    }

    // Otherwise, set for all existing sessions
    let session_ids = registry.list_sessions().await;
    for sid in session_ids {
        if let Some(manager) = registry.get(&sid).await {
            if let Err(e) = manager
                .set_working_directory(std::path::PathBuf::from(project_path.clone()))
                .await
            {
                warn!("Failed to set working directory for session {}: {}", sid, e);
            }
        }
    }

    Ok(true)
}

/// Extract title from a message (与前端 extractTitleFromFirstMessage 逻辑完全一致)
///
/// 算法：
/// 1. 优先使用 msg.content 字段
/// 2. 如果 content 是 JSON 字符串（以 `{` 或 `[` 开头），解析并提取 text 类型的块
/// 3. 提取前 50 个字符作为标题
/// 4. 如果 content 为空，尝试从 contentBlocks 中提取文本
fn extract_title_from_message(msg: &Message) -> Option<String> {
    // 优先使用 content 字段
    let content = match &msg.content {
        MessageContent::Text(text) => Some(text.clone()),
        MessageContent::Blocks(blocks) => {
            if let Ok(json) = serde_json::to_string(blocks) {
                Some(json)
            } else {
                None
            }
        }
    };

    if let Some(content_str) = content {
        let trimmed = content_str.trim();

        if !trimmed.is_empty() {
            // 如果 content 是 JSON 字符串，尝试解析并提取 text 类型的块
            let final_content = if trimmed.starts_with('{') || trimmed.starts_with('[') {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let Some(array) = parsed.as_array() {
                        array
                            .iter()
                            .find(|block| {
                                block.get("type").and_then(|t| t.as_str()) == Some("text")
                            })
                            .and_then(|block| {
                                block
                                    .get("content")
                                    .or_else(|| block.get("text"))
                                    .and_then(|c| c.as_str())
                            })
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let content_to_use = final_content.unwrap_or_else(|| trimmed.to_string());

            // 提取前 50 个字符作为标题
            let title = if content_to_use.chars().count() > 50 {
                format!("{}...", content_to_use.chars().take(50).collect::<String>())
            } else {
                content_to_use
            };

            return Some(title);
        }
    }

    // 如果 content 为空，尝试从 contentBlocks 中提取文本
    if let Some(ref blocks) = msg.content_blocks {
        if let Some(block) = blocks
            .iter()
            .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
        {
            if let Some(content) = block.get("content").or_else(|| block.get("text")) {
                if let Some(text_str) = content.as_str() {
                    let trimmed = text_str.trim();
                    if !trimmed.is_empty() {
                        let title = if trimmed.len() > 50 {
                            format!("{}...", &trimmed[..50])
                        } else {
                            trimmed.to_string()
                        };
                        return Some(title);
                    }
                }
            }
        }
    }

    None
}

/// Get all session IDs for a project
#[tauri::command]
pub async fn get_project_sessions(project_path: String) -> Result<Vec<SessionInfo>, String> {
    debug!("Getting sessions for project: {}", project_path);

    let reader =
        SessionFileReader::new().map_err(|e| format!("Failed to create session reader: {}", e))?;

    let session_ids = reader.list_sessions(&project_path)?;
    debug!("Found {} session IDs: {:?}", session_ids.len(), session_ids);

    // Convert session IDs to SessionInfo
    let sessions: Result<Vec<SessionInfo>, String> = session_ids
        .into_iter()
        .map(|id| {
            // Get session file path to check file metadata
            let session_path = reader.get_session_path(&project_path, &id);

            // Get file modification time
            let modified = std::fs::metadata(&session_path)
                .and_then(|m| m.modified())
                .ok()
                .map(|t| {
                    // Convert to Unix timestamp
                    t.duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                })
                .unwrap_or(0);

            // Get file size
            let file_size = std::fs::metadata(&session_path)
                .map(|m| m.len())
                .unwrap_or(0);

            // Load messages once to get both title and count
            let (title, message_count) = match reader.load_messages(&project_path, &id) {
                Ok(msgs) => {
                    // 优先从 User 消息中提取标题，跳过 /clear 等清空命令
                    // 与前端的 extractTitleFromFirstMessage 逻辑保持一致
                    let title = msgs
                        .iter()
                        .filter(|msg| msg.role == MessageRole::User)
                        .filter(|msg| {
                            // 跳过系统命令（/clear、/exit、/compact 等）
                            if let MessageContent::Text(content) = &msg.content {
                                let trimmed = content.trim();
                                let is_clear_cmd = trimmed.starts_with("/clear")
                                    || trimmed.starts_with("/exit")
                                    || trimmed.starts_with("/compact");
                                if is_clear_cmd {
                                    return false;
                                }
                            }
                            true
                        })
                        .find_map(|msg| extract_title_from_message(msg))
                        .or_else(|| {
                            msgs.iter()
                                .filter(|msg| msg.role == MessageRole::Assistant)
                                .find_map(|msg| extract_title_from_message(msg))
                        })
                        .unwrap_or_else(|| "Untitled".to_string());
                    (title, msgs.len())
                }
                Err(_) => ("Untitled".to_string(), 0),
            };

            Ok(SessionInfo {
                session_id: id.clone(),
                title,
                message_count,
                file_size,
                updated_at: modified,
                created_at: modified, // Use modified as created for now
            })
        })
        .collect();

    // Log each SessionInfo for debugging
    if let Ok(ref session_list) = sessions {
        for (i, session) in session_list.iter().enumerate() {
            debug!(
                "SessionInfo[{}]: session_id={}, title={}, message_count={}",
                i, session.session_id, session.title, session.message_count
            );
        }
    }

    sessions
}

/// Check if a session exists and is active
#[tauri::command]
pub async fn check_session_exists(session_id: String) -> Result<bool, String> {
    let registry = get_session_registry();
    Ok(registry.exists(&session_id).await)
}

/// Set the permission mode for a session
#[tauri::command]
pub async fn set_permission_mode(session_id: String, mode: String) -> Result<bool, String> {
    info!(
        "Setting permission mode for session {}: {}",
        session_id, mode
    );

    let registry = get_session_registry();
    let manager = registry
        .get(&session_id)
        .await
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    // Parse the permission mode
    use crate::claude::protocol::PermissionMode;
    let permission_mode = PermissionMode::from_str(&mode)
        .ok_or_else(|| format!("Invalid permission mode: {}", mode))?;

    // Set the permission mode
    manager.set_permission_mode(permission_mode).await;

    Ok(true)
}

async fn run_rewind_files_cli(
    session_id: &str,
    checkpoint_uuid: &str,
    cwd: &str,
) -> Result<(), String> {
    if !is_uuid_like(session_id) {
        return Err(format!("Invalid session_id format: {}", session_id));
    }

    if !is_uuid_like(checkpoint_uuid) {
        return Err(format!(
            "Invalid checkpoint_uuid format: {}",
            checkpoint_uuid
        ));
    }

    let claude_bin = find_claude_binary_for_rewind()?;
    let runtime_env = crate::commands::cli::resolve_claude_runtime_env(Path::new(&claude_bin))?;
    let output = Command::new(&claude_bin)
        .args(["--resume", session_id, "--rewind-files", checkpoint_uuid])
        .current_dir(cwd)
        .env("PATH", &runtime_env.path)
        .env("CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING", "1")
        .env_remove("CLAUDECODE")
        .output()
        .await
        .map_err(|e| format!("Failed to run claude --rewind-files: {}", e))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("rewind_files failed: {}", stderr))
}

fn find_claude_binary_for_rewind() -> Result<String, String> {
    crate::commands::cli::resolve_claude_binary_path()
        .map(|path| path.to_string_lossy().to_string())
}

fn is_uuid_like(value: &str) -> bool {
    value.len() >= 32 && value.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
}

/// Delete a session file from disk
#[tauri::command]
pub async fn delete_session_file(project_path: String, session_id: String) -> Result<bool, String> {
    info!(
        "Deleting session file: session_id={}, project_path={}",
        session_id, project_path
    );

    let reader = SessionFileReader::new()?;
    let session_path = reader.get_session_path(&project_path, &session_id);

    info!("Session file path: {}", session_path.display());

    // Check if session file exists
    if !session_path.exists() {
        return Err(format!(
            "Session file not found: {}",
            session_path.display()
        ));
    }

    // Delete the session file
    std::fs::remove_file(&session_path)
        .map_err(|e| format!("Failed to delete session file: {}", e))?;

    info!(
        "Successfully deleted session file: {}",
        session_path.display()
    );

    Ok(true)
}

/// Get the absolute session file path on disk
#[tauri::command]
pub async fn get_session_file_path(
    project_path: String,
    session_id: String,
) -> Result<String, String> {
    let reader = SessionFileReader::new()?;
    let session_path = reader.get_session_path(&project_path, &session_id);
    Ok(session_path.display().to_string())
}

/// Get session file metadata for change detection
#[tauri::command]
pub async fn get_session_file_metadata(
    project_path: String,
    session_id: String,
) -> Result<SessionFileMetadata, String> {
    let reader = SessionFileReader::new()?;
    let session_path = reader.get_session_path(&project_path, &session_id);

    match std::fs::metadata(&session_path) {
        Ok(metadata) => {
            let modified_at_ms = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as u64);

            Ok(SessionFileMetadata {
                exists: true,
                modified_at_ms,
                size: metadata.len(),
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SessionFileMetadata {
            exists: false,
            modified_at_ms: None,
            size: 0,
        }),
        Err(error) => Err(format!("Failed to read session file metadata: {}", error)),
    }
}
