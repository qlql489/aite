// Claude Code WebSocket SDK Protocol Types
//
// Based on: https://github.com/The-Vibe-Company/companion/blob/main/WEBSOCKET_PROTOCOL_REVERSED.md
//
// This is the NDJSON (newline-delimited JSON) protocol used by Claude Code CLI
// when launched with the --sdk-url flag.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ============================================================================
// Permission Mode
// ============================================================================

/// Claude Code 支持的 4 种权限模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// 默认 - 询问所有权限
    Default,
    /// 自动批准编辑类工具
    AcceptEdits,
    /// 绕过所有权限（yolo 模式）
    BypassPermissions,
    /// 计划模式
    Plan,
}

impl PermissionMode {
    /// 从字符串解析权限模式
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "default" | "Default" => Some(Self::Default),
            "acceptEdits" | "AcceptEdits" => Some(Self::AcceptEdits),
            "bypassPermissions" | "BypassPermissions" => Some(Self::BypassPermissions),
            "plan" | "Plan" => Some(Self::Plan),
            _ => None,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::BypassPermissions => "bypassPermissions",
            Self::Plan => "plan",
        }
    }
}

impl std::fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Message Envelope
// ============================================================================

/// All messages in the protocol share this envelope format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SdkMessage {
    // Bidirectional messages
    #[serde(rename = "control_request")]
    ControlRequest(ControlRequest),

    // Server → CLI messages
    #[serde(rename = "user")]
    User(UserMessage),

    #[serde(rename = "control_response")]
    ControlResponse(ControlResponse),

    #[serde(rename = "control_cancel_request")]
    ControlCancelRequest { request_id: String },

    #[serde(rename = "keep_alive")]
    KeepAlive,

    #[serde(rename = "update_environment_variables")]
    UpdateEnvironmentVariables { variables: HashMap<String, String> },

    // CLI → Server messages
    #[serde(rename = "system")]
    System(SystemMessage),

    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),

    #[serde(rename = "result")]
    Result(ResultMessage),

    #[serde(rename = "stream_event")]
    StreamEvent(StreamEventMessage),

    #[serde(rename = "tool_progress")]
    ToolProgress(ToolProgressMessage),

    #[serde(rename = "tool_use_summary")]
    ToolUseSummary(ToolUseSummaryMessage),

    #[serde(rename = "auth_status")]
    AuthStatus(AuthStatusMessage),
}

// ============================================================================
// User Message (Server → CLI)
// ============================================================================

/// Send a prompt or follow-up message to the Claude Code agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub message: UserMessageContent,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_synthetic: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserMessageContent {
    Simple {
        role: String,
        content: String,
    },
    Structured {
        role: String,
        content: Vec<ContentBlock>,
    },
}

// ============================================================================
// Control Response (Server → CLI)
// ============================================================================

/// Response to a control request (e.g., permission approval)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlResponse {
    pub response: ControlResponseInner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlResponseInner {
    pub subtype: ControlResponseType,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlResponseType {
    Success,
    Error,
}

/// Response from initialize control request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<CommandInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_output_styles: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<ModelInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_mode: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
    #[serde(rename = "argumentHint", skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<serde_json::Value>,
}

impl CommandInfo {
    /// 获取 argument_hint 的字符串表示
    /// 如果是数组，则用空格连接；如果是字符串，直接返回
    pub fn argument_hint_str(&self) -> Option<String> {
        match &self.argument_hint {
            None => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Array(arr)) => {
                let strings: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                if strings.is_empty() {
                    None
                } else {
                    Some(strings.join(" "))
                }
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub value: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_source: Option<String>,
}

// ============================================================================
// System Message (CLI → Server)
// ============================================================================

/// System messages from the CLI (init, status, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<McpServerInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<PermissionMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_code_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slash_commands: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<Vec<PluginInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_event: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_delay_ms: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "deserialize_optional_stringified_value"
    )]
    pub error_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SystemMessage {
    pub fn display_text(&self) -> Option<String> {
        if self.subtype.as_deref() == Some("transport_error") {
            let error = clean_system_detail(self.error.as_deref())
                .unwrap_or_else(|| "未知解析错误".to_string());
            return Some(format!("协议消息解析失败：{}", error));
        }

        if self.subtype.as_deref() != Some("api_retry") {
            return None;
        }

        let mut details = Vec::new();

        match (self.attempt, self.max_retries) {
            (Some(attempt), Some(max_retries)) if max_retries > 0 => {
                details.push(format!("已重试 {}/{} 次", attempt, max_retries));
            }
            (Some(attempt), _) if attempt > 0 => {
                details.push(format!("已重试 {} 次", attempt));
            }
            _ => {}
        }

        if let Some(retry_delay_ms) = self.retry_delay_ms {
            details.push(format!(
                "{}后再次尝试",
                format_retry_delay_ms(retry_delay_ms)
            ));
        }

        if let Some(error_status) = clean_system_detail(self.error_status.as_deref()) {
            details.push(format!("状态：{}", error_status));
        }

        if let Some(error) = clean_system_detail(self.error.as_deref()) {
            details.push(format!("错误：{}", error));
        }

        if details.is_empty() {
            Some("接口请求失败，正在准备重试".to_string())
        } else {
            Some(format!(
                "接口请求失败，正在准备重试（{}）",
                details.join("，")
            ))
        }
    }
}

fn clean_system_detail(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("unknown")
        || trimmed.eq_ignore_ascii_case("null")
    {
        return None;
    }

    Some(trimmed.to_string())
}

fn deserialize_optional_stringified_value<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };

    let stringified = match value {
        Value::Null => return Ok(None),
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => serde_json::to_string(&other).map_err(serde::de::Error::custom)?,
    };

    Ok(Some(stringified))
}

fn format_retry_delay_ms(retry_delay_ms: f64) -> String {
    if retry_delay_ms >= 1000.0 {
        format!("{:.1} 秒", retry_delay_ms / 1000.0)
    } else {
        format!("{} 毫秒", retry_delay_ms.round() as u64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub path: String,
}

// ============================================================================
// Assistant Message (CLI → Server)
// ============================================================================

/// Full assistant message after LLM completes a response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub message: AssistantContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub uuid: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantContent {
    pub id: String,
    #[serde(rename = "type")]
    pub content_type: String,
    pub role: String,
    pub model: String,
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    #[serde(alias = "input_tokens")]
    pub input_tokens: u64,
    #[serde(alias = "output_tokens")]
    pub output_tokens: u64,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "cache_read_input_tokens"
    )]
    pub cache_read_input_tokens: Option<u64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "cache_creation_input_tokens"
    )]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "context_window")]
    pub context_window: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "max_output_tokens")]
    pub max_output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "cost_usd")]
    pub cost_usd: Option<f64>,
}

// ============================================================================
// Content Blocks
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "image")]
    Image { source: ImageSource },

    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: ToolResultContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },

    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        budget_tokens: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    pub r#type: String,
    pub media_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    Single(String),
    Multiple(Vec<ContentBlock>),
}

// ============================================================================
// Result Message (CLI → Server)
// ============================================================================

/// Sent when the query finishes (success or error)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultMessage {
    pub subtype: String,
    pub is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
    pub duration_ms: u64,
    pub duration_api_ms: u64,
    pub num_turns: u64,
    pub total_cost_usd: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "modelUsage")]
    pub model_usage: Option<HashMap<String, ModelUsage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_denials: Option<Vec<PermissionDenial>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<Value>,
    pub uuid: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionDenial {
    pub tool_name: String,
    pub tool_use_id: String,
    pub tool_input: Value,
}

// ============================================================================
// Stream Event (CLI → Server)
// ============================================================================

/// Token-by-token streaming events (only when --verbose flag is used)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEventMessage {
    pub event: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_tool_use_id: Option<String>,
    pub uuid: String,
    pub session_id: String,
}

// ============================================================================
// Tool Progress (CLI → Server)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProgressMessage {
    pub tool_use_id: String,
    pub tool_name: String,
    pub parent_tool_use_id: Option<String>,
    pub elapsed_time_seconds: f64,
    pub uuid: String,
    pub session_id: String,
}

// ============================================================================
// Tool Use Summary (CLI → Server)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseSummaryMessage {
    pub summary: String,
    pub preceding_tool_use_ids: Vec<String>,
    pub uuid: String,
    pub session_id: String,
}

// ============================================================================
// Auth Status (CLI → Server)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatusMessage {
    pub is_authenticating: bool,
    pub output: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub uuid: String,
    pub session_id: String,
}

// ============================================================================
// Control Request (CLI → Server)
// ============================================================================

/// Control requests from CLI (permission requests, hook callbacks, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlRequest {
    pub request_id: String,
    pub request: ControlRequestPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "subtype")]
pub enum ControlRequestPayload {
    #[serde(rename = "can_use_tool")]
    CanUseTool(CanUseToolRequest),

    #[serde(rename = "hook_callback")]
    HookCallback(HookCallbackRequest),

    #[serde(rename = "initialize")]
    Initialize(InitializeRequest),

    /// Server → CLI: 设置权限模式（default / acceptEdits / bypassPermissions / plan）
    #[serde(rename = "set_permission_mode")]
    SetPermissionMode { mode: String },

    /// Server → CLI: 中断当前执行
    #[serde(rename = "interrupt")]
    Interrupt,

    /// Server → CLI: 基于用户消息 checkpoint 回退文件
    #[serde(rename = "rewind_files")]
    RewindFiles { user_message_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanUseToolRequest {
    pub tool_name: String,
    pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_suggestions: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_reason: Option<String>,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCallbackRequest {
    pub callback_id: String,
    pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk_mcp_servers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub append_system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<Value>,
}

// ============================================================================
// NDJSON Serialization Helpers
// ============================================================================

/// Serialize a message as NDJSON (JSON + newline)
pub fn serialize_message(msg: &SdkMessage) -> Result<String, serde_json::Error> {
    let json = serde_json::to_string(msg)?;
    Ok(format!("{}\n", json))
}

/// Deserialize a single NDJSON line
pub fn deserialize_message(line: &str) -> Result<SdkMessage, serde_json::Error> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_user_message() {
        let msg = SdkMessage::User(UserMessage {
            message: UserMessageContent::Simple {
                role: "user".to_string(),
                content: "Hello!".to_string(),
            },
            parent_tool_use_id: None,
            session_id: "".to_string(),
            uuid: None,
            is_synthetic: None,
        });

        let json = serialize_message(&msg).unwrap();
        assert!(json.ends_with('\n'));
        assert!(json.contains(r#""type":"user""#));
    }

    #[test]
    fn test_deserialize_system_init() {
        let json = r#"{"type":"system","subtype":"init","cwd":"/path","session_id":"abc123","model":"claude-opus-4-6","tools":["Bash","Read"],"uuid":"xyz"}"#;
        let msg = deserialize_message(json).unwrap();

        match msg {
            SdkMessage::System(sys) => {
                assert_eq!(sys.subtype, Some("init".to_string()));
                assert_eq!(sys.session_id, Some("abc123".to_string()));
            }
            _ => panic!("Expected system message"),
        }
    }

    #[test]
    fn test_system_api_retry_display_text() {
        let json = r#"{"type":"system","subtype":"api_retry","attempt":2,"max_retries":10,"retry_delay_ms":1122.1052712959254,"error_status":null,"error":"unknown","session_id":"abc123","uuid":"xyz"}"#;
        let msg = deserialize_message(json).unwrap();

        match msg {
            SdkMessage::System(sys) => {
                assert_eq!(sys.subtype.as_deref(), Some("api_retry"));
                assert_eq!(
                    sys.display_text().as_deref(),
                    Some("接口请求失败，正在准备重试（已重试 2/10 次，1.1 秒后再次尝试）")
                );
            }
            _ => panic!("Expected system message"),
        }
    }

    #[test]
    fn test_system_api_retry_accepts_numeric_error_status() {
        let json = r#"{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":502.1013160393518,"error_status":429,"error":"rate_limit","session_id":"abc123","uuid":"xyz"}"#;
        let msg = deserialize_message(json).unwrap();

        match msg {
            SdkMessage::System(sys) => {
                assert_eq!(sys.error_status.as_deref(), Some("429"));
                assert_eq!(
                    sys.display_text().as_deref(),
                    Some("接口请求失败，正在准备重试（已重试 1/10 次，502 毫秒后再次尝试，状态：429，错误：rate_limit）")
                );
            }
            _ => panic!("Expected system message"),
        }
    }

    #[test]
    fn test_transport_error_display_text() {
        let msg = SystemMessage {
            subtype: Some("transport_error".to_string()),
            cwd: None,
            session_id: Some("abc123".to_string()),
            tools: None,
            mcp_servers: None,
            model: None,
            permission_mode: None,
            api_key_source: None,
            claude_code_version: None,
            slash_commands: None,
            agents: None,
            skills: None,
            plugins: None,
            output_style: None,
            uuid: None,
            status: None,
            task_id: None,
            task_status: None,
            output_file: None,
            summary: None,
            hook_id: None,
            hook_name: None,
            hook_event: None,
            output: None,
            stdout: None,
            stderr: None,
            exit_code: None,
            outcome: None,
            attempt: None,
            max_retries: None,
            retry_delay_ms: None,
            error_status: None,
            error: Some("invalid type".to_string()),
        };

        assert_eq!(
            msg.display_text().as_deref(),
            Some("协议消息解析失败：invalid type")
        );
    }
}
