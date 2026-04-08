use serde::{Deserialize, Serialize};
/**
 * 统计数据 Tauri 命令
 * 从 Claude 会话日志文件读取使用统计数据
 * 支持异步加载和缓存
 */
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::command;
use tracing::{debug, info, warn};

pub mod stats_cache;
pub use stats_cache::StatsCache;

/// 项目信息（用于选择器）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub path: String,
    pub name: String,
}

/// JSONL 中的 cwd 信息
/// 实际 JSONL 格式中，cwd 字段直接在顶层，不嵌套在 params 中
#[derive(Debug, Deserialize)]
struct JsonlCwdEntry {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    cwd: Option<String>,
    // 保留 params 字段以防旧格式
    params: Option<CwdParams>,
}

#[derive(Debug, Deserialize)]
struct CwdParams {
    cwd: Option<String>,
}

/// Token 使用数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageData {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
}

/// 会话摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub timestamp: i64,
    pub model: String,
    pub usage: UsageData,
    pub cost: f64,
    pub summary: Option<String>,
}

/// 按模型聚合的统计数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub total_cost: f64,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub session_count: usize,
}

/// 每日使用数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyUsage {
    pub date: String,
    pub sessions: usize,
    pub usage: UsageData,
    pub cost: f64,
    pub models_used: Vec<String>,
}

/// 周对比数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeeklyComparison {
    pub current_week: WeekData,
    pub last_week: WeekData,
    pub trends: TrendData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeekData {
    pub sessions: usize,
    pub cost: f64,
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendData {
    pub sessions: f64,
    pub cost: f64,
    pub tokens: f64,
}

/// 项目完整统计数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStatistics {
    pub project_path: String,
    pub project_name: String,
    pub total_sessions: usize,
    pub total_usage: UsageData,
    pub estimated_cost: f64,
    pub sessions: Vec<SessionSummary>,
    pub daily_usage: Vec<DailyUsage>,
    pub weekly_comparison: WeeklyComparison,
    pub by_model: Vec<ModelUsage>,
    pub last_updated: i64,
}

/// Claude 日志消息（JSONL 格式）
#[derive(Debug, Deserialize)]
struct ClaudeLogMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    timestamp: Option<serde_json::Value>,
    message: Option<ClaudeMessage>,
    request_id: Option<String>,
    cost_usd: Option<f64>,
    summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeMessage {
    id: Option<String>,
    model: Option<String>,
    usage: Option<ClaudeUsage>,
}

#[derive(Debug, Deserialize)]
struct ClaudeUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

/// 日期范围筛选器
#[derive(Debug, Clone, Copy)]
pub enum DateRange {
    Days7,
    Days30,
    All,
}

/// 项目范围
#[derive(Debug, Clone, Copy)]
pub enum ProjectScope {
    Current,
    All,
}

// ========== Token 定价 ==========

/// 模型定价（每百万 tokens）
struct ModelPricing {
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
}

fn get_model_pricing(model: &str) -> ModelPricing {
    let model_lower = model.to_lowercase();

    if model_lower.contains("opus-4") || model_lower.contains("claude-opus-4") {
        ModelPricing {
            input: 15.0,
            output: 75.0,
            cache_write: 18.75,
            cache_read: 1.50,
        }
    } else if model_lower.contains("haiku-4") || model_lower.contains("claude-haiku-4") {
        ModelPricing {
            input: 0.8,
            output: 4.0,
            cache_write: 1.0,
            cache_read: 0.08,
        }
    } else {
        // 默认使用 Sonnet 4 定价
        ModelPricing {
            input: 3.0,
            output: 15.0,
            cache_write: 3.75,
            cache_read: 0.30,
        }
    }
}

// ========== 辅助函数 ==========

/// 获取 Claude 项目目录
fn get_claude_projects_dir() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(".claude").join("projects"))
}

/// 从 JSONL 文件中提取 cwd 路径
/// 参考 import.rs 的实现，直接从顶层读取 cwd 字段
fn extract_cwd_from_jsonl(file_path: &Path) -> Option<String> {
    let file = fs::File::open(file_path).ok()?;
    let reader = std::io::BufReader::new(file);

    debug!("正在提取 cwd: {:?}", file_path);

    for line in std::io::BufRead::lines(reader) {
        if let Ok(line_content) = line {
            // 首先尝试解析为通用 JSON 值，直接从顶层读取 cwd
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line_content) {
                // 优先从顶层获取 cwd（不限制类型）
                if let Some(cwd_value) = entry.get("cwd").and_then(|c| c.as_str()) {
                    let cwd = cwd_value.to_string();
                    if !cwd.is_empty() {
                        debug!("找到 cwd: {} (从文件: {:?})", cwd, file_path);
                        return Some(cwd);
                    }
                }

                // 回退：尝试从 params 中获取（兼容旧格式）
                if let Some(params) = entry.get("params") {
                    if let Some(cwd_value) = params.get("cwd").and_then(|c| c.as_str()) {
                        let cwd = cwd_value.to_string();
                        if !cwd.is_empty() {
                            debug!("找到 cwd (从 params): {} (从文件: {:?})", cwd, file_path);
                            return Some(cwd);
                        }
                    }
                }
            }
        }
    }

    debug!("未找到 cwd: {:?}", file_path);
    None
}

/// 解析时间戳
fn parse_timestamp(value: &serde_json::Value) -> Option<i64> {
    if let Some(s) = value.as_str() {
        // 尝试解析 ISO 8601 字符串
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return Some(dt.timestamp_millis());
        }
    } else if let Some(n) = value.as_i64() {
        // 判断是秒还是毫秒
        if n < 1000000000000 {
            return Some(n * 1000);
        } else {
            return Some(n);
        }
    }
    None
}

/// 将项目路径转换为 ~/.claude/projects 中的文件夹名称
///
/// Claude CLI 的文件夹命名规则：
/// - 开头的 '/' 替换为 '-'
/// - 所有 '/' 替换为 '-'
/// - 所有 '_' 替换为 '-'
/// - 所有非 ASCII 字符替换为 '-'
fn get_project_folder_name(project_path: &str) -> String {
    // 标准化路径：将反斜杠转换为正斜杠
    let normalized_path = project_path.replace('\\', "/");

    // 处理 Windows 盘符
    let clean_path = if let Some(rest) = normalized_path.strip_prefix(":/") {
        format!("{}-{}", &normalized_path[0..1].to_lowercase(), rest)
    } else if let Some(rest) = normalized_path.strip_prefix(":\\") {
        format!("{}-{}", &normalized_path[0..1].to_lowercase(), rest)
    } else {
        normalized_path.clone()
    };

    // 处理中文和特殊字符
    let clean_path = clean_path
        .chars()
        .map(|c| if c.is_ascii() && c != '_' { c } else { '-' })
        .collect::<String>();

    // 将 '/' 替换为 '-'
    clean_path.replace('/', "-")
}

/// 解析单个会话文件
fn parse_session_file(
    file_path: &Path,
    processed_hashes: &mut HashSet<String>,
) -> Option<SessionSummary> {
    let content = fs::read_to_string(file_path).ok()?;

    let mut usage = UsageData {
        input_tokens: 0,
        output_tokens: 0,
        cache_write_tokens: 0,
        cache_read_tokens: 0,
        total_tokens: 0,
    };

    let mut first_timestamp: Option<i64> = None;
    let mut model = "unknown".to_string();
    let mut total_cost = 0.0;
    let mut session_summary: Option<String> = None;

    for line in content.lines() {
        if let Ok(data) = serde_json::from_str::<ClaudeLogMessage>(line) {
            // 记录第一条消息的时间戳
            if first_timestamp.is_none() {
                if let Some(ts_value) = data.timestamp {
                    if let Some(ts) = parse_timestamp(&ts_value) {
                        first_timestamp = Some(ts);
                    }
                }
            }

            // 查找 summary 类型的消息
            if data.msg_type.as_deref() == Some("summary") {
                if let Some(summary) = data.summary {
                    session_summary = Some(summary);
                }
            }

            // 查找 assistant 消息中的 usage 数据
            if data.msg_type.as_deref() == Some("assistant") {
                if let Some(message) = data.message {
                    // 去重检查
                    if let (Some(id), Some(request_id)) = (message.id, data.request_id) {
                        let unique_hash = format!("{}:{}", id, request_id);
                        if processed_hashes.contains(&unique_hash) {
                            continue;
                        }
                        processed_hashes.insert(unique_hash);
                    }

                    if let Some(msg_usage) = message.usage {
                        // 跳过无意义的空 token 条目
                        let has_tokens = msg_usage.input_tokens.unwrap_or(0) > 0
                            || msg_usage.output_tokens.unwrap_or(0) > 0
                            || msg_usage.cache_creation_input_tokens.unwrap_or(0) > 0
                            || msg_usage.cache_read_input_tokens.unwrap_or(0) > 0;

                        if !has_tokens {
                            continue;
                        }

                        // 提取模型名称和计算成本
                        if let Some(ref m) = message.model {
                            if model == "unknown" {
                                model = m.clone();
                            }
                        }

                        // 累加 token 使用量
                        let input_tokens = msg_usage.input_tokens.unwrap_or(0);
                        let output_tokens = msg_usage.output_tokens.unwrap_or(0);
                        let cache_write_tokens = msg_usage.cache_creation_input_tokens.unwrap_or(0);
                        let cache_read_tokens = msg_usage.cache_read_input_tokens.unwrap_or(0);

                        usage.input_tokens += input_tokens;
                        usage.output_tokens += output_tokens;
                        usage.cache_write_tokens += cache_write_tokens;
                        usage.cache_read_tokens += cache_read_tokens;

                        // 计算成本
                        if let Some(cost) = data.cost_usd {
                            total_cost += cost;
                        } else if let Some(ref m) = message.model {
                            let pricing = get_model_pricing(m);
                            total_cost += (input_tokens as f64) * pricing.input / 1_000_000.0
                                + (output_tokens as f64) * pricing.output / 1_000_000.0
                                + (cache_write_tokens as f64) * pricing.cache_write / 1_000_000.0
                                + (cache_read_tokens as f64) * pricing.cache_read / 1_000_000.0;
                        }
                    }
                }
            }
        }
    }

    usage.total_tokens = usage.input_tokens
        + usage.output_tokens
        + usage.cache_write_tokens
        + usage.cache_read_tokens;

    if usage.total_tokens == 0 {
        return None;
    }

    let session_id = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp = first_timestamp.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    Some(SessionSummary {
        session_id,
        timestamp,
        model,
        usage,
        cost: total_cost,
        summary: session_summary,
    })
}

/// 聚合日期数据
fn aggregate_daily_usage(sessions: &[SessionSummary]) -> Vec<DailyUsage> {
    let mut daily_map: HashMap<String, DailyUsage> = HashMap::new();

    for session in sessions {
        let date = chrono::DateTime::from_timestamp(session.timestamp / 1000, 0)
            .unwrap()
            .format("%Y-%m-%d")
            .to_string();

        let entry = daily_map.entry(date.clone()).or_insert_with(|| DailyUsage {
            date: date.clone(),
            sessions: 0,
            usage: UsageData {
                input_tokens: 0,
                output_tokens: 0,
                cache_write_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 0,
            },
            cost: 0.0,
            models_used: Vec::new(),
        });

        entry.sessions += 1;
        entry.usage.input_tokens += session.usage.input_tokens;
        entry.usage.output_tokens += session.usage.output_tokens;
        entry.usage.cache_write_tokens += session.usage.cache_write_tokens;
        entry.usage.cache_read_tokens += session.usage.cache_read_tokens;
        entry.usage.total_tokens += session.usage.total_tokens;
        entry.cost += session.cost;

        if !entry.models_used.contains(&session.model) {
            entry.models_used.push(session.model.clone());
        }
    }

    let mut result: Vec<_> = daily_map.into_values().collect();
    result.sort_by(|a, b| a.date.cmp(&b.date));
    result
}

/// 聚合按模型的数据
fn aggregate_by_model(sessions: &[SessionSummary]) -> Vec<ModelUsage> {
    let mut model_map: HashMap<String, ModelUsage> = HashMap::new();

    for session in sessions {
        let entry = model_map
            .entry(session.model.clone())
            .or_insert_with(|| ModelUsage {
                model: session.model.clone(),
                total_cost: 0.0,
                total_tokens: 0,
                input_tokens: 0,
                output_tokens: 0,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                session_count: 0,
            });

        entry.total_cost += session.cost;
        entry.input_tokens += session.usage.input_tokens;
        entry.output_tokens += session.usage.output_tokens;
        entry.cache_creation_tokens += session.usage.cache_write_tokens;
        entry.cache_read_tokens += session.usage.cache_read_tokens;
        entry.total_tokens += session.usage.total_tokens;
        entry.session_count += 1;
    }

    let mut result: Vec<_> = model_map.into_values().collect();
    result.sort_by(|a, b| b.total_cost.partial_cmp(&a.total_cost).unwrap());
    result
}

/// 计算周对比数据
fn calculate_weekly_comparison(sessions: &[SessionSummary]) -> WeeklyComparison {
    let now = chrono::Utc::now();
    let one_week_ago = now - chrono::Duration::weeks(1);
    let two_weeks_ago = now - chrono::Duration::weeks(2);

    let current_week_sessions: Vec<_> = sessions
        .iter()
        .filter(|s| chrono::DateTime::from_timestamp(s.timestamp / 1000, 0).unwrap() > one_week_ago)
        .collect();

    let last_week_sessions: Vec<_> = sessions
        .iter()
        .filter(|s| {
            let dt = chrono::DateTime::from_timestamp(s.timestamp / 1000, 0).unwrap();
            dt > two_weeks_ago && dt <= one_week_ago
        })
        .collect();

    let current_week = WeekData {
        sessions: current_week_sessions.len(),
        cost: current_week_sessions.iter().map(|s| s.cost).sum(),
        tokens: current_week_sessions
            .iter()
            .map(|s| s.usage.total_tokens)
            .sum(),
    };

    let last_week = WeekData {
        sessions: last_week_sessions.len(),
        cost: last_week_sessions.iter().map(|s| s.cost).sum(),
        tokens: last_week_sessions
            .iter()
            .map(|s| s.usage.total_tokens)
            .sum(),
    };

    let trends = TrendData {
        sessions: if last_week.sessions == 0 {
            0.0
        } else {
            ((current_week.sessions as f64 - last_week.sessions as f64) / last_week.sessions as f64)
                * 100.0
        },
        cost: if last_week.cost == 0.0 {
            0.0
        } else {
            ((current_week.cost - last_week.cost) / last_week.cost) * 100.0
        },
        tokens: if last_week.tokens == 0 {
            0.0
        } else {
            ((current_week.tokens as f64 - last_week.tokens as f64) / last_week.tokens as f64)
                * 100.0
        },
    };

    WeeklyComparison {
        current_week,
        last_week,
        trends,
    }
}

/// 按日期范围筛选会话
fn filter_sessions_by_date_range(
    sessions: Vec<SessionSummary>,
    range: DateRange,
) -> Vec<SessionSummary> {
    if matches!(range, DateRange::All) {
        return sessions;
    }

    let days = match range {
        DateRange::Days7 => 7,
        DateRange::Days30 => 30,
        DateRange::All => return sessions,
    };

    let cutoff = chrono::Utc::now() - chrono::Duration::days(days);

    sessions
        .into_iter()
        .filter(|s| chrono::DateTime::from_timestamp(s.timestamp / 1000, 0).unwrap() > cutoff)
        .collect()
}

// ========== 命令实现（内部函数，供缓存调用） ==========

/// 获取所有有会话数据的项目列表（内部实现）
pub fn get_statistics_projects_impl() -> Result<Vec<ProjectInfo>, String> {
    let claude_dir = get_claude_projects_dir().ok_or("无法获取 Claude 项目目录")?;

    if !claude_dir.exists() {
        warn!("Claude 项目目录不存在");
        return Ok(Vec::new());
    }

    let entries =
        fs::read_dir(&claude_dir).map_err(|e| format!("无法读取 Claude 项目目录: {}", e))?;

    let mut projects_map: HashMap<String, ProjectInfo> = HashMap::new();

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();

        // 只处理目录
        if path.is_dir() {
            let folder_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

            debug!("扫描文件夹 [{:?}]", folder_name);

            // 扫描该目录下的 JSONL 文件，提取 cwd 信息
            if let Ok(file_entries) = fs::read_dir(&path) {
                let mut jsonl_count = 0;
                for file_entry in file_entries.filter_map(|e| e.ok()) {
                    let file_path = file_entry.path();
                    if file_path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                        jsonl_count += 1;
                        // 从 JSONL 文件中提取 cwd
                        if let Some(cwd) = extract_cwd_from_jsonl(&file_path) {
                            // 提取项目名称
                            let project_name = PathBuf::from(&cwd)
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Unknown")
                                .to_string();

                            debug!("找到项目: {} (cwd: {})", project_name, cwd);

                            // 使用 cwd 作为路径标识
                            projects_map
                                .entry(cwd.clone())
                                .or_insert_with(|| ProjectInfo {
                                    path: cwd.clone(),
                                    name: project_name,
                                });
                        }
                    }
                }
                debug!(
                    "文件夹 {:?} 包含 {} 个 JSONL 文件",
                    folder_name, jsonl_count
                );
            }
        }
    }

    // 转换为向量并按名称排序
    let mut projects: Vec<_> = projects_map.into_values().collect();
    projects.sort_by(|a, b| a.name.cmp(&b.name));

    // for p in &projects {
    //     info!("项目列表: {} -> {}", p.name, p.path);
    // }

    Ok(projects)
}

// ========== Tauri 命令（带缓存支持） ==========

/// 获取所有有会话数据的项目列表
#[command]
pub fn get_statistics_projects() -> Result<Vec<ProjectInfo>, String> {
    StatsCache::global().get_projects_list()
}

/// 获取当前项目的使用统计（带缓存）
#[command]
pub fn get_project_statistics(project_path: String) -> Result<ProjectStatistics, String> {
    StatsCache::global().get_project_stats(&project_path)
}

/// 获取当前项目的使用统计（内部实现）
pub fn get_project_statistics_impl(project_path: &str) -> Result<ProjectStatistics, String> {
    info!("获取项目统计: project_path={}", project_path);

    let claude_dir = get_claude_projects_dir().ok_or("无法获取 Claude 项目目录")?;

    let project_folder_name = get_project_folder_name(project_path);
    let project_dir = claude_dir.join(&project_folder_name);

    info!("项目文件夹名: {}", project_folder_name);
    info!("项目目录: {:?}", project_dir);

    // 提取项目名称并克隆路径以拥有所有权
    let project_path_owned = project_path.to_string();
    let project_name = PathBuf::from(project_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    if !project_dir.exists() {
        warn!("项目目录不存在: {:?}", project_dir);
        return Ok(ProjectStatistics {
            project_path: project_path_owned,
            project_name,
            total_sessions: 0,
            total_usage: UsageData {
                input_tokens: 0,
                output_tokens: 0,
                cache_write_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 0,
            },
            estimated_cost: 0.0,
            sessions: Vec::new(),
            daily_usage: Vec::new(),
            weekly_comparison: WeeklyComparison {
                current_week: WeekData {
                    sessions: 0,
                    cost: 0.0,
                    tokens: 0,
                },
                last_week: WeekData {
                    sessions: 0,
                    cost: 0.0,
                    tokens: 0,
                },
                trends: TrendData {
                    sessions: 0.0,
                    cost: 0.0,
                    tokens: 0.0,
                },
            },
            by_model: Vec::new(),
            last_updated: chrono::Utc::now().timestamp_millis(),
        });
    }

    // 读取所有会话文件
    let files: Vec<_> = fs::read_dir(&project_dir)
        .map_err(|e| format!("无法读取项目目录: {}", e))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .map(|entry| entry.path())
        .collect();

    info!("找到 {} 个 JSONL 文件", files.len());

    let mut sessions: Vec<SessionSummary> = Vec::new();
    let mut processed_hashes = HashSet::new();
    let mut total_usage = UsageData {
        input_tokens: 0,
        output_tokens: 0,
        cache_write_tokens: 0,
        cache_read_tokens: 0,
        total_tokens: 0,
    };
    let mut total_cost = 0.0;

    for file_path in &files {
        if let Some(session) = parse_session_file(file_path, &mut processed_hashes) {
            total_usage.input_tokens += session.usage.input_tokens;
            total_usage.output_tokens += session.usage.output_tokens;
            total_usage.cache_write_tokens += session.usage.cache_write_tokens;
            total_usage.cache_read_tokens += session.usage.cache_read_tokens;
            total_usage.total_tokens += session.usage.total_tokens;
            total_cost += session.cost;
            sessions.push(session);
        }
    }

    // 按时间戳排序
    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    info!("成功解析 {} 个会话", sessions.len());

    // 按成本排序
    let mut sessions_by_cost = sessions.clone();
    sessions_by_cost.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap());

    // 合并去重
    let mut top_sessions = std::collections::HashMap::new();
    for session in sessions_by_cost.iter().take(100) {
        top_sessions.insert(session.session_id.clone(), session.clone());
    }
    for session in sessions.iter().take(100) {
        top_sessions.insert(session.session_id.clone(), session.clone());
    }

    let final_sessions: Vec<_> = top_sessions.into_values().collect();

    let daily_usage = aggregate_daily_usage(&sessions);
    let by_model = aggregate_by_model(&sessions);
    let weekly_comparison = calculate_weekly_comparison(&sessions);

    Ok(ProjectStatistics {
        project_path: project_path_owned,
        project_name,
        total_sessions: sessions.len(),
        total_usage,
        estimated_cost: total_cost,
        sessions: final_sessions,
        daily_usage: daily_usage.into_iter().rev().take(30).collect(),
        weekly_comparison,
        by_model,
        last_updated: chrono::Utc::now().timestamp_millis(),
    })
}

/// 获取所有项目的聚合统计（带缓存）
#[command]
pub fn get_all_projects_statistics() -> Result<ProjectStatistics, String> {
    StatsCache::global().get_all_projects_stats()
}

/// 获取所有项目的聚合统计（内部实现）
pub fn get_all_projects_statistics_impl() -> Result<ProjectStatistics, String> {
    let claude_dir = get_claude_projects_dir().ok_or("无法获取 Claude 项目目录")?;

    if !claude_dir.exists() {
        return Ok(ProjectStatistics {
            project_path: "all".to_string(),
            project_name: "所有项目".to_string(),
            total_sessions: 0,
            total_usage: UsageData {
                input_tokens: 0,
                output_tokens: 0,
                cache_write_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 0,
            },
            estimated_cost: 0.0,
            sessions: Vec::new(),
            daily_usage: Vec::new(),
            weekly_comparison: WeeklyComparison {
                current_week: WeekData {
                    sessions: 0,
                    cost: 0.0,
                    tokens: 0,
                },
                last_week: WeekData {
                    sessions: 0,
                    cost: 0.0,
                    tokens: 0,
                },
                trends: TrendData {
                    sessions: 0.0,
                    cost: 0.0,
                    tokens: 0.0,
                },
            },
            by_model: Vec::new(),
            last_updated: chrono::Utc::now().timestamp_millis(),
        });
    }

    let mut all_sessions: Vec<SessionSummary> = Vec::new();
    let mut total_usage = UsageData {
        input_tokens: 0,
        output_tokens: 0,
        cache_write_tokens: 0,
        cache_read_tokens: 0,
        total_tokens: 0,
    };
    let mut total_cost = 0.0;
    let mut processed_hashes = HashSet::new();

    let entries =
        fs::read_dir(&claude_dir).map_err(|e| format!("无法读取 Claude 项目目录: {}", e))?;

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(entries) = fs::read_dir(&path) {
                for file_entry in entries.filter_map(|e| e.ok()) {
                    let file_path = file_entry.path();
                    if file_path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                        if let Some(session) = parse_session_file(&file_path, &mut processed_hashes)
                        {
                            total_usage.input_tokens += session.usage.input_tokens;
                            total_usage.output_tokens += session.usage.output_tokens;
                            total_usage.cache_write_tokens += session.usage.cache_write_tokens;
                            total_usage.cache_read_tokens += session.usage.cache_read_tokens;
                            total_usage.total_tokens += session.usage.total_tokens;
                            total_cost += session.cost;
                            all_sessions.push(session);
                        }
                    }
                }
            }
        }
    }

    // 按时间戳排序
    all_sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // 按成本排序
    let mut all_sessions_by_cost = all_sessions.clone();
    all_sessions_by_cost.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap());

    // 合并去重
    let mut top_sessions = std::collections::HashMap::new();
    for session in all_sessions_by_cost.iter().take(100) {
        top_sessions.insert(session.session_id.clone(), session.clone());
    }
    for session in all_sessions.iter().take(100) {
        top_sessions.insert(session.session_id.clone(), session.clone());
    }

    let final_sessions: Vec<_> = top_sessions.into_values().collect();

    let daily_usage = aggregate_daily_usage(&all_sessions);
    let by_model = aggregate_by_model(&all_sessions);
    let weekly_comparison = calculate_weekly_comparison(&all_sessions);

    Ok(ProjectStatistics {
        project_path: "all".to_string(),
        project_name: "所有项目".to_string(),
        total_sessions: all_sessions.len(),
        total_usage,
        estimated_cost: total_cost,
        sessions: final_sessions,
        daily_usage: daily_usage.into_iter().rev().take(30).collect(),
        weekly_comparison,
        by_model,
        last_updated: chrono::Utc::now().timestamp_millis(),
    })
}

/// 初始化统计缓存（应用启动时调用）
#[command]
pub fn init_stats_cache() -> Result<(), String> {
    info!("初始化统计缓存");
    StatsCache::global().warmup();
    StatsCache::global().start_auto_refresh();
    Ok(())
}

/// 检查统计数据是否正在加载
#[command]
pub fn is_stats_loading(project_path: Option<String>) -> bool {
    if let Some(path) = project_path {
        StatsCache::global().is_loading(Some(&path))
    } else {
        StatsCache::global().is_loading(None)
    }
}
