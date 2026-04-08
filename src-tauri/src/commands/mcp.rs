// MCP (Model Context Protocol) 服务命令模块
// 管理 Claude 的 MCP 服务器配置

use crate::models::get_aite_config_dir;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// MCP 服务器类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpServerType {
    Stdio,
    Http,
    Sse,
}

/// MCP 服务器规格
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerSpec {
    #[serde(rename = "type", default)]
    pub server_type: Option<McpServerType>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub url: Option<String>,
    #[serde(default)]
    pub env: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

/// MCP 应用启用状态
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpApps {
    #[serde(default)]
    pub claude: bool,
    #[serde(default)]
    pub codex: bool,
    #[serde(default)]
    pub gemini: bool,
}

/// MCP 服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "server")]
    pub server_spec: McpServerSpec,
    #[serde(default)]
    pub apps: McpApps,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub homepage: Option<String>,
    pub docs: Option<String>,
    pub tags: Option<Vec<String>>,
}

fn default_enabled() -> bool {
    true
}

/// MCP 服务器映射
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServersMap(pub std::collections::HashMap<String, McpServer>);

/// Aite 配置结构
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiteConfig {
    #[serde(default)]
    pub mcp: Option<McpConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Option<std::collections::HashMap<String, McpServer>>,
}

/// Claude 原生配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeConfig {
    #[serde(default)]
    pub mcp_servers: std::collections::HashMap<String, serde_json::Value>,
}

/// MCP 列表响应
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServersResponse {
    pub servers: Vec<McpServer>,
    pub source_path: String,
    pub source_scope: String,
    pub source_target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatusInfo {
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 获取 Claude 配置路径
///
/// 优先使用 ~/.claude/.config.json，不存在时回退到 ~/.claude.json。
fn get_claude_config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    let claude_config = home.join(".claude").join(".config.json");
    if claude_config.exists() {
        claude_config
    } else {
        home.join(".claude.json")
    }
}

fn get_claude_config_path_display() -> String {
    get_claude_config_path().to_string_lossy().to_string()
}

fn normalize_workspace_path(workspace_path: Option<&str>) -> Option<&str> {
    workspace_path.and_then(|path| {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn get_mcp_source_metadata(workspace_path: Option<&str>) -> (String, String) {
    if let Some(path) = normalize_workspace_path(workspace_path) {
        (
            "project".to_string(),
            format!("projects[\"{}\"].mcpServers", path),
        )
    } else {
        ("global".to_string(), "mcpServers".to_string())
    }
}

/// 获取 Aite 配置路径列表（按优先级）
fn get_aite_config_paths() -> Vec<PathBuf> {
    let aite_dir = get_aite_config_dir();
    vec![
        aite_dir.join("config.json"),
        aite_dir.join("config.json.migrated"),
    ]
}

/// 读取 Aite 配置
fn read_aite_config() -> AiteConfig {
    let config_paths = get_aite_config_paths();

    // 尝试读取第一个存在的配置文件
    for config_path in config_paths {
        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(content) if !content.trim().is_empty() => {
                    match serde_json::from_str::<AiteConfig>(&content) {
                        Ok(config) => return config,
                        Err(e) => {
                            // 尝试解析整个 JSON 文件（可能是迁移前的格式）
                            if let Ok(json_value) =
                                serde_json::from_str::<serde_json::Value>(&content)
                            {
                                // 手动构建 AiteConfig
                                let mcp = json_value.get("mcp").cloned();
                                return AiteConfig {
                                    mcp: mcp.map(|v| {
                                        let mut mcp_config = McpConfig::default();
                                        if let Some(servers) = v.get("servers") {
                                            if let Ok(servers_map) = serde_json::from_value::<
                                                std::collections::HashMap<String, McpServer>,
                                            >(
                                                servers.clone()
                                            ) {
                                                mcp_config.servers = Some(servers_map);
                                            }
                                        }
                                        mcp_config
                                    }),
                                };
                            }
                            println!("[MCP] 解析配置文件失败: {}", e);
                        }
                    }
                }
                _ => continue,
            }
        }
    }

    AiteConfig::default()
}

/// 写入 Aite 配置
fn write_aite_config(config: &AiteConfig) -> Result<(), String> {
    // 使用第一个路径（config.json）
    let config_path = get_aite_config_paths().remove(0);

    // 确保目录存在
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&config_path, content).map_err(|e| e.to_string())
}

/// 读取 Claude 配置 JSON
fn read_claude_config_json() -> serde_json::Value {
    let config_path = get_claude_config_path();

    if !config_path.exists() {
        println!("[MCP] Claude 配置文件不存在: {:?}", config_path);
        return serde_json::Value::Object(Default::default());
    }

    match fs::read_to_string(&config_path) {
        Ok(content) if !content.trim().is_empty() => {
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(json) => json,
                Err(e) => {
                    println!("[MCP] 解析 Claude 配置失败: {}", e);
                    serde_json::Value::Object(Default::default())
                }
            }
        }
        _ => serde_json::Value::Object(Default::default()),
    }
}

/// 写入 Claude 配置 JSON
fn write_claude_config_json(full_config: &serde_json::Value) -> Result<(), String> {
    let config_path = get_claude_config_path();
    let content = serde_json::to_string_pretty(full_config).map_err(|e| e.to_string())?;
    fs::write(&config_path, content).map_err(|e| e.to_string())
}

fn read_scoped_mcp_servers(workspace_path: Option<&str>) -> HashMap<String, serde_json::Value> {
    let config = read_claude_config_json();

    let maybe_object = if let Some(path) = normalize_workspace_path(workspace_path) {
        config
            .get("projects")
            .and_then(|v| v.as_object())
            .and_then(|projects| projects.get(path))
            .and_then(|project| project.get("mcpServers"))
            .and_then(|v| v.as_object())
    } else {
        config.get("mcpServers").and_then(|v| v.as_object())
    };

    let mcp_servers: HashMap<String, serde_json::Value> = maybe_object
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    println!("[MCP] 读取到 {} 个 MCP 服务器", mcp_servers.len());
    mcp_servers
}

fn write_scoped_mcp_servers(
    workspace_path: Option<&str>,
    mcp_servers: HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    let mut full_config = read_claude_config_json();
    if !full_config.is_object() {
        full_config = serde_json::Value::Object(Default::default());
    }

    let root = full_config
        .as_object_mut()
        .ok_or_else(|| "Claude 配置根节点必须是对象".to_string())?;
    let mcp_servers_json = serde_json::to_value(mcp_servers).map_err(|e| e.to_string())?;

    if let Some(path) = normalize_workspace_path(workspace_path) {
        let projects_value = root
            .entry("projects".to_string())
            .or_insert_with(|| serde_json::Value::Object(Default::default()));
        if !projects_value.is_object() {
            *projects_value = serde_json::Value::Object(Default::default());
        }

        let projects = projects_value
            .as_object_mut()
            .ok_or_else(|| "Claude 配置中的 projects 必须是对象".to_string())?;
        let project_value = projects
            .entry(path.to_string())
            .or_insert_with(|| serde_json::Value::Object(Default::default()));
        if !project_value.is_object() {
            *project_value = serde_json::Value::Object(Default::default());
        }

        project_value
            .as_object_mut()
            .ok_or_else(|| "Claude 项目配置必须是对象".to_string())?
            .insert("mcpServers".to_string(), mcp_servers_json);
    } else {
        root.insert("mcpServers".to_string(), mcp_servers_json);
    }

    write_claude_config_json(&full_config)
}

fn read_scoped_disabled_mcp_servers(workspace_path: Option<&str>) -> HashSet<String> {
    let config = read_claude_config_json();

    let maybe_array = if let Some(path) = normalize_workspace_path(workspace_path) {
        config
            .get("projects")
            .and_then(|v| v.as_object())
            .and_then(|projects| projects.get(path))
            .and_then(|project| project.get("disabledMcpServers"))
            .and_then(|v| v.as_array())
    } else {
        config.get("disabledMcpServers").and_then(|v| v.as_array())
    };

    maybe_array
        .map(|arr| {
            arr.iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn write_scoped_disabled_mcp_servers(
    workspace_path: Option<&str>,
    disabled_servers: &HashSet<String>,
) -> Result<(), String> {
    let mut full_config = read_claude_config_json();
    if !full_config.is_object() {
        full_config = serde_json::Value::Object(Default::default());
    }

    let root = full_config
        .as_object_mut()
        .ok_or_else(|| "Claude 配置根节点必须是对象".to_string())?;

    let disabled_json = serde_json::Value::Array({
        let mut values: Vec<_> = disabled_servers
            .iter()
            .cloned()
            .map(serde_json::Value::String)
            .collect();
        values.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        values
    });

    if let Some(path) = normalize_workspace_path(workspace_path) {
        let projects_value = root
            .entry("projects".to_string())
            .or_insert_with(|| serde_json::Value::Object(Default::default()));
        if !projects_value.is_object() {
            *projects_value = serde_json::Value::Object(Default::default());
        }

        let projects = projects_value
            .as_object_mut()
            .ok_or_else(|| "Claude 配置中的 projects 必须是对象".to_string())?;
        let project_value = projects
            .entry(path.to_string())
            .or_insert_with(|| serde_json::Value::Object(Default::default()));
        if !project_value.is_object() {
            *project_value = serde_json::Value::Object(Default::default());
        }

        project_value
            .as_object_mut()
            .ok_or_else(|| "Claude 项目配置必须是对象".to_string())?
            .insert("disabledMcpServers".to_string(), disabled_json);
    } else {
        root.insert("disabledMcpServers".to_string(), disabled_json);
    }

    write_claude_config_json(&full_config)
}

fn parse_server_type(spec: &serde_json::Value) -> McpServerType {
    match spec.get("type").and_then(|v| v.as_str()) {
        Some("http") => McpServerType::Http,
        Some("sse") => McpServerType::Sse,
        _ => McpServerType::Stdio,
    }
}

async fn verify_http_server_status(name: String, url: String) -> McpServerStatusInfo {
    let client = reqwest::Client::new();
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "aite",
                "version": "0.1.0"
            }
        }
    });

    let request = client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/event-stream")
        .json(&request_body);

    match timeout(Duration::from_secs(8), request.send()).await {
        Ok(Ok(response)) => {
            if response.status().is_success() {
                McpServerStatusInfo {
                    name,
                    status: "connected".to_string(),
                    error: None,
                }
            } else {
                McpServerStatusInfo {
                    name,
                    status: "failed".to_string(),
                    error: Some(format!(
                        "HTTP {} {}",
                        response.status().as_u16(),
                        response.status()
                    )),
                }
            }
        }
        Ok(Err(error)) => McpServerStatusInfo {
            name,
            status: "failed".to_string(),
            error: Some(error.to_string()),
        },
        Err(_) => McpServerStatusInfo {
            name,
            status: "pending".to_string(),
            error: Some("连接超时".to_string()),
        },
    }
}

async fn verify_sse_server_status(name: String, url: String) -> McpServerStatusInfo {
    let client = reqwest::Client::new();
    let request = client.get(&url).header(ACCEPT, "text/event-stream");

    match timeout(Duration::from_secs(8), request.send()).await {
        Ok(Ok(response)) => {
            if response.status().is_success() {
                McpServerStatusInfo {
                    name,
                    status: "connected".to_string(),
                    error: None,
                }
            } else {
                McpServerStatusInfo {
                    name,
                    status: "failed".to_string(),
                    error: Some(format!(
                        "HTTP {} {}",
                        response.status().as_u16(),
                        response.status()
                    )),
                }
            }
        }
        Ok(Err(error)) => McpServerStatusInfo {
            name,
            status: "failed".to_string(),
            error: Some(error.to_string()),
        },
        Err(_) => McpServerStatusInfo {
            name,
            status: "pending".to_string(),
            error: Some("连接超时".to_string()),
        },
    }
}

async fn verify_stdio_server_status(
    name: String,
    command: String,
    args: Vec<String>,
    env: Option<HashMap<String, String>>,
) -> McpServerStatusInfo {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    if let Some(env_vars) = env {
        cmd.envs(env_vars);
    }

    let child_result = cmd.spawn();
    let mut child = match child_result {
        Ok(child) => child,
        Err(error) => {
            return McpServerStatusInfo {
                name,
                status: "failed".to_string(),
                error: Some(error.to_string()),
            }
        }
    };

    let init_message = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "aite",
                "version": "0.1.0"
            }
        }
    })
    .to_string()
        + "\n";

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(error) = stdin.write_all(init_message.as_bytes()).await {
            let _ = child.kill().await;
            return McpServerStatusInfo {
                name,
                status: "failed".to_string(),
                error: Some(error.to_string()),
            };
        }
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let status = timeout(Duration::from_secs(8), async {
        let mut stdout_lines = stdout.map(|out| BufReader::new(out).lines());
        let mut stderr_lines = stderr.map(|err| BufReader::new(err).lines());

        loop {
            if let Some(lines) = stdout_lines.as_mut() {
                match timeout(Duration::from_millis(150), lines.next_line()).await {
                    Ok(Ok(Some(line))) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
                            return McpServerStatusInfo {
                                name: name.clone(),
                                status: "connected".to_string(),
                                error: None,
                            };
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        return McpServerStatusInfo {
                            name: name.clone(),
                            status: "failed".to_string(),
                            error: Some(error.to_string()),
                        };
                    }
                    Err(_) => {}
                }
            }

            if let Some(lines) = stderr_lines.as_mut() {
                match timeout(Duration::from_millis(50), lines.next_line()).await {
                    Ok(Ok(Some(line))) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            return McpServerStatusInfo {
                                name: name.clone(),
                                status: "failed".to_string(),
                                error: Some(trimmed.to_string()),
                            };
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        return McpServerStatusInfo {
                            name: name.clone(),
                            status: "failed".to_string(),
                            error: Some(error.to_string()),
                        };
                    }
                    Err(_) => {}
                }
            }

            if let Ok(Some(exit_status)) = child.try_wait() {
                return McpServerStatusInfo {
                    name: name.clone(),
                    status: "failed".to_string(),
                    error: Some(format!("进程已退出: {}", exit_status)),
                };
            }
        }
    })
    .await;

    let _ = child.kill().await;

    match status {
        Ok(result) => result,
        Err(_) => McpServerStatusInfo {
            name,
            status: "pending".to_string(),
            error: Some("连接超时".to_string()),
        },
    }
}

/// 获取所有 MCP 服务器
#[tauri::command]
pub fn get_mcp_servers(workspace_path: Option<String>) -> Result<McpServersResponse, String> {
    let mcp_servers = read_scoped_mcp_servers(workspace_path.as_deref());
    let disabled_servers = read_scoped_disabled_mcp_servers(workspace_path.as_deref());
    let mut servers: Vec<McpServer> = mcp_servers
        .into_iter()
        .map(|(id, spec)| {
            let is_enabled = !disabled_servers.contains(&id);
            // 解析 server spec
            let server_type = Some(parse_server_type(&spec));

            let command = spec
                .get("command")
                .and_then(|v| v.as_str())
                .map(String::from);
            let args = spec.get("args").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });
            let url = spec.get("url").and_then(|v| v.as_str()).map(String::from);

            // 解析 env
            let env = spec.get("env").and_then(|v| {
                if let Some(obj) = v.as_object() {
                    let mut map = std::collections::HashMap::new();
                    for (key, val) in obj {
                        if let Some(s) = val.as_str() {
                            map.insert(key.clone(), s.to_string());
                        }
                    }
                    if !map.is_empty() {
                        Some(map)
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            // 解析 headers
            let headers = spec.get("headers").and_then(|v| {
                if let Some(obj) = v.as_object() {
                    let mut map = std::collections::HashMap::new();
                    for (key, val) in obj {
                        if let Some(s) = val.as_str() {
                            map.insert(key.clone(), s.to_string());
                        }
                    }
                    if !map.is_empty() {
                        Some(map)
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

            McpServer {
                id: id.clone(),
                name: Some(id),
                description: None,
                server_spec: McpServerSpec {
                    server_type,
                    command,
                    args,
                    url,
                    env,
                    headers,
                },
                apps: McpApps {
                    claude: is_enabled,
                    codex: false,
                    gemini: false,
                },
                enabled: is_enabled,
                homepage: None,
                docs: None,
                tags: None,
            }
        })
        .collect();

    servers.sort_by(|a, b| a.id.cmp(&b.id));
    let (source_scope, source_target) = get_mcp_source_metadata(workspace_path.as_deref());
    Ok(McpServersResponse {
        servers,
        source_path: get_claude_config_path_display(),
        source_scope,
        source_target,
    })
}

#[tauri::command]
pub async fn get_mcp_server_status(
    workspace_path: Option<String>,
) -> Result<Vec<McpServerStatusInfo>, String> {
    let mcp_servers = read_scoped_mcp_servers(workspace_path.as_deref());
    let disabled_servers = read_scoped_disabled_mcp_servers(workspace_path.as_deref());
    let mut statuses = Vec::new();

    for (name, spec) in mcp_servers {
        if disabled_servers.contains(&name) {
            statuses.push(McpServerStatusInfo {
                name,
                status: "disabled".to_string(),
                error: Some("服务器已禁用".to_string()),
            });
            continue;
        }

        let status = match parse_server_type(&spec) {
            McpServerType::Http => {
                let url = spec
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                if url.is_empty() {
                    McpServerStatusInfo {
                        name,
                        status: "failed".to_string(),
                        error: Some("缺少 URL 配置".to_string()),
                    }
                } else {
                    verify_http_server_status(name, url).await
                }
            }
            McpServerType::Sse => {
                let url = spec
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                if url.is_empty() {
                    McpServerStatusInfo {
                        name,
                        status: "failed".to_string(),
                        error: Some("缺少 URL 配置".to_string()),
                    }
                } else {
                    verify_sse_server_status(name, url).await
                }
            }
            McpServerType::Stdio => {
                let command = spec
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let args = spec
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|value| value.as_str().map(ToString::to_string))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let env = spec.get("env").and_then(|v| v.as_object()).map(|obj| {
                    obj.iter()
                        .filter_map(|(key, value)| {
                            value.as_str().map(|value| (key.clone(), value.to_string()))
                        })
                        .collect::<HashMap<_, _>>()
                });

                if command.is_empty() {
                    McpServerStatusInfo {
                        name,
                        status: "failed".to_string(),
                        error: Some("缺少命令配置".to_string()),
                    }
                } else {
                    verify_stdio_server_status(name, command, args, env).await
                }
            }
        };

        statuses.push(status);
    }

    statuses.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(statuses)
}

/// 验证 MCP 服务器配置
#[tauri::command]
pub fn validate_mcp_server(server: McpServer) -> Result<(bool, Vec<String>), String> {
    let mut errors = Vec::new();

    // 验证 ID
    if server.id.trim().is_empty() {
        errors.push("服务器 ID 不能为空".to_string());
    }

    // 验证服务器规格
    let spec = &server.server_spec;
    let server_type = spec.server_type.as_ref().unwrap_or(&McpServerType::Stdio);

    match server_type {
        McpServerType::Stdio => {
            if spec
                .command
                .as_ref()
                .map(|s| s.trim())
                .unwrap_or("")
                .is_empty()
            {
                errors.push("stdio 类型需要指定 command".to_string());
            }
        }
        McpServerType::Http | McpServerType::Sse => {
            if spec.url.as_ref().map(|s| s.trim()).unwrap_or("").is_empty() {
                errors.push(format!("{:?} 类型需要指定 url", server_type));
            }
            // 验证 URL 格式
            if let Some(url) = &spec.url {
                if url::Url::parse(url).is_err() {
                    errors.push("URL 格式无效".to_string());
                }
            }
        }
    }

    Ok((errors.is_empty(), errors))
}

/// 添加或更新 MCP 服务器
#[tauri::command]
pub fn upsert_mcp_server(server: McpServer, workspace_path: Option<String>) -> Result<(), String> {
    // 验证服务器配置
    let (valid, errors) = validate_mcp_server(server.clone())?;
    if !valid {
        return Err(format!("服务器配置无效: {}", errors.join(", ")));
    }

    // 读取当前配置
    let mut mcp_servers = read_scoped_mcp_servers(workspace_path.as_deref());

    // 构建 server spec
    let mut spec = serde_json::Map::new();
    if let Some(server_type) = &server.server_spec.server_type {
        spec.insert(
            "type".to_string(),
            serde_json::json!(match server_type {
                McpServerType::Stdio => "stdio",
                McpServerType::Http => "http",
                McpServerType::Sse => "sse",
            }),
        );
    }
    if let Some(command) = &server.server_spec.command {
        spec.insert("command".to_string(), serde_json::json!(command));
    }
    if let Some(args) = &server.server_spec.args {
        spec.insert("args".to_string(), serde_json::json!(args));
    }
    if let Some(url) = &server.server_spec.url {
        spec.insert("url".to_string(), serde_json::json!(url));
    }
    if let Some(env) = &server.server_spec.env {
        if !env.is_empty() {
            spec.insert("env".to_string(), serde_json::json!(env));
        }
    }
    if let Some(headers) = &server.server_spec.headers {
        if !headers.is_empty() {
            spec.insert("headers".to_string(), serde_json::json!(headers));
        }
    }

    // 添加或更新服务器
    mcp_servers.insert(server.id.clone(), serde_json::Value::Object(spec));

    write_scoped_mcp_servers(workspace_path.as_deref(), mcp_servers)?;

    let mut disabled_servers = read_scoped_disabled_mcp_servers(workspace_path.as_deref());
    if server.enabled {
        disabled_servers.remove(&server.id);
    } else {
        disabled_servers.insert(server.id.clone());
    }
    write_scoped_disabled_mcp_servers(workspace_path.as_deref(), &disabled_servers)?;

    Ok(())
}

/// 删除 MCP 服务器
#[tauri::command]
pub fn delete_mcp_server(id: String, workspace_path: Option<String>) -> Result<bool, String> {
    let mut mcp_servers = read_scoped_mcp_servers(workspace_path.as_deref());
    let mut disabled_servers = read_scoped_disabled_mcp_servers(workspace_path.as_deref());

    if mcp_servers.remove(&id).is_some() {
        write_scoped_mcp_servers(workspace_path.as_deref(), mcp_servers)?;
        disabled_servers.remove(&id);
        write_scoped_disabled_mcp_servers(workspace_path.as_deref(), &disabled_servers)?;
        return Ok(true);
    }

    Ok(false)
}

/// 切换服务器启用状态
#[tauri::command]
pub fn toggle_mcp_server_app(
    server_id: String,
    app: String,
    enabled: bool,
    workspace_path: Option<String>,
) -> Result<(), String> {
    if app != "claude" {
        return Err("只支持 claude 应用".to_string());
    }

    let mcp_servers = read_scoped_mcp_servers(workspace_path.as_deref());
    if !mcp_servers.contains_key(&server_id) {
        return Err(format!("服务器不存在: {}", server_id));
    }

    let mut disabled_servers = read_scoped_disabled_mcp_servers(workspace_path.as_deref());
    if enabled {
        disabled_servers.remove(&server_id);
    } else {
        disabled_servers.insert(server_id);
    }

    write_scoped_disabled_mcp_servers(workspace_path.as_deref(), &disabled_servers)
}

/// 同步 MCP 配置到 Claude 原生文件
fn sync_mcp_to_claude_config() -> Result<(), String> {
    let aite_config = read_aite_config();

    // 获取所有启用了 claude 的服务器
    let mut mcp_servers: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();

    if let Some(mcp) = &aite_config.mcp {
        if let Some(servers) = &mcp.servers {
            for (id, server) in servers.iter() {
                if server.apps.claude {
                    // 将 McpServerSpec 转换为 JSON
                    let spec =
                        serde_json::to_value(&server.server_spec).map_err(|e| e.to_string())?;
                    mcp_servers.insert(id.clone(), spec);
                }
            }
        }
    }

    write_scoped_mcp_servers(None, mcp_servers)
}
