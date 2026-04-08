// Claude CLI Session File Reader
//
// This module handles reading and parsing Claude CLI session files
// stored in ~/.claude/projects/{encoded_project_path}/{session_id}.jsonl

use crate::claude::message_normalizer::{extract_tag_content, normalize_cli_message};
use crate::models::{
    Message, MessageAttachment, MessageContent, MessageRole, TokenUsage, ToolCall,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Claude CLI JSONL session file entry
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "lowercase")]
enum CliSessionEntry {
    /// User message
    User { message: CliUserMessage },
    /// Assistant message
    Assistant { message: CliAssistantMessage },
    /// Progress update
    Progress {},
    /// File history snapshot
    FileHistorySnapshot {},
    /// Other entry types
    #[serde(other)]
    Other,
}

/// User message from CLI
#[derive(Debug, Deserialize, Serialize)]
struct CliUserMessage {
    role: String,
    content: String,
}

/// Assistant message from CLI
#[derive(Debug, Deserialize, Serialize)]
struct CliAssistantMessage {
    id: String,
    r#type: String,
    role: String,
    model: String,
    content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequence: Option<String>,
    usage: Option<Usage>,
}

/// Content block in assistant messages
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContentBlock {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Token usage information
#[derive(Debug, Deserialize, Serialize)]
struct Usage {
    input_tokens: u64,
    output_tokens: u64,
}

/// Session file reader
pub struct SessionFileReader {
    claude_dir: PathBuf,
}

fn normalize_tool_result_content(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => extract_tag_content(s, "tool_use_error")
            .or_else(|| extract_tag_content(s, "stderr"))
            .unwrap_or_else(|| s.clone()),
        serde_json::Value::Array(items) => items
            .iter()
            .map(normalize_tool_result_content)
            .filter(|item| !item.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Object(map) => {
            if let Some(text) = map.get("text") {
                let normalized = normalize_tool_result_content(text);
                if !normalized.trim().is_empty() {
                    return normalized;
                }
            }

            if let Some(content) = map.get("content") {
                let normalized = normalize_tool_result_content(content);
                if !normalized.trim().is_empty() {
                    return normalized;
                }
            }

            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        }
        _ => value.to_string(),
    }
}

fn parse_json_timestamp(value: &serde_json::Value) -> Option<chrono::DateTime<chrono::Utc>> {
    match value {
        serde_json::Value::String(s) => chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .ok()
            .or_else(|| {
                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                    .ok()
                    .map(|dt| dt.and_utc())
            }),
        serde_json::Value::Number(n) => {
            let value = n.as_i64()?;
            if value >= 1_000_000_000_000 {
                chrono::DateTime::from_timestamp_millis(value)
            } else {
                chrono::DateTime::from_timestamp(value, 0)
            }
        }
        _ => None,
    }
}

fn build_active_uuid_lineage(entries: &[serde_json::Value]) -> HashSet<String> {
    let known_uuids: HashSet<String> = entries
        .iter()
        .filter(|entry| {
            !entry
                .get("isSidechain")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        })
        .filter_map(|entry| entry.get("uuid").and_then(|value| value.as_str()))
        .map(|value| value.to_string())
        .collect();

    let mut parent_by_uuid: HashMap<String, Option<String>> = HashMap::new();
    let mut child_count: HashMap<String, usize> = HashMap::new();
    let mut ordered_nodes: Vec<(String, i64, usize)> = Vec::new();
    let mut previous_uuid: Option<String> = None;

    for (index, entry) in entries.iter().enumerate() {
        if entry
            .get("isSidechain")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            continue;
        }

        let Some(uuid) = entry
            .get("uuid")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
        else {
            continue;
        };

        let parent_uuid = entry
            .get("parentUuid")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .or_else(|| {
                entry
                    .get("logicalParentUuid")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
            })
            .filter(|value| known_uuids.contains(value))
            .or_else(|| {
                let is_compact_boundary = entry.get("type").and_then(|value| value.as_str())
                    == Some("system")
                    && entry.get("subtype").and_then(|value| value.as_str())
                        == Some("compact_boundary");

                if is_compact_boundary {
                    previous_uuid.clone()
                } else {
                    None
                }
            });

        parent_by_uuid.insert(uuid.clone(), parent_uuid.clone());
        child_count.entry(uuid.clone()).or_insert(0);

        if let Some(parent_uuid) = parent_uuid {
            *child_count.entry(parent_uuid).or_insert(0) += 1;
        }

        let ordering_ts = entry
            .get("timestamp")
            .and_then(parse_json_timestamp)
            .map(|timestamp| timestamp.timestamp_millis())
            .unwrap_or(i64::MIN);

        ordered_nodes.push((uuid, ordering_ts, index));
        previous_uuid = Some(
            entry
                .get("uuid")
                .and_then(|value| value.as_str())
                .unwrap()
                .to_string(),
        );
    }

    let Some((latest_leaf_uuid, _, _)) = ordered_nodes
        .into_iter()
        .filter(|(uuid, _, _)| child_count.get(uuid).copied().unwrap_or(0) == 0)
        .max_by_key(|(_, ordering_ts, index)| (*ordering_ts, *index))
    else {
        return HashSet::new();
    };

    let mut lineage = HashSet::new();
    let mut current = Some(latest_leaf_uuid);

    while let Some(uuid) = current {
        if !lineage.insert(uuid.clone()) {
            break;
        }
        current = parent_by_uuid.get(&uuid).cloned().flatten();
    }

    lineage
}

/// 解析消息中的文件引用 (@path 格式)
/// 返回附件信息数组
fn parse_file_references(content: &str) -> Vec<crate::models::MessageAttachment> {
    use regex::Regex;

    let mut attachments = Vec::new();

    // 匹配 @path 格式，path 可以包含路径分隔符
    let re = Regex::new(r"@([^\s]+)").unwrap();

    for cap in re.captures_iter(content) {
        let path = &cap[1];
        let name = path
            .split(|c| c == '/' || c == '\\')
            .last()
            .unwrap_or(path)
            .to_string();

        let ext = name
            .split('.')
            .last()
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        // 判断是否为图片
        let is_image = matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico"
        );

        attachments.push(crate::models::MessageAttachment {
            name,
            path: path.to_string(),
            is_image,
            preview: None,
        });
    }

    attachments
}

fn image_extension_from_media_type(media_type: &str) -> &str {
    match media_type {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/bmp" => "bmp",
        "image/x-icon" => "ico",
        _ => "png",
    }
}

fn parse_structured_user_content(
    content_array: &[serde_json::Value],
) -> (String, Vec<serde_json::Value>, Vec<MessageAttachment>) {
    let mut text_parts = Vec::new();
    let mut content_blocks_json = Vec::new();
    let mut attachments = Vec::new();

    for block in content_array {
        content_blocks_json.push(block.clone());

        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    text_parts.push(text.to_string());
                }
            }
            Some("image") => {
                let media_type = block
                    .get("source")
                    .and_then(|s| s.get("media_type"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("image/png");
                let data = block
                    .get("source")
                    .and_then(|s| s.get("data"))
                    .and_then(|d| d.as_str())
                    .unwrap_or_default();
                let ext = image_extension_from_media_type(media_type);
                attachments.push(MessageAttachment {
                    name: format!("image.{}", ext),
                    path: format!("pasted-image.{}", ext),
                    is_image: true,
                    preview: Some(format!("data:{};base64,{}", media_type, data)),
                });
            }
            _ => {}
        }
    }

    let text = text_parts.join("\n");
    let mut file_refs = parse_file_references(&text);
    attachments.append(&mut file_refs);

    (text, content_blocks_json, attachments)
}

impl SessionFileReader {
    /// Create a new session file reader
    pub fn new() -> Result<Self, String> {
        // Get home directory
        let home = dirs::home_dir().ok_or("Could not find home directory")?;

        let claude_dir = home.join(".claude");

        if !claude_dir.exists() {
            return Err(format!(
                "Claude directory not found: {}",
                claude_dir.display()
            ));
        }

        debug!(
            "SessionFileReader initialized with Claude dir: {}",
            claude_dir.display()
        );

        Ok(Self { claude_dir })
    }

    /// Encode a path to match Claude CLI's encoding format
    /// Replaces "/" with "_" and then "_" with "-"
    fn encode_path(path: &str) -> String {
        path.trim().replace('/', "_").replace('_', "-")
    }

    /// Get the path to a session file
    pub fn get_session_path(&self, project_path: &str, session_id: &str) -> PathBuf {
        let encoded = Self::encode_path(project_path);
        self.claude_dir
            .join("projects")
            .join(encoded)
            .join(format!("{}.jsonl", session_id))
    }

    /// Load messages from a session file
    pub fn load_messages(
        &self,
        project_path: &str,
        session_id: &str,
    ) -> Result<Vec<Message>, String> {
        let session_path = self.get_session_path(project_path, session_id);

        if !session_path.exists() {
            return Err(format!(
                "Session file not found: {}",
                session_path.display()
            ));
        }

        debug!("Loading messages from: {}", session_path.display());

        let file =
            File::open(&session_path).map_err(|e| format!("Failed to open session file: {}", e))?;

        let reader = BufReader::new(file);
        let mut entries: Vec<serde_json::Value> = Vec::new();

        for line in reader.lines() {
            let line = line.map_err(|e| format!("Failed to read line: {}", e))?;

            if line.trim().is_empty() {
                continue;
            }

            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&line) {
                entries.push(json_value);
            }
        }

        let active_uuid_lineage = build_active_uuid_lineage(&entries);
        let mut messages: Vec<Message> = Vec::new();
        // 记录 tool_use_id 到 assistant 消息索引的映射，用于正确关联 tool_result
        let mut tool_use_to_assistant: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        // 记录原始 assistant uuid 到消息索引的映射，用于 tool_result 兜底关联
        let mut assistant_uuid_to_index: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for json_value in entries {
            if let Some(uuid) = json_value.get("uuid").and_then(|value| value.as_str()) {
                if !active_uuid_lineage.is_empty() && !active_uuid_lineage.contains(uuid) {
                    continue;
                }
            }

            let entry_type = json_value.get("type").and_then(|t| t.as_str());
            let entry_created_at = json_value.get("timestamp").and_then(parse_json_timestamp);

            match entry_type {
                Some("user") => {
                    if json_value
                        .get("isCompactSummary")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false)
                    {
                        continue;
                    }

                    // 检查 isMeta 字段，过滤掉元数据消息
                    if json_value
                        .get("isMeta")
                        .and_then(|m| m.as_bool())
                        .unwrap_or(false)
                    {
                        debug!("Filtering out isMeta message");
                        continue;
                    }

                    // 检查 message.content 是否包含 tool_result
                    if let Some(message) = json_value.get("message") {
                        if let Some(content) = message.get("content") {
                            // content 可能是数组（包含 tool_result）
                            if let Some(content_array) = content.as_array() {
                                let has_tool_result = content_array.iter().any(|item| {
                                    item.get("type").and_then(|t| t.as_str()) == Some("tool_result")
                                });

                                if has_tool_result {
                                    // 处理 tool_result
                                    for item in content_array {
                                        if let Some(item_type) =
                                            item.get("type").and_then(|t| t.as_str())
                                        {
                                            if item_type == "tool_result" {
                                                let tool_use_id = item
                                                    .get("tool_use_id")
                                                    .or_else(|| item.get("toolUseId"))
                                                    .and_then(|id| id.as_str())
                                                    .map(|s| s.to_string());

                                                let result_content = item
                                                    .get("content")
                                                    .map(normalize_tool_result_content)
                                                    .filter(|content| !content.trim().is_empty())
                                                    .or_else(|| {
                                                        json_value
                                                            .get("toolUseResult")
                                                            .map(normalize_tool_result_content)
                                                    })
                                                    .unwrap_or_default();

                                                let is_error = item
                                                    .get("is_error")
                                                    .or_else(|| item.get("isError"))
                                                    .and_then(|e| e.as_bool())
                                                    .unwrap_or(false);

                                                // 关联到正确的 assistant 消息（使用 tool_use_id 映射）
                                                if let Some(tool_use_id) = tool_use_id {
                                                    let target_index = tool_use_to_assistant
                                                        .get(&tool_use_id)
                                                        .copied()
                                                        .or_else(|| {
                                                            json_value
                                                                .get("sourceToolAssistantUUID")
                                                                .and_then(|uuid| uuid.as_str())
                                                                .and_then(|uuid| {
                                                                    assistant_uuid_to_index
                                                                        .get(uuid)
                                                                        .copied()
                                                                })
                                                        });

                                                    if let Some(idx) = target_index {
                                                        if let Some(msg) = messages.get_mut(idx) {
                                                            if msg.tool_results.is_none() {
                                                                msg.tool_results =
                                                                    Some(HashMap::new());
                                                                msg.tool_result_errors =
                                                                    Some(HashMap::new());
                                                            }
                                                            if let Some(ref mut results) =
                                                                msg.tool_results
                                                            {
                                                                results.insert(
                                                                    tool_use_id.clone(),
                                                                    result_content,
                                                                );
                                                            }
                                                            if let Some(ref mut errors) =
                                                                msg.tool_result_errors
                                                            {
                                                                errors
                                                                    .insert(tool_use_id, is_error);
                                                            }
                                                        }
                                                    } else {
                                                        warn!(
                                                            tool_use_id = %tool_use_id,
                                                            source_tool_assistant_uuid = ?json_value.get("sourceToolAssistantUUID").and_then(|uuid| uuid.as_str()),
                                                            "Failed to associate tool_result with assistant message while loading session history"
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    continue; // 跳过这个 entry，不创建用户消息
                                }
                            }
                        }

                        let msg = if let Some(content_array) =
                            message.get("content").and_then(|c| c.as_array())
                        {
                            let (user_content, content_blocks_json, attachments) =
                                parse_structured_user_content(content_array);

                            let (normalized_user_content, role) =
                                match normalize_cli_message(&user_content) {
                                    Some(normalized) => normalized.role_or(MessageRole::User),
                                    None if attachments.is_empty() => {
                                        debug!("Filtering out normalized message");
                                        continue;
                                    }
                                    None => (String::new(), MessageRole::User),
                                };

                            let mut msg = Message::new(
                                session_id.to_string(),
                                role,
                                MessageContent::Text(normalized_user_content),
                            );
                            msg.checkpoint_uuid = json_value
                                .get("uuid")
                                .and_then(|value| value.as_str())
                                .map(|value| value.to_string());
                            if let Some(created_at) = entry_created_at {
                                msg.created_at = created_at;
                            }
                            msg.content_blocks = Some(content_blocks_json);
                            if !attachments.is_empty() {
                                msg.attachments = Some(attachments);
                            }
                            msg
                        } else {
                            let user_content_raw = message
                                .get("content")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string();

                            let (user_content, role) =
                                match normalize_cli_message(&user_content_raw) {
                                    Some(normalized) => normalized.role_or(MessageRole::User),
                                    None => {
                                        debug!("Filtering out normalized message");
                                        continue;
                                    }
                                };

                            let attachments = parse_file_references(&user_content);

                            let mut msg = Message::new(
                                session_id.to_string(),
                                role,
                                MessageContent::Text(user_content),
                            );
                            msg.checkpoint_uuid = json_value
                                .get("uuid")
                                .and_then(|value| value.as_str())
                                .map(|value| value.to_string());
                            if let Some(created_at) = entry_created_at {
                                msg.created_at = created_at;
                            }

                            if !attachments.is_empty() {
                                msg.attachments = Some(attachments);
                            }
                            msg
                        };

                        messages.push(msg);
                    }
                }
                Some("assistant") => {
                    // 处理 assistant 消息
                    if let Some(message) = json_value.get("message") {
                        if let Some(content) = message.get("content") {
                            if let Some(content_array) = content.as_array() {
                                // 保留原始 content_blocks 作为 JSON
                                let content_blocks_json: Vec<serde_json::Value> =
                                    content_array.iter().cloned().collect();

                                // 提取文本内容
                                let text_parts: Vec<&str> = content_array
                                    .iter()
                                    .filter_map(|block| {
                                        if block.get("type").and_then(|t| t.as_str())
                                            == Some("text")
                                        {
                                            block.get("text").and_then(|t| t.as_str())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                let text_raw = text_parts.join("\n");

                                // 规范化内容：过滤或转换系统标签
                                let (text, role) = match normalize_cli_message(&text_raw) {
                                    Some(normalized) => normalized.role_or(MessageRole::Assistant),
                                    None => {
                                        debug!("Filtering out normalized assistant message");
                                        continue;
                                    }
                                };

                                // 提取 tool_use 块到 tool_calls
                                let mut tool_calls = Vec::new();
                                let mut tool_results: HashMap<String, String> = HashMap::new();
                                let mut tool_result_errors: HashMap<String, bool> = HashMap::new();

                                for block in content_array {
                                    if block.get("type").and_then(|t| t.as_str())
                                        == Some("tool_use")
                                    {
                                        let id = block.get("id").and_then(|i| i.as_str());
                                        let name = block.get("name").and_then(|n| n.as_str());
                                        let input = block.get("input");

                                        if let (Some(id), Some(name), Some(input)) =
                                            (id, name, input)
                                        {
                                            tool_calls.push(ToolCall {
                                                id: id.to_string(),
                                                name: name.to_string(),
                                                input: input.clone(),
                                            });
                                            // 记录 tool_use_id 到当前消息索引的映射（在添加到 messages 之前）
                                            // 注意：这里使用 messages.len() 作为未来消息的索引
                                            let future_index = messages.len();
                                            tool_use_to_assistant
                                                .insert(id.to_string(), future_index);
                                        }
                                    } else if block.get("type").and_then(|t| t.as_str())
                                        == Some("tool_result")
                                    {
                                        // 同时处理内联的 tool_result 块
                                        let tool_use_id = block
                                            .get("tool_use_id")
                                            .or_else(|| block.get("toolUseId"))
                                            .and_then(|id| id.as_str())
                                            .map(|s| s.to_string());

                                        let result_content = block
                                            .get("content")
                                            .map(normalize_tool_result_content)
                                            .unwrap_or_default();

                                        let is_error = block
                                            .get("is_error")
                                            .or_else(|| block.get("isError"))
                                            .and_then(|e| e.as_bool())
                                            .unwrap_or(false);

                                        if let Some(tool_use_id) = tool_use_id {
                                            tool_results
                                                .insert(tool_use_id.clone(), result_content);
                                            tool_result_errors.insert(tool_use_id, is_error);
                                        }
                                    }
                                }

                                let mut msg = Message::new(
                                    session_id.to_string(),
                                    role,
                                    MessageContent::Text(text),
                                );
                                if let Some(created_at) = entry_created_at {
                                    msg.created_at = created_at;
                                }

                                msg.model = message
                                    .get("model")
                                    .and_then(|m| m.as_str())
                                    .map(|m| m.to_string());

                                msg.usage = message.get("usage").and_then(|usage| {
                                    serde_json::from_value::<TokenUsage>(usage.clone()).ok()
                                });

                                msg.tool_calls = tool_calls;
                                msg.content_blocks = Some(content_blocks_json);
                                // 只有当有实际结果时才设置
                                if !tool_results.is_empty() {
                                    msg.tool_results = Some(tool_results);
                                    msg.tool_result_errors = Some(tool_result_errors);
                                }
                                messages.push(msg);
                                if let Some(uuid) = json_value.get("uuid").and_then(|v| v.as_str())
                                {
                                    assistant_uuid_to_index
                                        .insert(uuid.to_string(), messages.len() - 1);
                                }
                            }
                        }
                    }
                }
                Some("system") => {
                    let subtype = json_value.get("subtype").and_then(|value| value.as_str());
                    let content = json_value
                        .get("content")
                        .and_then(|value| value.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();

                    if subtype == Some("compact_boundary") && !content.is_empty() {
                        let mut msg = Message::new(
                            session_id.to_string(),
                            MessageRole::System,
                            MessageContent::Text(content),
                        );
                        if let Some(created_at) = entry_created_at {
                            msg.created_at = created_at;
                        }
                        msg.checkpoint_uuid = json_value
                            .get("uuid")
                            .and_then(|value| value.as_str())
                            .map(|value| value.to_string());
                        messages.push(msg);
                    }
                }
                _ => {
                    // 其他类型，忽略
                    debug!("Ignoring session entry with type: {:?}", entry_type);
                }
            }
        }

        Ok(messages)
    }

    /// Extract text content from assistant message blocks
    fn extract_assistant_text(blocks: &[ContentBlock]) -> String {
        let mut parts = Vec::new();

        for block in blocks {
            match block.r#type.as_str() {
                "text" => {
                    if let Some(text) = &block.text {
                        parts.push(text.clone());
                    }
                }
                "thinking" => {
                    // Skip thinking blocks for display
                }
                "tool_use" => {
                    // Format tool use
                    if let Some(name) = &block.name {
                        parts.push(format!("🔧 Using tool: {}", name));
                    }
                }
                "tool_result" => {
                    // Format tool result
                    if let Some(is_error) = block.is_error {
                        if is_error {
                            parts.push("❌ Tool error".to_string());
                        }
                    }
                }
                _ => {
                    // Ignore unknown block types
                }
            }
        }

        parts.join("\n")
    }

    /// Check if a session file exists
    pub fn session_exists(&self, project_path: &str, session_id: &str) -> bool {
        let session_path = self.get_session_path(project_path, session_id);
        session_path.exists()
    }

    /// List all session IDs for a project
    pub fn list_sessions(&self, project_path: &str) -> Result<Vec<String>, String> {
        let encoded = Self::encode_path(project_path);
        let project_dir = self.claude_dir.join("projects").join(&encoded);

        if !project_dir.exists() {
            info!("Project directory does not exist");
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();

        let entries = std::fs::read_dir(&project_dir)
            .map_err(|e| format!("Failed to read project directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();

            // Check if it's a .jsonl file
            if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    sessions.push(name.to_string());
                }
            }
        }

        sessions.sort();
        Ok(sessions)
    }
}

impl Default for SessionFileReader {
    fn default() -> Self {
        Self::new().expect("Failed to create SessionFileReader")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_encode_path() {
        assert_eq!(SessionFileReader::encode_path("/Users/test"), "Users-test");
        assert_eq!(SessionFileReader::encode_path("Users/test"), "Users-test");
    }

    #[test]
    fn test_load_messages_associates_user_tool_result_with_prior_assistant_tool_use() {
        let temp_root = std::env::temp_dir().join(format!(
            "claude-desk-session-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let claude_dir = temp_root.join(".claude");
        let project_path = "/tmp/demo-project";
        let session_id = "session-123";
        let session_dir = claude_dir
            .join("projects")
            .join(SessionFileReader::encode_path(project_path));
        fs::create_dir_all(&session_dir).unwrap();

        let session_file = session_dir.join(format!("{}.jsonl", session_id));
        let payload = concat!(
            r#"{"type":"assistant","uuid":"assistant-1","message":{"id":"msg_1","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"tool_use","id":"call_1","name":"Skill","input":{"name":"zeus:get-workflow"}}]}}
"#,
            r#"{"type":"user","sourceToolAssistantUUID":"assistant-1","toolUseResult":"InputValidationError: expected string","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_1","is_error":true,"content":"<tool_use_error>InputValidationError: Skill failed due to the following issue:
The required parameter `skill` is missing</tool_use_error>"}]}}
"#
        );
        fs::write(&session_file, payload).unwrap();

        let reader = SessionFileReader { claude_dir };
        let messages = reader.load_messages(project_path, session_id).unwrap();

        assert_eq!(messages.len(), 1);
        let msg = &messages[0];
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].id, "call_1");
        assert_eq!(
            msg.tool_results.as_ref().unwrap().get("call_1").unwrap(),
            "InputValidationError: Skill failed due to the following issue:
The required parameter `skill` is missing"
        );
        assert_eq!(
            msg.tool_result_errors.as_ref().unwrap().get("call_1"),
            Some(&true)
        );

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn test_load_messages_restores_timestamp_from_jsonl_entry() {
        let temp_root = std::env::temp_dir().join(format!(
            "claude-desk-session-timestamp-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let claude_dir = temp_root.join(".claude");
        let project_path = "/tmp/demo-project";
        let session_id = "session-456";
        let session_dir = claude_dir
            .join("projects")
            .join(SessionFileReader::encode_path(project_path));
        fs::create_dir_all(&session_dir).unwrap();

        let session_file = session_dir.join(format!("{}.jsonl", session_id));
        let payload = concat!(
            r#"{"type":"user","timestamp":"2026-03-09T15:52:19.562Z","message":{"role":"user","content":"你好"}}"#,
            "\n"
        );
        fs::write(&session_file, payload).unwrap();

        let reader = SessionFileReader { claude_dir };
        let messages = reader.load_messages(project_path, session_id).unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0]
                .created_at
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "2026-03-09T15:52:19.562Z"
        );

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn test_load_messages_restores_usage_from_snake_case_jsonl() {
        let temp_root = std::env::temp_dir().join(format!(
            "claude-desk-session-usage-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let claude_dir = temp_root.join(".claude");
        let project_path = "/tmp/demo-project";
        let session_id = "session-usage";
        let session_dir = claude_dir
            .join("projects")
            .join(SessionFileReader::encode_path(project_path));
        fs::create_dir_all(&session_dir).unwrap();

        let session_file = session_dir.join(format!("{}.jsonl", session_id));
        let payload = concat!(
            r#"{"type":"assistant","timestamp":"2026-03-09T15:52:19.562Z","message":{"id":"msg_usage","type":"message","role":"assistant","model":"claude-3-5-sonnet","usage":{"input_tokens":37800,"output_tokens":218,"cache_creation_input_tokens":0,"cache_read_input_tokens":41800},"content":[{"type":"text","text":"hello"}]}}"#,
            "\n"
        );
        fs::write(&session_file, payload).unwrap();

        let reader = SessionFileReader { claude_dir };
        let messages = reader.load_messages(project_path, session_id).unwrap();

        assert_eq!(messages.len(), 1);
        let usage = messages[0]
            .usage
            .as_ref()
            .expect("usage should be restored");
        assert_eq!(usage.input_tokens, 37800);
        assert_eq!(usage.output_tokens, 218);
        assert_eq!(usage.cache_creation_input_tokens, Some(0));
        assert_eq!(usage.cache_read_input_tokens, Some(41800));

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn test_load_messages_only_keeps_active_branch_after_rewind() {
        let temp_root = std::env::temp_dir().join(format!(
            "claude-desk-session-rewind-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let claude_dir = temp_root.join(".claude");
        let project_path = "/tmp/demo-project";
        let session_id = "session-rewind";
        let session_dir = claude_dir
            .join("projects")
            .join(SessionFileReader::encode_path(project_path));
        fs::create_dir_all(&session_dir).unwrap();

        let session_file = session_dir.join(format!("{}.jsonl", session_id));
        let payload = concat!(
            r#"{"type":"user","uuid":"user-1","parentUuid":null,"timestamp":"2026-03-25T15:00:35.952Z","message":{"role":"user","content":"创建一个a1.md文件"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-1","parentUuid":"user-1","timestamp":"2026-03-25T15:00:57.058Z","message":{"id":"msg_1","type":"message","role":"assistant","model":"glm-5","content":[{"type":"text","text":"已创建 `a1.md` 文件。"}]}}"#,
            "\n",
            r#"{"type":"user","uuid":"user-2-old","parentUuid":"assistant-1","timestamp":"2026-03-25T16:35:48.738Z","message":{"role":"user","content":"创建一个文件a2.md"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-2-old","parentUuid":"user-2-old","timestamp":"2026-03-25T16:38:25.715Z","message":{"id":"msg_2","type":"message","role":"assistant","model":"glm-5","content":[{"type":"text","text":"旧分支的 a2" }]}}"#,
            "\n",
            r#"{"type":"user","uuid":"user-2-new","parentUuid":"assistant-1","timestamp":"2026-03-25T16:42:51.917Z","message":{"role":"user","content":"创建一个文件a2.md"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-2-new","parentUuid":"user-2-new","timestamp":"2026-03-25T16:44:16.258Z","message":{"id":"msg_3","type":"message","role":"assistant","model":"glm-5","content":[{"type":"text","text":"新分支的 a2"}]}}"#,
            "\n"
        );
        fs::write(&session_file, payload).unwrap();

        let reader = SessionFileReader { claude_dir };
        let messages = reader.load_messages(project_path, session_id).unwrap();

        assert_eq!(messages.len(), 4);

        match &messages[0].content {
            MessageContent::Text(content) => assert_eq!(content, "创建一个a1.md文件"),
            other => panic!("expected text content, got {:?}", other),
        }
        match &messages[1].content {
            MessageContent::Text(content) => assert_eq!(content, "已创建 `a1.md` 文件。"),
            other => panic!("expected text content, got {:?}", other),
        }
        match &messages[2].content {
            MessageContent::Text(content) => assert_eq!(content, "创建一个文件a2.md"),
            other => panic!("expected text content, got {:?}", other),
        }
        match &messages[3].content {
            MessageContent::Text(content) => assert_eq!(content, "新分支的 a2"),
            other => panic!("expected text content, got {:?}", other),
        }

        assert_eq!(messages[2].checkpoint_uuid.as_deref(), Some("user-2-new"));

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn test_load_messages_keeps_pre_compact_history_and_hides_compact_summary() {
        let temp_root = std::env::temp_dir().join(format!(
            "claude-desk-session-compact-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let claude_dir = temp_root.join(".claude");
        let project_path = "/tmp/demo-project";
        let session_id = "session-compact";
        let session_dir = claude_dir
            .join("projects")
            .join(SessionFileReader::encode_path(project_path));
        fs::create_dir_all(&session_dir).unwrap();

        let session_file = session_dir.join(format!("{}.jsonl", session_id));
        let payload = concat!(
            r#"{"type":"user","uuid":"user-1","parentUuid":null,"timestamp":"2026-03-23T10:46:00.000Z","message":{"role":"user","content":"旧历史用户消息"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-1","parentUuid":"user-1","timestamp":"2026-03-23T10:46:10.000Z","message":{"id":"msg_1","type":"message","role":"assistant","model":"glm-5","content":[{"type":"text","text":"旧历史助手消息"}]}}"#,
            "\n",
            r#"{"type":"system","uuid":"compact-1","parentUuid":null,"logicalParentUuid":"assistant-1","subtype":"compact_boundary","content":"Conversation compacted","timestamp":"2026-03-23T10:48:14.531Z"}"#,
            "\n",
            r#"{"type":"user","uuid":"summary-1","parentUuid":"compact-1","isCompactSummary":true,"timestamp":"2026-03-23T10:48:14.532Z","message":{"role":"user","content":"This session is being continued from a previous conversation that ran out of context."}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-2","parentUuid":"summary-1","timestamp":"2026-03-23T10:48:20.000Z","message":{"id":"msg_2","type":"message","role":"assistant","model":"glm-5","content":[{"type":"text","text":"压缩后的助手消息"}]}}"#,
            "\n"
        );
        fs::write(&session_file, payload).unwrap();

        let reader = SessionFileReader { claude_dir };
        let messages = reader.load_messages(project_path, session_id).unwrap();

        assert_eq!(messages.len(), 4);

        match &messages[0].content {
            MessageContent::Text(content) => assert_eq!(content, "旧历史用户消息"),
            other => panic!("expected text content, got {:?}", other),
        }
        assert_eq!(messages[1].role, MessageRole::Assistant);
        match &messages[1].content {
            MessageContent::Text(content) => assert_eq!(content, "旧历史助手消息"),
            other => panic!("expected text content, got {:?}", other),
        }
        assert_eq!(messages[2].role, MessageRole::System);
        match &messages[2].content {
            MessageContent::Text(content) => assert_eq!(content, "Conversation compacted"),
            other => panic!("expected text content, got {:?}", other),
        }
        match &messages[3].content {
            MessageContent::Text(content) => assert_eq!(content, "压缩后的助手消息"),
            other => panic!("expected text content, got {:?}", other),
        }

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn test_load_messages_falls_back_when_compact_logical_parent_is_missing() {
        let temp_root = std::env::temp_dir().join(format!(
            "claude-desk-session-compact-missing-parent-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let claude_dir = temp_root.join(".claude");
        let project_path = "/tmp/demo-project";
        let session_id = "session-compact-missing-parent";
        let session_dir = claude_dir
            .join("projects")
            .join(SessionFileReader::encode_path(project_path));
        fs::create_dir_all(&session_dir).unwrap();

        let session_file = session_dir.join(format!("{}.jsonl", session_id));
        let payload = concat!(
            r#"{"type":"user","uuid":"user-1","parentUuid":null,"timestamp":"2026-03-23T10:46:00.000Z","message":{"role":"user","content":"旧历史用户消息"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-1","parentUuid":"user-1","timestamp":"2026-03-23T10:46:10.000Z","message":{"id":"msg_1","type":"message","role":"assistant","model":"glm-5","content":[{"type":"text","text":"旧历史助手消息"}]}}"#,
            "\n",
            r#"{"type":"progress","uuid":"progress-1","parentUuid":"assistant-1","timestamp":"2026-03-23T10:46:11.000Z","data":{"type":"hook_progress"}}"#,
            "\n",
            r#"{"type":"system","uuid":"compact-1","parentUuid":null,"logicalParentUuid":"missing-parent","subtype":"compact_boundary","content":"Conversation compacted","timestamp":"2026-03-23T10:48:14.531Z"}"#,
            "\n",
            r#"{"type":"user","uuid":"summary-1","parentUuid":"compact-1","isCompactSummary":true,"timestamp":"2026-03-23T10:48:14.532Z","message":{"role":"user","content":"This session is being continued from a previous conversation that ran out of context."}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-2","parentUuid":"summary-1","timestamp":"2026-03-23T10:48:20.000Z","message":{"id":"msg_2","type":"message","role":"assistant","model":"glm-5","content":[{"type":"text","text":"压缩后的助手消息"}]}}"#,
            "\n"
        );
        fs::write(&session_file, payload).unwrap();

        let reader = SessionFileReader { claude_dir };
        let messages = reader.load_messages(project_path, session_id).unwrap();

        assert_eq!(messages.len(), 4);
        match &messages[0].content {
            MessageContent::Text(content) => assert_eq!(content, "旧历史用户消息"),
            other => panic!("expected text content, got {:?}", other),
        }
        match &messages[1].content {
            MessageContent::Text(content) => assert_eq!(content, "旧历史助手消息"),
            other => panic!("expected text content, got {:?}", other),
        }
        assert_eq!(messages[2].role, MessageRole::System);
        match &messages[2].content {
            MessageContent::Text(content) => assert_eq!(content, "Conversation compacted"),
            other => panic!("expected text content, got {:?}", other),
        }
        match &messages[3].content {
            MessageContent::Text(content) => assert_eq!(content, "压缩后的助手消息"),
            other => panic!("expected text content, got {:?}", other),
        }

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn test_load_messages_converts_task_notification_to_system_message() {
        let temp_root = std::env::temp_dir().join(format!(
            "claude-desk-session-task-notification-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let claude_dir = temp_root.join(".claude");
        let project_path = "/tmp/demo-project";
        let session_id = "session-task-notification";
        let session_dir = claude_dir
            .join("projects")
            .join(SessionFileReader::encode_path(project_path));
        fs::create_dir_all(&session_dir).unwrap();

        let session_file = session_dir.join(format!("{}.jsonl", session_id));
        let payload = concat!(
            r#"{"type":"user","timestamp":"2026-03-18T07:31:19.467Z","message":{"role":"user","content":"<task-notification>\n<task-id>bhss8seor</task-id>\n<tool-use-id>call_91d49aed625c407193d793a7</tool-use-id>\n<output-file>/tmp/bhss8seor.output</output-file>\n<status>completed</status>\n<summary>Background command \"查找 MqProduceCallback 类\" completed (exit code 0)</summary>\n</task-notification>\nRead the output file to retrieve the result: /tmp/bhss8seor.output"}}"#,
            "\n"
        );
        fs::write(&session_file, payload).unwrap();

        let reader = SessionFileReader { claude_dir };
        let messages = reader.load_messages(project_path, session_id).unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::System);
        match &messages[0].content {
            MessageContent::Text(content) => {
                assert_eq!(
                    content,
                    "Background command \"查找 MqProduceCallback 类\" completed (exit code 0)"
                );
            }
            other => panic!("expected text content, got {:?}", other),
        }

        let _ = fs::remove_dir_all(temp_root);
    }
}
