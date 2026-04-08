use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::ClientRequestBuilder, Message as WsMessage},
};
use url::Url;

const IDE_STATE_EVENT: &str = "ide:state_changed";
const IDE_SELECTION_EVENT: &str = "ide:selection_changed";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IDEConnectionInfo {
    pub key: String,
    pub url: String,
    pub name: String,
    pub workspace_folders: Vec<String>,
    pub port: u16,
    pub is_valid: bool,
    pub auth_token: Option<String>,
    pub ide_running_in_windows: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IDESelection {
    pub file_path: Option<String>,
    pub text: Option<String>,
    pub line_count: Option<u32>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IDEConnectionState {
    pub status: String,
    pub connected_ide: Option<IDEConnectionInfo>,
    pub selection: Option<IDESelection>,
    pub error: Option<String>,
}

impl Default for IDEConnectionState {
    fn default() -> Self {
        Self {
            status: "disconnected".to_string(),
            connected_ide: None,
            selection: None,
            error: None,
        }
    }
}

struct ActiveIDEConnection {
    abort_tx: watch::Sender<bool>,
    handle: JoinHandle<()>,
}

#[derive(Default)]
struct IDEManagerInner {
    state: IDEConnectionState,
    active: Option<ActiveIDEConnection>,
}

struct IDEManager {
    inner: Mutex<IDEManagerInner>,
}

impl IDEManager {
    fn new() -> Self {
        Self {
            inner: Mutex::new(IDEManagerInner::default()),
        }
    }

    async fn current_state(&self) -> IDEConnectionState {
        self.inner.lock().await.state.clone()
    }

    async fn set_state(&self, app: &AppHandle, next_state: IDEConnectionState) {
        {
            let mut inner = self.inner.lock().await;
            inner.state = next_state.clone();
        }
        let _ = app.emit(IDE_STATE_EVENT, next_state);
    }

    async fn set_selection(&self, app: &AppHandle, selection: Option<IDESelection>) {
        {
            let mut inner = self.inner.lock().await;
            inner.state.selection = selection.clone();
        }
        let _ = app.emit(IDE_SELECTION_EVENT, selection.clone());
        let _ = app.emit(IDE_STATE_EVENT, self.current_state().await);
    }

    async fn connect(
        self: &Arc<Self>,
        app: AppHandle,
        ide: IDEConnectionInfo,
    ) -> Result<IDEConnectionState, String> {
        self.disconnect_internal().await;

        self.set_state(
            &app,
            IDEConnectionState {
                status: "connecting".to_string(),
                connected_ide: Some(ide.clone()),
                selection: None,
                error: None,
            },
        )
        .await;

        let (abort_tx, abort_rx) = watch::channel(false);
        let manager = Arc::clone(self);
        let ide_for_task = ide.clone();
        let app_for_task = app.clone();
        let handle = tokio::spawn(async move {
            let result = if ide_for_task.url.starts_with("ws://")
                || ide_for_task.url.starts_with("wss://")
            {
                run_ws_connection(
                    manager.clone(),
                    app_for_task.clone(),
                    ide_for_task.clone(),
                    abort_rx,
                )
                .await
            } else {
                run_sse_connection(
                    manager.clone(),
                    app_for_task.clone(),
                    ide_for_task.clone(),
                    abort_rx,
                )
                .await
            };

            if let Err(error) = result {
                manager
                    .set_state(
                        &app_for_task,
                        IDEConnectionState {
                            status: "error".to_string(),
                            connected_ide: Some(ide_for_task),
                            selection: None,
                            error: Some(error),
                        },
                    )
                    .await;
            }
        });

        {
            let mut inner = self.inner.lock().await;
            inner.active = Some(ActiveIDEConnection { abort_tx, handle });
        }

        Ok(self.current_state().await)
    }

    async fn disconnect(self: &Arc<Self>, app: Option<AppHandle>) -> Result<bool, String> {
        self.disconnect_internal().await;

        let next_state = IDEConnectionState::default();
        if let Some(app_handle) = app {
            self.set_state(&app_handle, next_state).await;
        } else {
            let mut inner = self.inner.lock().await;
            inner.state = next_state;
        }

        Ok(true)
    }

    async fn disconnect_internal(&self) {
        let active = {
            let mut inner = self.inner.lock().await;
            inner.active.take()
        };

        if let Some(active_connection) = active {
            let _ = active_connection.abort_tx.send(true);
            active_connection.handle.abort();
        }
    }
}

static GLOBAL_IDE_MANAGER: once_cell::sync::Lazy<Arc<IDEManager>> =
    once_cell::sync::Lazy::new(|| Arc::new(IDEManager::new()));

fn get_ide_manager() -> Arc<IDEManager> {
    GLOBAL_IDE_MANAGER.clone()
}

fn normalize_path(path_str: &str) -> String {
    std::fs::canonicalize(path_str)
        .unwrap_or_else(|_| PathBuf::from(path_str))
        .to_string_lossy()
        .to_string()
}

fn is_directory_match(current_dir: &str, workspace_folder: &str) -> bool {
    let mut current = normalize_path(current_dir);
    let mut folder = normalize_path(workspace_folder);

    if cfg!(target_os = "windows") {
        current = current.to_lowercase();
        folder = folder.to_lowercase();
    }

    current == folder || current.starts_with(&(folder + std::path::MAIN_SEPARATOR_STR))
}

fn get_ide_lock_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();

    if let Some(home_dir) = dirs::home_dir() {
        let primary = home_dir.join(".claude").join("ide");
        if primary.exists() {
            directories.push(primary);
        }
    }

    directories
}

fn parse_ide_lock_file(lock_path: &Path) -> Option<IDEConnectionInfo> {
    let file_name = lock_path.file_name()?.to_string_lossy().to_string();
    let port = file_name.strip_suffix(".lock")?.parse::<u16>().ok()?;
    let raw = std::fs::read_to_string(lock_path).ok()?;

    let mut workspace_folders = Vec::new();
    let mut ide_name = "IDE".to_string();
    let mut use_web_socket = false;
    let mut auth_token = None;
    let mut ide_running_in_windows = None;

    if let Ok(json) = serde_json::from_str::<Value>(&raw) {
        if let Some(folders) = json.get("workspaceFolders").and_then(Value::as_array) {
            workspace_folders = folders
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect();
        }

        ide_name = json
            .get("ideName")
            .and_then(Value::as_str)
            .unwrap_or("IDE")
            .to_string();
        use_web_socket = json
            .get("transport")
            .and_then(Value::as_str)
            .map(|transport| transport == "ws")
            .unwrap_or(false);
        auth_token = json
            .get("authToken")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        ide_running_in_windows = json.get("runningInWindows").and_then(Value::as_bool);
    }

    if workspace_folders.is_empty() {
        workspace_folders = raw
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string)
            .collect();
    }

    let url = if use_web_socket {
        format!("ws://127.0.0.1:{port}")
    } else {
        format!("http://127.0.0.1:{port}/sse")
    };

    Some(IDEConnectionInfo {
        key: url.clone(),
        url,
        name: ide_name,
        workspace_folders,
        port,
        is_valid: false,
        auth_token,
        ide_running_in_windows,
    })
}

fn collect_lock_files() -> Vec<PathBuf> {
    let mut lock_files = Vec::new();

    for directory in get_ide_lock_directories() {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("lock") {
                lock_files.push(path);
            }
        }
    }

    lock_files.sort_by(|left, right| {
        let left_modified = std::fs::metadata(left)
            .and_then(|meta| meta.modified())
            .ok();
        let right_modified = std::fs::metadata(right)
            .and_then(|meta| meta.modified())
            .ok();
        right_modified.cmp(&left_modified)
    });

    lock_files
}

#[tauri::command]
pub fn detect_running_ides(
    current_dir: Option<String>,
    return_all: Option<bool>,
) -> Result<Vec<IDEConnectionInfo>, String> {
    let requested_dir = current_dir.unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .to_string_lossy()
            .to_string()
    });
    let include_all = return_all.unwrap_or(false);

    let mut detected = Vec::new();

    for lock_file in collect_lock_files() {
        let Some(mut ide) = parse_ide_lock_file(&lock_file) else {
            continue;
        };

        ide.is_valid = ide
            .workspace_folders
            .iter()
            .any(|workspace| is_directory_match(&requested_dir, workspace));

        if ide.is_valid || include_all {
            detected.push(ide);
        }
    }

    Ok(detected)
}

#[tauri::command]
pub async fn connect_ide(
    ide: IDEConnectionInfo,
    app: AppHandle,
) -> Result<IDEConnectionState, String> {
    get_ide_manager().connect(app, ide).await
}

#[tauri::command]
pub async fn disconnect_ide(app: AppHandle) -> Result<bool, String> {
    get_ide_manager().disconnect(Some(app)).await
}

#[tauri::command]
pub async fn get_ide_connection_state() -> Result<IDEConnectionState, String> {
    Ok(get_ide_manager().current_state().await)
}

async fn run_ws_connection(
    manager: Arc<IDEManager>,
    app: AppHandle,
    ide: IDEConnectionInfo,
    mut abort_rx: watch::Receiver<bool>,
) -> Result<(), String> {
    let mut request_builder = ClientRequestBuilder::new(
        ide.url
            .parse()
            .map_err(|error| format!("无效的 IDE WebSocket 地址: {error}"))?,
    )
    .with_header("Origin", format!("http://localhost:{}", ide.port))
    .with_header("User-Agent", "claude-code-cli")
    .with_sub_protocol("mcp");

    if let Some(token) = ide.auth_token.as_deref() {
        request_builder = request_builder.with_header("X-Claude-Code-Ide-Authorization", token);
    }

    let (mut socket, _) = connect_async(request_builder)
        .await
        .map_err(|error| format!("连接 IDE WebSocket 失败: {error}"))?;

    manager
        .set_state(
            &app,
            IDEConnectionState {
                status: "connected".to_string(),
                connected_ide: Some(ide.clone()),
                selection: None,
                error: None,
            },
        )
        .await;

    send_jsonrpc_notification_ws(
        &mut socket,
        "ide_connected",
        serde_json::json!({ "client": "aite" }),
    )
    .await?;

    loop {
        tokio::select! {
            changed = abort_rx.changed() => {
                if changed.is_ok() && *abort_rx.borrow() {
                    break;
                }
            }
            message = socket.next() => {
                match message {
                    Some(Ok(WsMessage::Text(text))) => {
                        handle_ide_json_message(manager.clone(), &app, &ide, &text).await;
                    }
                    Some(Ok(WsMessage::Binary(bytes))) => {
                        if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                            handle_ide_json_message(manager.clone(), &app, &ide, &text).await;
                        }
                    }
                    Some(Ok(WsMessage::Ping(payload))) => {
                        let _ = socket.send(WsMessage::Pong(payload)).await;
                    }
                    Some(Ok(WsMessage::Close(_))) => {
                        break;
                    }
                    Some(Err(error)) => {
                        return Err(format!("IDE WebSocket 已断开: {error}"));
                    }
                    None => break,
                    _ => {}
                }
            }
        }
    }

    manager
        .set_state(
            &app,
            IDEConnectionState {
                status: "disconnected".to_string(),
                connected_ide: None,
                selection: None,
                error: None,
            },
        )
        .await;

    Ok(())
}

async fn run_sse_connection(
    manager: Arc<IDEManager>,
    app: AppHandle,
    ide: IDEConnectionInfo,
    mut abort_rx: watch::Receiver<bool>,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let mut request = client
        .get(&ide.url)
        .header("Accept", "text/event-stream")
        .header("User-Agent", "aite");

    if let Some(token) = ide.auth_token.as_deref() {
        request = request.header("X-Claude-Code-Ide-Authorization", token);
    }

    let response = request
        .send()
        .await
        .map_err(|error| format!("连接 IDE SSE 失败: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("IDE SSE 返回异常状态: {}", response.status()));
    }

    manager
        .set_state(
            &app,
            IDEConnectionState {
                status: "connected".to_string(),
                connected_ide: Some(ide.clone()),
                selection: None,
                error: None,
            },
        )
        .await;

    let session_id = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string);
    let post_url = Arc::new(Mutex::new(None::<String>));
    let post_url_for_send = Arc::clone(&post_url);
    let client_for_send = client.clone();
    let ide_for_send = ide.clone();

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
        let target_url = {
            let current = post_url_for_send.lock().await;
            current.clone().unwrap_or_else(|| ide_for_send.url.clone())
        };
        let _ = send_jsonrpc_notification_sse(
            &client_for_send,
            &target_url,
            session_id.as_deref(),
            ide_for_send.auth_token.as_deref(),
            "ide_connected",
            serde_json::json!({ "client": "aite" }),
        )
        .await;
    });

    let mut buffer = String::new();
    let mut stream = response.bytes_stream();

    loop {
        tokio::select! {
            changed = abort_rx.changed() => {
                if changed.is_ok() && *abort_rx.borrow() {
                    break;
                }
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(event) = extract_sse_event(&mut buffer) {
                            handle_sse_event(manager.clone(), &app, &ide, &post_url, &event).await;
                        }
                    }
                    Some(Err(error)) => return Err(format!("读取 IDE SSE 事件失败: {error}")),
                    None => break,
                }
            }
        }
    }

    manager
        .set_state(
            &app,
            IDEConnectionState {
                status: "disconnected".to_string(),
                connected_ide: None,
                selection: None,
                error: None,
            },
        )
        .await;

    Ok(())
}

async fn send_jsonrpc_notification_ws(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    method: &str,
    params: Value,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });

    socket
        .send(WsMessage::Text(payload.to_string().into()))
        .await
        .map_err(|error| format!("发送 IDE 通知失败: {error}"))
}

async fn send_jsonrpc_notification_sse(
    client: &reqwest::Client,
    target_url: &str,
    session_id: Option<&str>,
    auth_token: Option<&str>,
    method: &str,
    params: Value,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });

    let mut request = client
        .post(target_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("User-Agent", "aite")
        .body(payload.to_string());

    if let Some(value) = session_id {
        request = request.header("mcp-session-id", value);
    }
    if let Some(value) = auth_token {
        request = request.header("X-Claude-Code-Ide-Authorization", value);
    }

    request
        .send()
        .await
        .map_err(|error| format!("发送 IDE SSE 通知失败: {error}"))?;
    Ok(())
}

fn extract_sse_event(buffer: &mut String) -> Option<String> {
    let newline_index = buffer.find("\n\n");
    let windows_index = buffer.find("\r\n\r\n");

    let (index, separator_len) = match (newline_index, windows_index) {
        (Some(unix), Some(windows)) if windows < unix => (windows, 4),
        (Some(unix), _) => (unix, 2),
        (_, Some(windows)) => (windows, 4),
        _ => return None,
    };

    let event = buffer[..index].to_string();
    *buffer = buffer[index + separator_len..].to_string();
    Some(event)
}

async fn handle_sse_event(
    manager: Arc<IDEManager>,
    app: &AppHandle,
    ide: &IDEConnectionInfo,
    post_url: &Arc<Mutex<Option<String>>>,
    raw_event: &str,
) {
    let mut event_name = "message".to_string();
    let mut data_lines = Vec::new();

    for line in raw_event.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event_name = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim_start().to_string());
        }
    }

    let data = data_lines.join("\n");
    if data.is_empty() {
        return;
    }

    if event_name == "endpoint" {
        if let Ok(url) = Url::parse(&ide.url).and_then(|base| base.join(&data)) {
            let mut current = post_url.lock().await;
            *current = Some(url.to_string());
        }
        return;
    }

    handle_ide_json_message(manager, app, ide, &data).await;
}

async fn handle_ide_json_message(
    manager: Arc<IDEManager>,
    app: &AppHandle,
    ide: &IDEConnectionInfo,
    payload: &str,
) {
    let Ok(json) = serde_json::from_str::<Value>(payload) else {
        return;
    };

    if let Some(items) = json.as_array() {
        for item in items {
            handle_ide_json_value(manager.clone(), app, ide, item).await;
        }
        return;
    }

    handle_ide_json_value(manager, app, ide, &json).await;
}

async fn handle_ide_json_value(
    manager: Arc<IDEManager>,
    app: &AppHandle,
    ide: &IDEConnectionInfo,
    json: &Value,
) {
    let Some(method) = json.get("method").and_then(Value::as_str) else {
        return;
    };
    let params = json.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "selection_changed" => {
            let selection = normalize_selection(&params);
            manager.set_selection(app, Some(selection)).await;
        }
        "ide_connected" => {
            manager
                .set_state(
                    app,
                    IDEConnectionState {
                        status: "connected".to_string(),
                        connected_ide: Some(ide.clone()),
                        selection: manager.current_state().await.selection,
                        error: None,
                    },
                )
                .await;
        }
        "ide_disconnected" => {
            manager
                .set_state(
                    app,
                    IDEConnectionState {
                        status: "disconnected".to_string(),
                        connected_ide: None,
                        selection: None,
                        error: None,
                    },
                )
                .await;
        }
        _ => {}
    }
}

fn normalize_selection(params: &Value) -> IDESelection {
    let file_path = params
        .get("filePath")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            params
                .get("currentFile")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        });

    let text = params
        .get("text")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    let start_line = params
        .get("startLine")
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .or_else(|| {
            params
                .get("selection")
                .and_then(|selection| selection.get("start"))
                .and_then(|start| start.get("line"))
                .and_then(Value::as_u64)
                .map(|value| value as u32 + 1)
        });

    let end_line = params
        .get("endLine")
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .or_else(|| {
            params
                .get("selection")
                .and_then(|selection| selection.get("end"))
                .and_then(|end| end.get("line"))
                .and_then(Value::as_u64)
                .map(|value| value as u32 + 1)
        });

    let line_count = params
        .get("lineCount")
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .or_else(|| {
            text.as_ref()
                .map(|selected_text| selected_text.lines().count().max(1) as u32)
        })
        .or_else(|| match (start_line, end_line) {
            (Some(start), Some(end)) if end >= start => Some(end - start + 1),
            _ => None,
        });

    IDESelection {
        file_path,
        text,
        line_count,
        start_line,
        end_line,
    }
}
