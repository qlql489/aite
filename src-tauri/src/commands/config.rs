// Application configuration commands
// 管理应用级别的配置，如流式输出开关、连接模式等

use crate::claude::cli_args::{reserved_cli_flags_for_display, validate_custom_cli_args};
use crate::models::{
    get_aite_config_dir, AppConfig, ConnectionMode, ProviderConfigPayload, ThemeMode,
    AITE_APP_CONFIG_FILE,
};
use std::fs;
use std::path::PathBuf;

const MIN_INTERFACE_FONT_SIZE: u16 = 14;
const MAX_INTERFACE_FONT_SIZE: u16 = 20;
const MIN_CHAT_FONT_SIZE: u16 = 12;
const MAX_CHAT_FONT_SIZE: u16 = 24;

/// 获取应用配置文件路径
fn get_app_config_path() -> PathBuf {
    get_aite_config_dir().join(AITE_APP_CONFIG_FILE)
}

fn clamp_interface_font_size(size: u16) -> u16 {
    size.clamp(MIN_INTERFACE_FONT_SIZE, MAX_INTERFACE_FONT_SIZE)
}

fn clamp_chat_font_size(size: u16) -> u16 {
    size.clamp(MIN_CHAT_FONT_SIZE, MAX_CHAT_FONT_SIZE)
}

/// 读取应用配置
fn read_app_config() -> AppConfig {
    let config_path = get_app_config_path();

    if !config_path.exists() {
        tracing::info!("应用配置文件不存在，使用默认配置: {:?}", config_path);
        return AppConfig::default();
    }

    match fs::read_to_string(&config_path) {
        Ok(content) if !content.trim().is_empty() => {
            match serde_json::from_str::<AppConfig>(&content) {
                Ok(config) => {
                    tracing::info!(
                        "成功读取应用配置: setup_completed={}, streaming_enabled={}, debug_enabled={}, connection_mode={}, theme_color={}, theme_mode={:?}, interface_font_size={}, chat_font_size={}",
                        config.setup_completed,
                        config.streaming_enabled,
                        config.debug_enabled,
                        config.connection_mode,
                        config.theme_color,
                        config.theme_mode,
                        config.interface_font_size,
                        config.chat_font_size
                    );
                    config
                }
                Err(e) => {
                    tracing::warn!("解析应用配置失败: {}, 使用默认配置", e);
                    AppConfig::default()
                }
            }
        }
        _ => AppConfig::default(),
    }
}

/// 写入应用配置
fn write_app_config(config: &AppConfig) -> Result<(), String> {
    let config_path = get_app_config_path();

    // 确保目录存在
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            tracing::error!("创建配置目录失败: {}", e);
            e.to_string()
        })?;
    }

    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&config_path, content).map_err(|e| {
        tracing::error!("写入应用配置失败: {}", e);
        e.to_string()
    })?;

    tracing::info!(
        "成功写入应用配置: setup_completed={}, streaming_enabled={}, debug_enabled={}, connection_mode={}, theme_color={}, theme_mode={:?}, interface_font_size={}, chat_font_size={}",
        config.setup_completed,
        config.streaming_enabled,
        config.debug_enabled,
        config.connection_mode,
        config.theme_color,
        config.theme_mode,
        config.interface_font_size,
        config.chat_font_size
    );
    Ok(())
}

/// 获取应用配置
#[tauri::command]
pub fn get_app_config() -> Result<AppConfig, String> {
    Ok(read_app_config())
}

#[tauri::command]
pub fn get_setup_completed() -> Result<bool, String> {
    Ok(read_app_config().setup_completed)
}

#[tauri::command]
pub fn set_setup_completed(completed: bool) -> Result<bool, String> {
    let mut config = read_app_config();
    config.setup_completed = completed;
    write_app_config(&config)?;
    Ok(completed)
}

/// 设置流式输出开关
#[tauri::command]
pub fn set_streaming_enabled(enabled: bool) -> Result<bool, String> {
    tracing::info!("设置流式输出: {}", enabled);

    let mut config = read_app_config();
    config.streaming_enabled = enabled;

    write_app_config(&config)?;
    Ok(enabled)
}

/// 获取流式输出开关状态
#[tauri::command]
pub fn get_streaming_enabled() -> Result<bool, String> {
    Ok(read_app_config().streaming_enabled)
}

/// 设置调试日志开关
#[tauri::command]
pub fn set_debug_enabled(enabled: bool) -> Result<bool, String> {
    let mut config = read_app_config();
    config.debug_enabled = enabled;

    write_app_config(&config)?;

    tracing::info!(
        "调试日志已{}，输出文件: {}",
        if enabled { "启用" } else { "关闭" },
        crate::logging::get_debug_log_path().display()
    );

    Ok(enabled)
}

/// 获取调试日志开关状态
#[tauri::command]
pub fn get_debug_enabled() -> Result<bool, String> {
    Ok(read_app_config().debug_enabled)
}

/// 设置连接模式
#[tauri::command]
pub fn set_connection_mode(mode: ConnectionMode) -> Result<ConnectionMode, String> {
    tracing::info!("设置连接模式: {}", mode);

    let mut config = read_app_config();
    config.connection_mode = mode;

    write_app_config(&config)?;
    Ok(mode)
}

/// 获取连接模式
#[tauri::command]
pub fn get_connection_mode() -> Result<ConnectionMode, String> {
    Ok(read_app_config().connection_mode)
}

/// 设置主题颜色
#[tauri::command]
pub fn set_theme_color(color: String) -> Result<String, String> {
    tracing::info!("设置主题颜色: {}", color);

    let mut config = read_app_config();
    config.theme_color = color.clone();

    write_app_config(&config)?;
    Ok(color)
}

/// 获取主题颜色
#[tauri::command]
pub fn get_theme_color() -> Result<String, String> {
    Ok(read_app_config().theme_color)
}

/// 设置主题模式
#[tauri::command]
pub fn set_theme_mode(mode: ThemeMode) -> Result<ThemeMode, String> {
    tracing::info!("设置主题模式: {:?}", mode);

    let mut config = read_app_config();
    config.theme_mode = mode;

    write_app_config(&config)?;
    Ok(mode)
}

/// 获取主题模式
#[tauri::command]
pub fn get_theme_mode() -> Result<ThemeMode, String> {
    Ok(read_app_config().theme_mode)
}

#[tauri::command]
pub fn set_interface_font_size(size: u16) -> Result<u16, String> {
    let next_size = clamp_interface_font_size(size);
    tracing::info!("设置界面基础字号: {} -> {}", size, next_size);

    let mut config = read_app_config();
    config.interface_font_size = next_size;

    write_app_config(&config)?;
    Ok(next_size)
}

#[tauri::command]
pub fn get_interface_font_size() -> Result<u16, String> {
    Ok(clamp_interface_font_size(
        read_app_config().interface_font_size,
    ))
}

#[tauri::command]
pub fn set_chat_font_size(size: u16) -> Result<u16, String> {
    let next_size = clamp_chat_font_size(size);
    tracing::info!("设置对话内容字号: {} -> {}", size, next_size);

    let mut config = read_app_config();
    config.chat_font_size = next_size;

    write_app_config(&config)?;
    Ok(next_size)
}

#[tauri::command]
pub fn get_chat_font_size() -> Result<u16, String> {
    Ok(clamp_chat_font_size(read_app_config().chat_font_size))
}

#[tauri::command]
pub fn get_claude_cli_extra_args() -> Result<Vec<String>, String> {
    Ok(read_app_config().claude_cli_extra_args)
}

#[tauri::command]
pub fn set_claude_cli_extra_args(args: Vec<String>) -> Result<Vec<String>, String> {
    tracing::info!("设置 Claude CLI 自定义启动参数: {:?}", args);

    validate_custom_cli_args(&args)?;

    let mut config = read_app_config();
    config.claude_cli_extra_args = args.clone();

    write_app_config(&config)?;

    tracing::info!(
        "Claude CLI 自定义启动参数已保存，保留参数列表: {}",
        reserved_cli_flags_for_display().join(", ")
    );

    Ok(args)
}

#[tauri::command]
pub fn get_provider_config() -> Result<ProviderConfigPayload, String> {
    let config = read_app_config();
    Ok(ProviderConfigPayload {
        providers: config.providers,
        active_provider_id: config.active_provider_id,
        inherit_system_config: config.inherit_system_config,
    })
}

#[tauri::command]
pub fn save_provider_config(payload: ProviderConfigPayload) -> Result<bool, String> {
    let mut config = read_app_config();
    config.providers = payload.providers;
    config.active_provider_id = payload.active_provider_id;
    config.inherit_system_config = payload.inherit_system_config;
    write_app_config(&config)?;

    if config.inherit_system_config {
        crate::claude::cli_settings::restore_env_to_claude_settings()?;
    }

    Ok(true)
}
