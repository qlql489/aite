// Import CLI Sessions - 按项目导入 Claude CLI 会话
//
// 此模块实现从 Claude CLI 扫描和导入会话的功能
// 与 CodePilot 不同，这里以项目为单位组织会话

use crate::claude::message_normalizer::{normalize_cli_message, NormalizedMessageKind};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// 安全截取字符串前 N 个字符用于日志显示（避免多字节字符截断问题）
fn safe_truncate_for_log(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// 最大单个 JSONL 文件大小（50MB），防止内存问题
const MAX_JSONL_FILE_SIZE: u64 = 50 * 1024 * 1024;

/// Claude CLI 会话信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliSessionInfo {
    pub session_id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub file_size: u64,
    /// 工作目录（从 JSONL 的第一个 user message 中提取）
    pub cwd: String,
}

/// 项目会话信息（包含该项目的所有会话）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectWithSessions {
    /// 项目名称（从 cwd 提取）
    pub project_name: String,
    /// 工作目录（从 JSONL 提取的权威路径）
    pub cwd: String,
    /// 项目路径（编码后的文件夹路径，仅用于内部引用）
    pub project_path: String,
    /// 该项目的所有会话
    pub sessions: Vec<CliSessionInfo>,
    /// 会话总数
    pub session_count: usize,
    /// 最后活动时间
    pub last_activity: String,
    /// 总文件大小（所有会话文件大小之和）
    pub total_file_size: u64,
    /// Claude CLI 版本
    pub cli_version: String,
    /// 最近会话的预览文本（第一个用户消息）
    pub preview: String,
}

/// Claude CLI JSONL 条目类型
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum JsonlEntry {
    #[serde(rename = "queue-operation")]
    QueueOperation { name: String, timestamp: String },
    #[serde(rename = "user")]
    User { params: serde_json::Value },
    #[serde(rename = "assistant")]
    Assistant { params: serde_json::Value },
}

/// 获取 Claude CLI projects 目录
fn get_claude_projects_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("无法获取用户目录"))?;
    let projects_dir = home.join(".claude").join("projects");

    if !projects_dir.exists() {
        return Err(anyhow::anyhow!(
            "Claude CLI projects 目录不存在: {:?}",
            projects_dir
        ));
    }

    Ok(projects_dir)
}

/// 获取 Claude CLI 版本
fn get_cli_version() -> String {
    // 尝试从 ~/.claude/config.json 读取版本信息
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return "".to_string(),
    };

    let config_path = home.join(".claude").join("config.json");

    if let Ok(content) = fs::read_to_string(&config_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(version) = json.get("version").and_then(|v| v.as_str()) {
                return version.to_string();
            }
        }
    }

    // 不再尝试运行命令，直接返回空字符串
    // 运行命令可能会挂起，导致 UI 无法响应
    "".to_string()
}

/// 从路径中提取项目名称
///
/// 优先使用 cwd（从 JSONL 提取的真实路径）的最后一部分
/// 如果 cwd 为空或无效，回退到编码路径的解码
fn extract_project_name(path: &str) -> String {
    if path.is_empty() {
        return "Unknown".to_string();
    }

    // 尝试解析为路径（处理 cwd 情况）
    if path.starts_with('/') || path.starts_with('~') {
        // 这是绝对路径，直接使用最后一部分
        let path_buf = PathBuf::from(path);
        if let Some(folder_name) = path_buf.file_name().and_then(|n| n.to_str()) {
            if !folder_name.is_empty() {
                return folder_name.to_string();
            }
        }
    }

    // 回退到编码路径的处理（以 - 分隔的路径）
    // 尝试解码：将 - 替换回 /，然后提取项目名称
    let clean_path = path.strip_prefix('-').unwrap_or(path);

    // 尝试将编码路径解码回绝对路径
    let decoded_path = format!("/{}", clean_path.replace('-', "/"));

    // 从解码后的路径中提取项目名称（最后一部分）
    let path_buf = PathBuf::from(&decoded_path);
    if let Some(folder_name) = path_buf.file_name().and_then(|n| n.to_str()) {
        if !folder_name.is_empty() {
            return folder_name.to_string();
        }
    }

    // 最后的后备方案：直接返回原始路径
    clean_path.to_string()
}

/// 解析单个 JSONL 会话文件（简化版，只提取必要信息）
/// 将 ISO 8601 时间戳转换为 Unix 时间戳（秒）
fn parse_timestamp_to_secs(timestamp_str: &str) -> String {
    // 尝试解析 ISO 8601 格式
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp_str) {
        return dt.timestamp().to_string();
    }

    // 尝试解析其他常见格式
    for format in [
        "%Y-%m-%dT%H:%M:%S%.fZ",
        "%Y-%m-%dT%H:%M:%SZ",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(timestamp_str, format) {
            return dt.and_utc().timestamp().to_string();
        }
    }

    // 如果解析失败，返回 0
    "0".to_string()
}

fn parse_session_file(file_path: &Path) -> Result<CliSessionInfo> {
    let metadata = fs::metadata(file_path)?;
    let file_size = metadata.len();

    // 跳过过大的文件
    if file_size > MAX_JSONL_FILE_SIZE {
        warn!("跳过过大的 JSONL 文件 {:?}: {} bytes", file_path, file_size);
        return anyhow::bail!("文件过大: {} bytes", file_size);
    }

    let content = fs::read_to_string(file_path)?;

    let mut message_count = 0;
    let mut created_at: Option<String> = None;
    let mut updated_at: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut first_user_message: Option<String> = None;

    // 解析 JSONL 文件（只读取前 500 行以限制处理时间）
    for line in content.lines().take(500) {
        if line.is_empty() {
            continue;
        }

        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
            // 优先尝试从任何条目中提取 cwd（不限制类型）
            if cwd.is_none() {
                if let Some(cwd_value) = entry.get("cwd").and_then(|c| c.as_str()) {
                    debug!("    在条目中找到 cwd: {}", cwd_value);
                    cwd = Some(cwd_value.to_string());
                }
            }

            if let Some(entry_type) = entry.get("type").and_then(|t| t.as_str()) {
                match entry_type {
                    "queue-operation" => {
                        if let Some(name) = entry.get("name").and_then(|n| n.as_str()) {
                            let timestamp = entry.get("timestamp").and_then(|t| t.as_str());
                            match name {
                                "session_created" => {
                                    if let Some(ts) = timestamp {
                                        // 转换为 Unix 时间戳（秒）
                                        created_at = Some(parse_timestamp_to_secs(ts));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "user" => {
                        message_count += 1;
                        // 提取第一条有效消息内容作为标题（与 get_project_sessions 逻辑一致）
                        if first_user_message.is_none() {
                            if let Some(title) = extract_title_from_json_entry(&entry) {
                                debug!(
                                    "[parse_session_file] Extracted title from user message: '{}'",
                                    title
                                );
                                first_user_message = Some(title);
                            }
                        }
                        // 更新时间（用户消息作为活动时间）
                        if let Some(ts) = entry.get("timestamp").and_then(|t| t.as_str()) {
                            // 转换为 Unix 时间戳（秒）
                            updated_at = Some(parse_timestamp_to_secs(ts));
                        }
                    }
                    "assistant" => {
                        // 如果还没有找到标题，也尝试从 assistant 消息中提取
                        if first_user_message.is_none() {
                            if let Some(title) = extract_title_from_json_entry(&entry) {
                                info!("[parse_session_file] Extracted title from assistant message: '{}'", title);
                                first_user_message = Some(title);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let file_stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    // 格式化时间戳
    fn format_time(time: std::io::Result<std::time::SystemTime>) -> String {
        time.ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| {
                let secs = d.as_secs();
                format!("{}", secs)
            })
            .unwrap_or_else(|| "0".to_string())
    }

    Ok(CliSessionInfo {
        session_id: file_stem.to_string(),
        title: first_user_message.unwrap_or_else(|| "Untitled".to_string()),
        created_at: created_at.unwrap_or_else(|| format_time(metadata.created())),
        updated_at: updated_at.unwrap_or_else(|| format_time(metadata.modified())),
        message_count,
        file_size,
        cwd: cwd.unwrap_or_else(|| "".to_string()),
    })
}

/// 从消息条目中提取标题（与 claude.rs 的 extract_title_from_message 逻辑一致）
///
/// Claude CLI JSONL 格式：
/// {
///   "type": "user",
///   "message": {
///     "role": "user",
///     "content": "用户消息内容" 或 [{...}, {...}]
///   },
///   ...
/// }
fn extract_title_from_json_entry(entry: &serde_json::Value) -> Option<String> {
    debug!("[extract_title_from_json_entry] Starting extraction");

    // 检查 isMeta 字段，过滤掉元数据消息
    if entry
        .get("isMeta")
        .and_then(|m| m.as_bool())
        .unwrap_or(false)
    {
        debug!("[extract_title_from_json_entry] Filtering out isMeta message");
        return None;
    }

    // 获取 message.content
    let content = entry.get("message")?.get("content")?;

    // 处理 content 字段
    let content_str = if let Some(s) = content.as_str() {
        s.to_string()
    } else if let Some(arr) = content.as_array() {
        // 尝试序列化为 JSON 字符串
        if let Ok(json_str) = serde_json::to_string(arr) {
            json_str
        } else {
            warn!("[extract_title_from_json_entry] Failed to serialize array to JSON");
            return None;
        }
    } else if let Some(obj) = content.as_object() {
        // 尝试序列化对象为 JSON 字符串
        if let Ok(json_str) = serde_json::to_string(obj) {
            json_str
        } else {
            warn!("[extract_title_from_json_entry] Failed to serialize object to JSON");
            return None;
        }
    } else {
        return None;
    };

    let trimmed = content_str.trim();

    if trimmed.is_empty() {
        return None;
    }

    // 规范化内容：过滤或转换系统标签（与 session_file.rs 的 normalize_message_content 逻辑一致）
    let normalized = normalize_cli_message(trimmed);
    let normalized_content = match normalized {
        Some(normalized) if normalized.kind == NormalizedMessageKind::System => return None,
        Some(normalized) => normalized.text,
        None => return None,
    };

    debug!(
        "[extract_title_from_json_entry] Normalized content: '{}'",
        safe_truncate_for_log(&normalized_content, 50)
    );

    // 跳过系统命令（/clear、/exit、/compact 等），不适合作为标题
    {
        let t = normalized_content.trim();
        if t.starts_with("/clear")
            || t.starts_with("/exit")
            || t.starts_with("/compact")
            || t == "/clear"
            || t == "/exit"
            || t == "/compact"
        {
            debug!(
                "[extract_title_from_json_entry] Skipping clear command: {}",
                t
            );
            return None;
        }
    }

    // 如果 normalized_content 是 JSON 字符串，尝试解析并提取 text 类型的块
    // 否则直接使用 normalized_content
    let content_to_use =
        if normalized_content.starts_with('{') || normalized_content.starts_with('[') {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&normalized_content) {
                if let Some(array) = parsed.as_array() {
                    // 查找 text 类型块
                    if let Some(text_content) = array
                        .iter()
                        .find(|block| block.get("type").and_then(|t| t.as_str()) == Some("text"))
                        .and_then(|block| {
                            block
                                .get("content")
                                .or_else(|| block.get("text"))
                                .and_then(|c| c.as_str())
                        })
                    {
                        text_content.to_string()
                    } else {
                        debug!("[extract_title_from_json_entry] No text block found in JSON array");
                        return None; // 没有找到 text 块，过滤掉这条消息
                    }
                } else {
                    debug!("[extract_title_from_json_entry] JSON is not an array");
                    return None; // 不是数组，过滤掉这条消息
                }
            } else {
                debug!("[extract_title_from_json_entry] Failed to parse JSON");
                return None; // 解析失败，过滤掉这条消息
            }
        } else {
            // 不是 JSON 格式，直接使用 normalized_content
            normalized_content
        };

    // 提取前 50 个字符作为标题
    let title = if content_to_use.chars().count() > 50 {
        let truncated: String = content_to_use.chars().take(50).collect();
        format!("{}...", truncated)
    } else {
        content_to_use
    };

    Some(title)
}

/// 扫描项目目录下的所有会话文件
fn scan_project_sessions(project_dir: &Path) -> Result<Vec<CliSessionInfo>> {
    let mut sessions = Vec::new();

    let entries = fs::read_dir(project_dir)
        .with_context(|| format!("无法读取项目目录: {:?}", project_dir))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // 只处理 .jsonl 文件
        if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            match parse_session_file(&path) {
                Ok(session) => sessions.push(session),
                Err(e) => {
                    warn!("解析会话文件失败 {:?}: {}", path, e);
                }
            }
        }
    }

    // 按更新时间排序（最新的在前）
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(sessions)
}

/// 获取所有包含会话的项目
#[tauri::command]
pub async fn get_projects_with_sessions() -> Result<Vec<ProjectWithSessions>, String> {
    info!("开始扫描 Claude CLI 项目会话");

    let projects_dir = get_claude_projects_dir().map_err(|e| e.to_string())?;

    let cli_version = get_cli_version();

    let mut project_map: HashMap<String, ProjectWithSessions> = HashMap::new();

    let entries =
        fs::read_dir(&projects_dir).map_err(|e| format!("无法读取 projects 目录: {}", e))?;

    let mut processed_count = 0;
    let total_projects = entries.size_hint().0;

    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
        let path = entry.path();

        // 只处理目录
        if path.is_dir() {
            processed_count += 1;
            info!(
                "处理项目 {}/{}: {:?}",
                processed_count,
                total_projects,
                path.file_name().unwrap_or_default()
            );

            // 获取目录名并去除可能的前导破折号
            let clean_project_path = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .strip_prefix('-')
                .unwrap_or("")
                .to_string();

            let project_path = if clean_project_path.is_empty() {
                // 如果去除前导后为空，使用原始路径的最后部分
                path.to_string_lossy().to_string()
            } else {
                clean_project_path
            };

            match scan_project_sessions(&path) {
                Ok(sessions) => {
                    if !sessions.is_empty() {
                        // 找到第一个有 cwd 的会话（不一定是最新更新的）
                        let session_with_cwd = sessions
                            .iter()
                            .find(|s| !s.cwd.is_empty())
                            .or_else(|| sessions.first());

                        let cwd_value = session_with_cwd
                            .map(|s| s.cwd.clone())
                            .unwrap_or_else(|| "".to_string());
                        info!("  会话 cwd: {}", cwd_value);

                        let cwd = session_with_cwd
                            .and_then(|s| {
                                if s.cwd.is_empty() {
                                    None
                                } else {
                                    Some(s.cwd.clone())
                                }
                            })
                            .unwrap_or_else(|| project_path.clone());

                        // 从 cwd 中提取项目名称
                        let project_name = extract_project_name(&cwd);

                        info!("  编码路径: {}", path.display());
                        info!("  清理后路径: {}", project_path);
                        info!("  最终 cwd: {}", cwd);
                        info!("  项目名称: {}", project_name);

                        let session_count = sessions.len();
                        let last_activity = sessions
                            .first()
                            .map(|s| s.updated_at.clone())
                            .unwrap_or_else(|| "Unknown".to_string());

                        // 计算总文件大小
                        let total_file_size: u64 = sessions.iter().map(|s| s.file_size).sum();

                        // 获取最近会话的预览文本（第一个用户消息）
                        let preview = sessions
                            .first()
                            .map(|s| s.title.clone())
                            .unwrap_or_else(|| "(no preview)".to_string());

                        // 使用 cwd 作为 key，避免重复
                        project_map.insert(
                            cwd.clone(),
                            ProjectWithSessions {
                                project_name,
                                cwd,
                                project_path,
                                sessions,
                                session_count,
                                last_activity,
                                total_file_size,
                                cli_version: cli_version.clone(),
                                preview,
                            },
                        );
                    }
                }
                Err(e) => {
                    warn!("扫描项目会话失败 {:?}: {}", path, e);
                }
            }
        }
    }

    // 转换为向量并按最后活动时间排序
    let mut projects: Vec<ProjectWithSessions> = project_map.into_values().collect();

    projects.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));

    info!("扫描完成，找到 {} 个包含会话的项目", projects.len());

    Ok(projects)
}

/// 导入项目及其所有会话
#[tauri::command]
pub async fn import_project(project_path: String) -> Result<String, String> {
    info!("导入项目: {}", project_path);

    // 尝试不同的路径格式
    let project_dir = if Path::new(&project_path).exists() {
        // 原始路径存在，直接使用
        PathBuf::from(project_path.clone())
    } else {
        // 尝试添加前导破折号
        let with_prefix = format!("-{}", project_path);
        if Path::new(&with_prefix).exists() {
            info!("使用带前导的路径: {}", with_prefix);
            PathBuf::from(with_prefix.clone())
        } else {
            // 尝试完整路径（~/.claude/projects/...）
            let projects_dir = get_claude_projects_dir().map_err(|e| e.to_string())?;
            let full_path = projects_dir.join(&project_path);
            if full_path.exists() {
                info!("使用完整路径: {}", full_path.display());
                full_path
            } else {
                // 尝试完整路径加前导
                let full_path_with_prefix = projects_dir.join(&with_prefix);
                if full_path_with_prefix.exists() {
                    info!(
                        "使用完整路径（带前导）: {}",
                        full_path_with_prefix.display()
                    );
                    full_path_with_prefix
                } else {
                    return Err(format!("项目目录不存在: {}", project_path));
                }
            }
        }
    };

    // 扫描会话
    let sessions =
        scan_project_sessions(&project_dir).map_err(|e| format!("扫描会话失败: {}", e))?;

    // TODO: 创建项目记录和会话记录
    // 这需要与现有的项目管理系统集成

    Ok(format!("成功导入项目，包含 {} 个会话", sessions.len()))
}

/// 检查 Claude CLI 是否已配置
#[tauri::command]
pub async fn check_cli_configured() -> Result<bool, String> {
    let projects_dir = get_claude_projects_dir().map_err(|e| e.to_string())?;

    Ok(projects_dir.exists())
}
