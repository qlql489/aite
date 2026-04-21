// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

// Module declarations
mod claude;
mod commands;
mod logging;
mod models;
mod node;

// Re-exports for use in other modules
pub use models::*;

fn login_shell_proxy_env() -> &'static std::collections::HashMap<String, String> {
    static CACHE: std::sync::OnceLock<std::collections::HashMap<String, String>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let script = r#"for v in https_proxy http_proxy all_proxy no_proxy HTTPS_PROXY HTTP_PROXY ALL_PROXY NO_PROXY; do eval "val=\$$v"; if [ -n "$val" ]; then echo "$v=$val"; fi; done"#;
        let output = std::process::Command::new(&shell)
            .args(["-ic", script])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output();

        let mut map = std::collections::HashMap::new();
        if let Ok(output) = output {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    if let Some((key, value)) = line.split_once('=') {
                        let key = key.trim();
                        let value = value.trim();
                        if !key.is_empty() && !value.is_empty() {
                            map.insert(key.to_string(), value.to_string());
                        }
                    }
                }
            }
        }
        map
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::Layer;

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(LevelFilter::INFO),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(logging::DebugFileMakeWriter)
                .with_ansi(false)
                .with_filter(LevelFilter::DEBUG),
        )
        .init();

    tracing::info!(
        "Aite 启动: version={}, debug_log_enabled={}, debug_log_path={}",
        env!("CARGO_PKG_VERSION"),
        logging::is_debug_logging_enabled(),
        logging::get_debug_log_path().display()
    );

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(not(target_os = "windows"))]
            {
                let proxy_env = login_shell_proxy_env();
                for (key, value) in proxy_env {
                    if std::env::var(key).is_err() {
                        unsafe {
                            std::env::set_var(key, value);
                        }
                    }
                }
            }

            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            #[cfg(not(desktop))]
            let _ = app;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Claude commands
            commands::create_session,
            commands::send_message,
            commands::send_message_to_session,
            commands::send_message_with_multi_stream,
            commands::stop_streaming,
            commands::get_sdk_status,
            commands::get_commands_and_skills,
            commands::start_server,
            commands::stop_server,
            commands::is_server_running,
            commands::load_session_messages,
            commands::set_working_directory,
            commands::get_project_sessions,
            commands::check_session_exists,
            commands::delete_session_file,
            commands::get_session_file_path,
            commands::get_session_file_metadata,
            commands::respond_to_permission,
            commands::set_permission_mode,
            commands::rewind_files_to_checkpoint,
            commands::get_active_sessions,
            commands::close_session,
            // Legacy Node.js backend commands (deprecated)
            commands::start_node_backend,
            commands::stop_node_backend,
            commands::is_node_backend_running,
            // Import commands
            commands::get_projects_with_sessions,
            commands::import_project,
            commands::check_cli_configured,
            // Git commands
            commands::get_git_info,
            commands::git_pull,
            commands::git_list_branches,
            commands::git_get_current_branch,
            commands::git_init_repository,
            commands::git_checkout_branch,
            commands::detect_running_ides,
            commands::connect_ide,
            commands::disconnect_ide,
            commands::get_ide_connection_state,
            // Worktree commands
            commands::get_repo_info,
            commands::list_branches,
            commands::list_worktrees,
            commands::ensure_worktree,
            commands::remove_worktree,
            commands::track_worktree,
            commands::untrack_worktree,
            commands::get_worktree_mapping,
            commands::is_worktree_in_use,
            commands::get_all_worktree_mappings,
            // Stats commands
            commands::get_project_statistics,
            commands::get_all_projects_statistics,
            commands::get_statistics_projects,
            commands::init_stats_cache,
            commands::is_stats_loading,
            // MCP commands
            commands::get_mcp_servers,
            commands::get_mcp_server_status,
            commands::validate_mcp_server,
            commands::upsert_mcp_server,
            commands::delete_mcp_server,
            commands::toggle_mcp_server_app,
            // Config commands
            commands::get_app_config,
            commands::get_setup_completed,
            commands::set_setup_completed,
            commands::set_streaming_enabled,
            commands::get_streaming_enabled,
            commands::set_debug_enabled,
            commands::get_debug_enabled,
            commands::set_theme_color,
            commands::get_theme_color,
            commands::set_theme_mode,
            commands::get_theme_mode,
            commands::set_interface_font_size,
            commands::get_interface_font_size,
            commands::set_chat_font_size,
            commands::get_chat_font_size,
            commands::get_claude_cli_extra_args,
            commands::set_claude_cli_extra_args,
            commands::get_provider_config,
            commands::save_provider_config,
            // CLI commands
            commands::check_claude_cli,
            commands::install_claude_cli,
            commands::install_claude_cli_with_node_runtime,
            commands::check_node_env,
            commands::install_node_env,
            commands::install_portable_git,
            // Skills commands
            commands::get_skills,
            commands::get_skill_content,
            commands::save_skill,
            commands::create_skill,
            commands::import_skill_folder,
            commands::create_command,
            commands::delete_skill,
            // File commands
            commands::save_temp_file,
            commands::get_file_size,
            commands::copy_file,
            commands::check_file_access,
            commands::read_file_base64,
            commands::read_project_tree,
            commands::read_project_tree_children,
            commands::search_project_files,
            commands::read_project_file,
            commands::write_project_file,
            commands::ensure_data_dir,
            commands::get_data_file_path,
            commands::read_data_file,
            commands::backup_data_file,
            commands::append_data_log,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    let cleanup_started = Arc::new(AtomicBool::new(false));

    app.run(move |_app_handle, event| {
        if !matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            return;
        }

        if cleanup_started.swap(true, Ordering::SeqCst) {
            return;
        }

        tracing::info!("App exiting, cleaning up Claude sessions...");

        if let Err(error) =
            tauri::async_runtime::block_on(crate::claude::session_registry::get_session_registry().cleanup_all())
        {
            tracing::error!("Failed to clean up Claude sessions on exit: {}", error);
        }
    });
}
