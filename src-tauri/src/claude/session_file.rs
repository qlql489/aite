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
use std::path::{Path, PathBuf};
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
    let mut children_by_uuid: HashMap<String, Vec<String>> = HashMap::new();
    let mut entry_by_uuid: HashMap<String, &serde_json::Value> = HashMap::new();
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

        entry_by_uuid.insert(uuid.clone(), entry);
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
            *child_count.entry(parent_uuid.clone()).or_insert(0) += 1;
            children_by_uuid
                .entry(parent_uuid)
                .or_default()
                .push(uuid.clone());
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

    let preserved_parallel_roots: Vec<String> = parent_by_uuid
        .iter()
        .filter_map(|(uuid, parent_uuid)| {
            let parent_uuid = parent_uuid.as_ref()?;
            if !lineage.contains(parent_uuid) || lineage.contains(uuid) {
                return None;
            }

            let entry = entry_by_uuid.get(uuid)?;
            if entry_contains_subagent_tool(entry) {
                Some(uuid.clone())
            } else {
                None
            }
        })
        .collect();

    for root_uuid in preserved_parallel_roots {
        let mut stack = vec![root_uuid];
        while let Some(uuid) = stack.pop() {
            if !lineage.insert(uuid.clone()) {
                continue;
            }

            if let Some(children) = children_by_uuid.get(&uuid) {
                stack.extend(children.iter().cloned());
            }
        }
    }

    lineage
}

fn entry_contains_subagent_tool(entry: &serde_json::Value) -> bool {
    if entry.get("type").and_then(|value| value.as_str()) != Some("assistant") {
        return false;
    }

    let Some(content) = entry
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_array())
    else {
        return false;
    };

    content.iter().any(|block| {
        parse_tool_use_block(block)
            .map(|tool_call| tool_call.name == "Task" || tool_call.name == "Agent")
            .unwrap_or(false)
    })
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

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubagentMeta {
    description: Option<String>,
    agent_type: Option<String>,
}

#[derive(Debug, Clone)]
struct ParentToolCandidate {
    tool_use_id: String,
    description: String,
    agent_type: String,
    timestamp: Option<chrono::DateTime<chrono::Utc>>,
    assigned: bool,
}

fn read_jsonl_entries(path: &Path) -> Result<Vec<serde_json::Value>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open session file: {}", e))?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read line: {}", e))?;

        if line.trim().is_empty() {
            continue;
        }

        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&line) {
            entries.push(json_value);
        }
    }

    Ok(entries)
}

fn parse_tool_use_block(block: &serde_json::Value) -> Option<ToolCall> {
    if block.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
        return None;
    }

    let content_payload = match block.get("content") {
        Some(serde_json::Value::String(text)) => serde_json::from_str::<serde_json::Value>(text).ok(),
        Some(serde_json::Value::Object(map)) => Some(serde_json::Value::Object(map.clone())),
        _ => None,
    };

    let id = block
        .get("id")
        .and_then(|value| value.as_str())
        .or_else(|| {
            content_payload
                .as_ref()
                .and_then(|value| value.get("id"))
                .and_then(|value| value.as_str())
        })?;

    let name = block
        .get("name")
        .and_then(|value| value.as_str())
        .or_else(|| {
            content_payload
                .as_ref()
                .and_then(|value| value.get("name"))
                .and_then(|value| value.as_str())
        })?;

    let input = block.get("input").cloned().or_else(|| {
        content_payload
            .as_ref()
            .and_then(|value| value.get("input"))
            .cloned()
    })?;

    Some(ToolCall {
        id: id.to_string(),
        name: name.to_string(),
        input,
    })
}

fn first_entry_timestamp(entries: &[serde_json::Value]) -> Option<chrono::DateTime<chrono::Utc>> {
    entries
        .iter()
        .filter_map(|entry| entry.get("timestamp").and_then(parse_json_timestamp))
        .min()
}

fn collect_parent_tool_candidates(messages: &[Message]) -> Vec<ParentToolCandidate> {
    let mut candidates = Vec::new();

    for msg in messages {
        let Some(content_blocks) = msg.content_blocks.as_ref() else {
            continue;
        };

        for block in content_blocks {
            let Some(tool_call) = parse_tool_use_block(block) else {
                continue;
            };

            if tool_call.name != "Task" && tool_call.name != "Agent" {
                continue;
            }

            let description = tool_call
                .input
                .get("description")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();

            let agent_type = tool_call
                .input
                .get("subagent_type")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();

            candidates.push(ParentToolCandidate {
                tool_use_id: tool_call.id,
                description,
                agent_type,
                timestamp: Some(msg.created_at),
                assigned: false,
            });
        }
    }

    candidates
}

fn pick_candidate_index(
    candidates: &[ParentToolCandidate],
    predicate: impl Fn(&ParentToolCandidate) -> bool,
    timestamp: Option<chrono::DateTime<chrono::Utc>>,
) -> Option<usize> {
    let mut matches: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| !candidate.assigned && predicate(candidate))
        .map(|(index, _)| index)
        .collect();

    if matches.is_empty() {
        return None;
    }

    if matches.len() == 1 || timestamp.is_none() {
        return matches.into_iter().next();
    }

    let ts = timestamp.unwrap();
    matches.sort_by_key(|index| {
        candidates[*index]
            .timestamp
            .map(|candidate_ts| (candidate_ts.timestamp_millis() - ts.timestamp_millis()).abs())
            .unwrap_or(i64::MAX)
    });

    matches.into_iter().next()
}

fn match_subagent_parent_tool(
    candidates: &mut [ParentToolCandidate],
    meta: &SubagentMeta,
    timestamp: Option<chrono::DateTime<chrono::Utc>>,
) -> Option<String> {
    let description = meta.description.as_deref().unwrap_or("").trim();
    let agent_type = meta.agent_type.as_deref().unwrap_or("").trim();

    let exact_match = if !description.is_empty() && !agent_type.is_empty() {
        pick_candidate_index(
            candidates,
            |candidate| candidate.description == description && candidate.agent_type == agent_type,
            timestamp,
        )
    } else {
        None
    };

    let desc_only_match = if exact_match.is_none() && !description.is_empty() {
        pick_candidate_index(
            candidates,
            |candidate| candidate.description == description,
            timestamp,
        )
    } else {
        None
    };

    let type_only_match = if exact_match.is_none() && desc_only_match.is_none() && !agent_type.is_empty()
    {
        pick_candidate_index(
            candidates,
            |candidate| candidate.agent_type == agent_type,
            timestamp,
        )
    } else {
        None
    };

    let fallback_match = if exact_match.is_none() && desc_only_match.is_none() && type_only_match.is_none() {
        pick_candidate_index(candidates, |_| true, timestamp)
    } else {
        None
    };

    let index = exact_match
        .or(desc_only_match)
        .or(type_only_match)
        .or(fallback_match)?;

    candidates[index].assigned = true;
    Some(candidates[index].tool_use_id.clone())
}

fn parse_entries_into_messages(
    entries: Vec<serde_json::Value>,
    session_id: &str,
    forced_parent_tool_use_id: Option<&str>,
) -> Vec<Message> {
    let active_uuid_lineage = build_active_uuid_lineage(&entries);
    let mut messages: Vec<Message> = Vec::new();
    let mut tool_use_to_assistant: HashMap<String, usize> = HashMap::new();
    let mut assistant_uuid_to_index: HashMap<String, usize> = HashMap::new();
    let mut pending_tool_results: Vec<(String, String, bool, Option<String>)> = Vec::new();

    for json_value in entries {
        if let Some(uuid) = json_value.get("uuid").and_then(|value| value.as_str()) {
            if !active_uuid_lineage.is_empty() && !active_uuid_lineage.contains(uuid) {
                continue;
            }
        }

        let entry_type = json_value.get("type").and_then(|t| t.as_str());
        let entry_created_at = json_value.get("timestamp").and_then(parse_json_timestamp);
        let entry_parent_tool_use_id = json_value
            .get("parent_tool_use_id")
            .or_else(|| json_value.get("parentToolUseId"))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .or_else(|| forced_parent_tool_use_id.map(|value| value.to_string()));

        match entry_type {
            Some("user") => {
                if json_value
                    .get("isCompactSummary")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
                {
                    continue;
                }

                if json_value
                    .get("isMeta")
                    .and_then(|m| m.as_bool())
                    .unwrap_or(false)
                {
                    debug!("Filtering out isMeta message");
                    continue;
                }

                if let Some(message) = json_value.get("message") {
                    if let Some(content) = message.get("content") {
                        if let Some(content_array) = content.as_array() {
                            let has_tool_result = content_array.iter().any(|item| {
                                item.get("type").and_then(|t| t.as_str()) == Some("tool_result")
                            });

                            if has_tool_result {
                                for item in content_array {
                                    if item.get("type").and_then(|t| t.as_str()) != Some("tool_result")
                                    {
                                        continue;
                                    }

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

                                    if let Some(tool_use_id) = tool_use_id {
                                        let target_index = tool_use_to_assistant
                                            .get(&tool_use_id)
                                            .copied()
                                            .or_else(|| {
                                                json_value
                                                    .get("sourceToolAssistantUUID")
                                                    .and_then(|uuid| uuid.as_str())
                                                    .and_then(|uuid| {
                                                        assistant_uuid_to_index.get(uuid).copied()
                                                    })
                                            });

                                        if let Some(idx) = target_index {
                                            if let Some(msg) = messages.get_mut(idx) {
                                                if msg.tool_results.is_none() {
                                                    msg.tool_results = Some(HashMap::new());
                                                    msg.tool_result_errors = Some(HashMap::new());
                                                }
                                                if let Some(ref mut results) = msg.tool_results {
                                                    results.insert(
                                                        tool_use_id.clone(),
                                                        result_content,
                                                    );
                                                }
                                                if let Some(ref mut errors) = msg.tool_result_errors
                                                {
                                                    errors.insert(tool_use_id, is_error);
                                                }
                                            }
                                        } else {
                                            pending_tool_results.push((
                                                tool_use_id.clone(),
                                                result_content.clone(),
                                                is_error,
                                                json_value
                                                    .get("sourceToolAssistantUUID")
                                                    .and_then(|uuid| uuid.as_str())
                                                    .map(|uuid| uuid.to_string()),
                                            ));
                                            warn!(
                                                tool_use_id = %tool_use_id,
                                                source_tool_assistant_uuid = ?json_value.get("sourceToolAssistantUUID").and_then(|uuid| uuid.as_str()),
                                                "Failed to associate tool_result with assistant message while loading session history"
                                            );
                                        }
                                    }
                                }
                                continue;
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
                        msg.parent_tool_use_id = entry_parent_tool_use_id.clone();
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
                        msg.parent_tool_use_id = entry_parent_tool_use_id.clone();

                        if !attachments.is_empty() {
                            msg.attachments = Some(attachments);
                        }
                        msg
                    };

                    messages.push(msg);
                }
            }
            Some("assistant") => {
                if let Some(message) = json_value.get("message") {
                    if let Some(content) = message.get("content") {
                        if let Some(content_array) = content.as_array() {
                            let content_blocks_json: Vec<serde_json::Value> =
                                content_array.iter().cloned().collect();

                            let text_parts: Vec<&str> = content_array
                                .iter()
                                .filter_map(|block| {
                                    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                                        block.get("text").and_then(|t| t.as_str())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            let text_raw = text_parts.join("\n");

                            let (text, role) = match normalize_cli_message(&text_raw) {
                                Some(normalized) => {
                                    normalized.role_or(MessageRole::Assistant)
                                }
                                None => {
                                    debug!("Filtering out normalized assistant message");
                                    continue;
                                }
                            };

                            let mut tool_calls = Vec::new();
                            let mut tool_results: HashMap<String, String> = HashMap::new();
                            let mut tool_result_errors: HashMap<String, bool> = HashMap::new();

                            for block in content_array {
                                if let Some(tool_call) = parse_tool_use_block(block) {
                                    let future_index = messages.len();
                                    tool_use_to_assistant
                                        .insert(tool_call.id.clone(), future_index);
                                    tool_calls.push(tool_call);
                                } else if block.get("type").and_then(|t| t.as_str())
                                    == Some("tool_result")
                                {
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
                            msg.parent_tool_use_id = entry_parent_tool_use_id.clone();
                            if !tool_results.is_empty() {
                                msg.tool_results = Some(tool_results);
                                msg.tool_result_errors = Some(tool_result_errors);
                            }
                            messages.push(msg);
                            if let Some(uuid) =
                                json_value.get("uuid").and_then(|v| v.as_str())
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
                    msg.parent_tool_use_id = entry_parent_tool_use_id.clone();
                    messages.push(msg);
                }
            }
            _ => {
                debug!("Ignoring session entry with type: {:?}", entry_type);
            }
        }
    }

    for (tool_use_id, result_content, is_error, source_assistant_uuid) in pending_tool_results {
        let target_index = messages
            .iter()
            .enumerate()
            .find_map(|(index, msg)| {
                let matches_tool_call = msg.tool_calls.iter().any(|call| call.id == tool_use_id);
                if matches_tool_call {
                    return Some(index);
                }

                let matches_source_uuid = source_assistant_uuid
                    .as_deref()
                    .zip(msg.checkpoint_uuid.as_deref())
                    .map(|(source_uuid, checkpoint_uuid)| source_uuid == checkpoint_uuid)
                    .unwrap_or(false);

                if matches_source_uuid {
                    Some(index)
                } else {
                    None
                }
            });

        if let Some(idx) = target_index {
            if let Some(msg) = messages.get_mut(idx) {
                if msg.tool_results.is_none() {
                    msg.tool_results = Some(HashMap::new());
                    msg.tool_result_errors = Some(HashMap::new());
                }
                if let Some(ref mut results) = msg.tool_results {
                    results.insert(tool_use_id.clone(), result_content);
                }
                if let Some(ref mut errors) = msg.tool_result_errors {
                    errors.insert(tool_use_id, is_error);
                }
            }
        }
    }

    messages
}

fn sort_messages_by_created_at(messages: &mut [Message]) {
    messages.sort_by_key(|message| message.created_at);
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

        let main_entries = read_jsonl_entries(&session_path)?;
        let mut messages = parse_entries_into_messages(main_entries, session_id, None);
        let mut parent_candidates = collect_parent_tool_candidates(&messages);

        let subagents_dir = session_path.with_extension("").join("subagents");
        if subagents_dir.exists() {
            let mut subagent_paths = std::fs::read_dir(&subagents_dir)
                .map_err(|e| format!("Failed to read subagents directory: {}", e))?
                .filter_map(|entry| entry.ok().map(|value| value.path()))
                .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("jsonl"))
                .collect::<Vec<_>>();
            subagent_paths.sort();

            for subagent_path in subagent_paths {
                let entries = read_jsonl_entries(&subagent_path)?;
                let meta_path = subagent_path.with_extension("meta.json");
                let meta = std::fs::read_to_string(&meta_path)
                    .ok()
                    .and_then(|content| serde_json::from_str::<SubagentMeta>(&content).ok())
                    .unwrap_or_default();
                let subagent_parent_tool_use_id =
                    match_subagent_parent_tool(&mut parent_candidates, &meta, first_entry_timestamp(&entries));

                let mut subagent_messages = parse_entries_into_messages(
                    entries,
                    session_id,
                    subagent_parent_tool_use_id.as_deref(),
                );
                messages.append(&mut subagent_messages);
            }
        }

        sort_messages_by_created_at(&mut messages);

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
            r#"{"type":"user","sourceToolAssistantUUID":"assistant-1","toolUseResult":"InputValidationError: expected string","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_1","is_error":true,"content":"<tool_use_error>InputValidationError: Skill failed due to the following issue:\nThe required parameter `skill` is missing</tool_use_error>"}]}}
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
    fn test_load_messages_includes_subagent_history_from_sidecar_files() {
        let temp_root = std::env::temp_dir().join(format!(
            "claude-desk-session-subagent-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let claude_dir = temp_root.join(".claude");
        let project_path = "/tmp/demo-project";
        let session_id = "session-subagent";
        let session_dir = claude_dir
            .join("projects")
            .join(SessionFileReader::encode_path(project_path));
        fs::create_dir_all(&session_dir).unwrap();

        let session_file = session_dir.join(format!("{}.jsonl", session_id));
        let session_runtime_dir = session_dir.join(session_id);
        let subagents_dir = session_runtime_dir.join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();

        let payload = concat!(
            r#"{"type":"user","uuid":"user-1","parentUuid":null,"timestamp":"2026-04-22T05:26:50.751Z","message":{"role":"user","content":[{"type":"text","text":"使用agent探索架构模式"}]}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-1","parentUuid":"user-1","timestamp":"2026-04-22T05:26:57.059Z","message":{"id":"msg_main","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"tool_use","id":"call_agent_1","name":"Agent","input":{"description":"探索 openclaw 架构模式","prompt":"探索 openclaw 项目的架构模式","subagent_type":"Explore"}}]}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-2","parentUuid":"assistant-1","timestamp":"2026-04-22T05:27:10.000Z","message":{"id":"msg_main_2","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"text","text":"这里是主线程最终总结。"}]}}"#,
            "\n"
        );
        fs::write(&session_file, payload).unwrap();

        let subagent_meta = r#"{"agentType":"Explore","description":"探索 openclaw 架构模式"}"#;
        fs::write(
            subagents_dir.join("agent-a1.meta.json"),
            subagent_meta,
        )
        .unwrap();

        let subagent_payload = concat!(
            r#"{"parentUuid":null,"isSidechain":true,"agentId":"a1","type":"user","timestamp":"2026-04-22T05:26:57.066Z","message":{"role":"user","content":"探索 openclaw 项目的架构模式"}}"#,
            "\n",
            r#"{"parentUuid":"user-sub-1","isSidechain":true,"agentId":"a1","type":"assistant","timestamp":"2026-04-22T05:26:59.568Z","message":{"id":"msg_sub_1","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"text","text":"我来帮您深入探索 openclaw 项目的架构模式。"}]}}"#,
            "\n"
        );
        fs::write(
            subagents_dir.join("agent-a1.jsonl"),
            subagent_payload,
        )
        .unwrap();

        let reader = SessionFileReader { claude_dir };
        let messages = reader.load_messages(project_path, session_id).unwrap();

        assert_eq!(messages.len(), 5);
        assert!(
            messages
                .iter()
                .any(|message| message.parent_tool_use_id.as_deref() == Some("call_agent_1"))
        );
        let parent_index = messages
            .iter()
            .position(|message| {
                message
                    .content_blocks
                    .as_ref()
                    .map(|blocks| {
                        blocks.iter().any(|block| {
                            block.get("type").and_then(|value| value.as_str()) == Some("tool_use")
                                && block.get("id").and_then(|value| value.as_str())
                                    == Some("call_agent_1")
                        })
                    })
                    .unwrap_or(false)
            })
            .expect("parent agent message should exist");
        let child_index = messages
            .iter()
            .position(|message| message.parent_tool_use_id.as_deref() == Some("call_agent_1"))
            .expect("subagent child message should exist");
        let final_summary_index = messages
            .iter()
            .position(|message| match &message.content {
                MessageContent::Text(text) => text.contains("主线程最终总结"),
                _ => false,
            })
            .expect("main summary should exist");

        assert!(parent_index < child_index);
        assert!(child_index < final_summary_index);

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn test_load_messages_keeps_parallel_agent_branches_under_active_parent() {
        let temp_root = std::env::temp_dir().join(format!(
            "claude-desk-session-parallel-agent-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let claude_dir = temp_root.join(".claude");
        let project_path = "/tmp/demo-project";
        let session_id = "session-parallel-agent";
        let session_dir = claude_dir
            .join("projects")
            .join(SessionFileReader::encode_path(project_path));
        fs::create_dir_all(&session_dir).unwrap();

        let session_file = session_dir.join(format!("{}.jsonl", session_id));
        let payload = concat!(
            r#"{"type":"user","uuid":"user-1","parentUuid":null,"timestamp":"2026-04-22T14:17:50.000Z","message":{"role":"user","content":"处理两个项目"}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-thinking","parentUuid":"user-1","timestamp":"2026-04-22T14:17:52.000Z","message":{"id":"msg_thinking","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"thinking","thinking":"准备启动两个 agent"}]}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-agent-1","parentUuid":"assistant-thinking","timestamp":"2026-04-22T14:17:53.419Z","message":{"id":"msg_agent_1","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"tool_use","id":"call_agent_1","name":"Agent","input":{"description":"处理 wb-antispam-root 项目","prompt":"处理 wb-antispam-root","name":"antispam-agent"}}]}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-agent-2","parentUuid":"assistant-agent-1","timestamp":"2026-04-22T14:17:54.309Z","message":{"id":"msg_agent_2","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"tool_use","id":"call_agent_2","name":"Agent","input":{"description":"处理 pf_soc_audit 项目","prompt":"处理 pf_soc_audit","name":"soc-audit-agent"}}]}}"#,
            "\n",
            r#"{"type":"user","uuid":"user-result-2","parentUuid":"assistant-agent-2","timestamp":"2026-04-22T14:18:00.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_agent_2","content":[{"type":"text","text":"pf_soc_audit 结果"}]}]}}"#,
            "\n",
            r#"{"type":"user","uuid":"user-result-1","parentUuid":"assistant-agent-1","timestamp":"2026-04-22T14:18:05.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call_agent_1","content":[{"type":"text","text":"wb-antispam-root 结果"}]}]}}"#,
            "\n",
            r#"{"type":"assistant","uuid":"assistant-final","parentUuid":"user-result-1","timestamp":"2026-04-22T14:18:06.000Z","message":{"id":"msg_final","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"text","text":"最终汇总"}]}}"#
        );
        fs::write(&session_file, payload).unwrap();

        let reader = SessionFileReader { claude_dir };
        let messages = reader.load_messages(project_path, session_id).unwrap();

        assert!(messages.iter().any(|message| {
            message
                .content_blocks
                .as_ref()
                .map(|blocks| {
                    blocks.iter().any(|block| {
                        block.get("type").and_then(|value| value.as_str()) == Some("tool_use")
                            && block.get("id").and_then(|value| value.as_str())
                                == Some("call_agent_2")
                    })
                })
                .unwrap_or(false)
        }));

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
