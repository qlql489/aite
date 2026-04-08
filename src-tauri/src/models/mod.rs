// Data models for Aite

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Aite 应用配置目录名称
pub const AITE_CONFIG_DIR: &str = ".aite";

/// Aite 应用配置文件名
pub const AITE_APP_CONFIG_FILE: &str = "app-config.json";

/// 获取 Aite 配置目录路径
pub fn get_aite_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(AITE_CONFIG_DIR)
}

/// Session representing a Claude conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Session {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now(),
        }
    }

    pub fn new_with_id(id: String) -> Self {
        Self {
            id,
            created_at: chrono::Utc::now(),
        }
    }
}

/// Message in a session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: MessageRole,
    pub content: MessageContent,
    pub tool_calls: Vec<ToolCall>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_uuid: Option<String>,

    // 历史消息加载时保留的结构化数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_blocks: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_results: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result_errors: Option<std::collections::HashMap<String, bool>>,

    // 文件附件（从 @path 格式解析）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<MessageAttachment>>,

    // Token 使用统计
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

impl Message {
    pub fn new(session_id: String, role: MessageRole, content: MessageContent) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id,
            role,
            content,
            tool_calls: Vec::new(),
            created_at: chrono::Utc::now(),
            checkpoint_uuid: None,
            content_blocks: None,
            tool_results: None,
            tool_result_errors: None,
            attachments: None,
            model: None,
            usage: None,
        }
    }
}

/// Message role (user, assistant or system)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Message content (text or structured blocks)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// Structured content block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    pub r#type: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_source: Option<MediaSource>,
}

/// Media source for images or files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaSource {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub data: String,
}

/// Tool call made by Claude
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    #[serde(alias = "input_tokens")]
    pub input_tokens: u64,
    #[serde(alias = "output_tokens")]
    pub output_tokens: u64,
    #[serde(alias = "cache_creation_input_tokens")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(alias = "cache_read_input_tokens")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
}

/// 文件附件信息（从 @path 格式解析）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageAttachment {
    pub name: String,
    pub path: String,
    pub is_image: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

/// SDK initialization status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkStatus {
    pub initialized: bool,
    pub cli_path: Option<String>,
    pub error: Option<String>,
}

impl SdkStatus {
    pub fn not_initialized() -> Self {
        Self {
            initialized: false,
            cli_path: None,
            error: None,
        }
    }

    pub fn initialized(cli_path: String) -> Self {
        Self {
            initialized: true,
            cli_path: Some(cli_path),
            error: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            initialized: false,
            cli_path: None,
            error: Some(error),
        }
    }
}

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl RpcRequest {
    pub fn new(method: String, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Uuid::new_v4().to_string(),
            method,
            params,
        }
    }
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    pub fn result(id: String, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: String, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    // Standard JSON-RPC error codes
    pub fn parse_error() -> Self {
        Self {
            code: -32700,
            message: "Parse error".to_string(),
            data: None,
        }
    }

    pub fn invalid_request() -> Self {
        Self {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: None,
        }
    }

    pub fn method_not_found(method: String) -> Self {
        Self {
            code: -32601,
            message: format!("Method not found: {}", method),
            data: None,
        }
    }

    pub fn invalid_params() -> Self {
        Self {
            code: -32602,
            message: "Invalid params".to_string(),
            data: None,
        }
    }

    pub fn internal_error(message: String) -> Self {
        Self {
            code: -32603,
            message,
            data: None,
        }
    }
}

/// Source/scope of a command
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandScope {
    Global,
    Project,
    Plugin,
    BuiltIn,
}

/// A command/skill that can be invoked by the user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// Unique identifier for the command
    pub id: String,
    /// Display name/label
    pub label: String,
    /// Command value (e.g., "/commit")
    pub value: String,
    /// Description of what the command does
    pub description: String,
    /// Hint for arguments (e.g., "[--dry-run]")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
    /// Full content of the skill markdown file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Source/scope of the command
    pub scope: CommandScope,
    /// Path to the command file (for non-built-in commands)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Whether this is a built-in command that executes immediately
    #[serde(default)]
    pub built_in: bool,
    /// Whether this command executes immediately (without sending to Claude)
    #[serde(default)]
    pub immediate: bool,
}

/// Response containing all available commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandsResponse {
    pub commands: Vec<Command>,
}

/// Information about a session/conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: String,
    pub title: String,
    pub message_count: usize,
    pub file_size: u64,
    pub updated_at: u64,
    pub created_at: u64,
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 首次安装向导是否已完成
    #[serde(default)]
    pub setup_completed: bool,

    /// Whether streaming output is enabled
    #[serde(default)]
    pub streaming_enabled: bool,

    /// Whether debug logging to ~/.aite/debug.log is enabled
    #[serde(default = "default_debug_enabled")]
    pub debug_enabled: bool,

    /// Connection mode for Claude CLI
    #[serde(default)]
    pub connection_mode: ConnectionMode,

    /// 主题颜色 (hex 格式，如 #3b82f6)
    #[serde(default = "default_theme_color")]
    pub theme_color: String,

    /// 主题模式
    #[serde(default)]
    pub theme_mode: ThemeMode,

    /// 界面基础字号（影响 rem 体系的界面文字）
    #[serde(default = "default_interface_font_size")]
    pub interface_font_size: u16,

    /// 对话内容字号（消息正文）
    #[serde(default = "default_chat_font_size")]
    pub chat_font_size: u16,

    /// 全局 API 供应商列表
    #[serde(default)]
    pub providers: Vec<ApiProvider>,

    /// 全局默认供应商 ID
    #[serde(default)]
    pub active_provider_id: Option<String>,

    #[serde(default = "default_inherit_system_config")]
    pub inherit_system_config: bool,

    #[serde(default)]
    pub claude_cli_extra_args: Vec<String>,

    #[serde(default)]
    pub claude_settings_anthropic_env_backup: Option<serde_json::Value>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            setup_completed: false,
            streaming_enabled: false,
            debug_enabled: default_debug_enabled(),
            connection_mode: ConnectionMode::default(),
            theme_color: default_theme_color(),
            theme_mode: ThemeMode::default(),
            interface_font_size: default_interface_font_size(),
            chat_font_size: default_chat_font_size(),
            providers: Vec::new(),
            active_provider_id: None,
            inherit_system_config: default_inherit_system_config(),
            claude_cli_extra_args: Vec::new(),
            claude_settings_anthropic_env_backup: None,
        }
    }
}

fn default_inherit_system_config() -> bool {
    true
}

fn default_debug_enabled() -> bool {
    true
}

fn default_theme_color() -> String {
    "#3b82f6".to_string()
}

fn default_interface_font_size() -> u16 {
    16
}

fn default_chat_font_size() -> u16 {
    14
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModel {
    pub model: String,
    pub model_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiProvider {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub homepage_url: Option<String>,
    pub base_url: String,
    #[serde(default = "default_provider_enabled")]
    pub enabled: bool,
    pub api_protocol: ApiProtocol,
    pub auth_type: AuthType,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub models: Vec<ProviderModel>,
    #[serde(default)]
    pub primary_model: Option<String>,
    #[serde(default)]
    pub extra_env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub upstream_format: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ApiProtocol {
    #[default]
    Anthropic,
    Openai,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthType {
    AuthToken,
    ApiKey,
    #[default]
    Both,
}

fn default_provider_enabled() -> bool {
    true
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigPayload {
    #[serde(default)]
    pub providers: Vec<ApiProvider>,
    #[serde(default)]
    pub active_provider_id: Option<String>,

    #[serde(default = "default_inherit_system_config")]
    pub inherit_system_config: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionProviderEnv {
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_protocol: Option<ApiProtocol>,
    #[serde(default)]
    pub auth_type: Option<AuthType>,
    #[serde(default)]
    pub extra_env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub upstream_format: Option<String>,
}

/// Connection mode for Claude CLI
/// 0 = WebSocket, 1 = Stdio
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionMode {
    /// Use WebSocket SDK mode (--sdk-url flag) - represented as 0
    WebSocket = 0,
    /// Use stdin/stdout mode (--input-format/--output-format stream-json) - represented as 1
    Stdio = 1,
}

impl Default for ConnectionMode {
    fn default() -> Self {
        Self::Stdio // Default to stdin/stdout mode (value 1)
    }
}

impl Serialize for ConnectionMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value = match self {
            Self::WebSocket => 0,
            Self::Stdio => 1,
        };
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ConnectionMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        match value {
            0 => Ok(Self::WebSocket),
            1 => Ok(Self::Stdio),
            _ => Err(serde::de::Error::custom(format!(
                "Invalid connection mode value: {}, expected 0 (WebSocket) or 1 (Stdio)",
                value
            ))),
        }
    }
}

impl std::fmt::Display for ConnectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WebSocket => write!(f, "0 (WebSocket)"),
            Self::Stdio => write!(f, "1 (Stdio)"),
        }
    }
}

impl std::str::FromStr for ConnectionMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Support both numeric and string formats
        match s.trim().to_lowercase().as_str() {
            "0" | "websocket" | "ws" => Ok(Self::WebSocket),
            "1" | "stdio" | "stdin" | "stdin_stdout" => Ok(Self::Stdio),
            _ => Err(format!(
                "Unknown connection mode: {}, expected 0 (WebSocket) or 1 (Stdio)",
                s
            )),
        }
    }
}
