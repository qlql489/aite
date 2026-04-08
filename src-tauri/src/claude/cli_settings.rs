use crate::models::{get_aite_config_dir, AppConfig, AITE_APP_CONFIG_FILE};
use serde_json::{Map, Value};
use std::fs;
use std::path::PathBuf;

fn get_app_config_path() -> PathBuf {
    get_aite_config_dir().join(AITE_APP_CONFIG_FILE)
}

fn read_app_config() -> AppConfig {
    let config_path = get_app_config_path();

    if !config_path.exists() {
        return AppConfig::default();
    }

    match fs::read_to_string(&config_path) {
        Ok(content) if !content.trim().is_empty() => {
            serde_json::from_str::<AppConfig>(&content).unwrap_or_default()
        }
        _ => AppConfig::default(),
    }
}

fn write_app_config(config: &AppConfig) -> Result<(), String> {
    let config_path = get_app_config_path();
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let content = serde_json::to_string_pretty(config).map_err(|error| error.to_string())?;
    fs::write(config_path, content).map_err(|error| error.to_string())
}

fn get_claude_settings_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法定位用户 Home 目录".to_string())?;
    Ok(home.join(".claude").join("settings.json"))
}

fn read_claude_settings() -> Result<Value, String> {
    let settings_path = get_claude_settings_path()?;

    if !settings_path.exists() {
        return Ok(Value::Object(Map::new()));
    }

    let content = fs::read_to_string(&settings_path).map_err(|error| error.to_string())?;
    if content.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }

    serde_json::from_str::<Value>(&content).map_err(|error| {
        format!(
            "解析 Claude settings 失败: {} ({})",
            settings_path.display(),
            error
        )
    })
}

fn write_claude_settings(settings: &Value) -> Result<(), String> {
    let settings_path = get_claude_settings_path()?;
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let content = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(&settings_path, content).map_err(|error| {
        format!(
            "写入 Claude settings 失败: {} ({})",
            settings_path.display(),
            error
        )
    })
}

fn ensure_root_map_mut(settings: &mut Value) -> Result<&mut Map<String, Value>, String> {
    if !settings.is_object() {
        *settings = Value::Object(Map::new());
    }

    settings
        .as_object_mut()
        .ok_or_else(|| "Claude settings 根节点不是对象".to_string())
}

pub fn detach_env_from_claude_settings() -> Result<(), String> {
    let mut app_config = read_app_config();
    let mut settings = read_claude_settings()?;
    let root = ensure_root_map_mut(&mut settings)?;

    if app_config.claude_settings_anthropic_env_backup.is_none() {
        app_config.claude_settings_anthropic_env_backup =
            Some(root.remove("env").unwrap_or(Value::Null));
    } else {
        root.remove("env");
    }

    write_claude_settings(&settings)?;
    write_app_config(&app_config)?;

    tracing::info!("已备份并移除 Claude settings 中的 env 字段");
    Ok(())
}

pub fn restore_env_to_claude_settings() -> Result<(), String> {
    let mut app_config = read_app_config();
    let Some(backup) = app_config.claude_settings_anthropic_env_backup.take() else {
        tracing::info!("未找到 Claude settings 的 env 备份，跳过恢复");
        return Ok(());
    };

    let mut settings = read_claude_settings()?;
    let root = ensure_root_map_mut(&mut settings)?;
    root.remove("env");

    if !backup.is_null() {
        root.insert("env".to_string(), backup);
    }

    write_claude_settings(&settings)?;
    write_app_config(&app_config)?;

    tracing::info!("已恢复 Claude settings 中备份的 env 字段");
    Ok(())
}
