// Claude CLI detection and management commands
// Claude CLI 检测和管理命令

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter};
use tokio::process::Command as TokioCommand;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(target_os = "windows")]
const GIT_PORTABLE_VERSION: &str = "2.47.1.2";
#[cfg(target_os = "windows")]
const GIT_RELEASE_TAG: &str = "v2.47.1.windows.2";
#[cfg(target_os = "windows")]
const GIT_DIST_GITHUB: &str = "https://github.com/git-for-windows/git/releases/download";
#[cfg(target_os = "windows")]
const GIT_DIST_NPMMIRROR: &str = "https://registry.npmmirror.com/-/binary/git-for-windows";
#[cfg(target_os = "windows")]
const GIT_DIST_HUAWEI: &str = "https://mirrors.huaweicloud.com/git-for-windows";
const SETUP_PROGRESS_EVENT: &str = "setup:download:progress";
const NODE_LTS_VERSION: &str = "v22.22.0";
const NODE_DIST_OFFICIAL: &str = "https://nodejs.org/dist";
const NODE_DIST_NPMMIRROR: &str = "https://cdn.npmmirror.com/binaries/node";
const NODE_DIST_HUAWEI: &str = "https://mirrors.huaweicloud.com/nodejs";

/// CLI 状态信息
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CliStatus {
    /// 是否已安装
    pub installed: bool,
    /// CLI 可执行文件路径
    pub path: Option<String>,
    /// CLI 版本号
    pub version: Option<String>,
    /// Windows 下 Git Bash 是否缺失
    pub git_bash_missing: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeEnvStatus {
    pub node_available: bool,
    pub node_version: Option<String>,
    pub node_source: Option<String>,
    pub npm_available: bool,
}

/// 清理 ANSI 转义序列
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                Some('[') => {
                    chars.next();
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ('\x40'..='\x7e').contains(&ch) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                Some('(' | ')') => {
                    chars.next();
                    chars.next();
                }
                _ => {
                    chars.next();
                }
            }
        } else if c < '\x20' && c != '\n' && c != '\r' && c != '\t' {
            // 跳过控制字符
        } else {
            out.push(c);
        }
    }
    out
}

/// 检查是否为有效的可执行文件
fn is_valid_executable(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }

    #[cfg(target_os = "windows")]
    {
        // Windows: 检查文件扩展名
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if ext == "exe" || ext == "cmd" || ext == "bat" || ext == "ps1" {
                return true;
            }
        }
        false
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Unix/macOS: 检查执行权限
        path.metadata()
            .map(|m| m.permissions().readonly())
            .unwrap_or(false)
            || path
                .metadata()
                .map(|m| !m.permissions().readonly())
                .unwrap_or(false)
    }
}

#[cfg(target_os = "windows")]
fn get_local_git_dir() -> Result<std::path::PathBuf, String> {
    Ok(get_local_app_dir()?.join("git"))
}

#[cfg(target_os = "windows")]
fn get_local_git_bash() -> Option<String> {
    let git_dir = get_local_git_dir().ok()?;
    let bash = git_dir.join("bin").join("bash.exe");
    if bash.exists() {
        Some(bash.to_string_lossy().to_string())
    } else {
        None
    }
}

fn get_local_app_dir() -> Result<std::path::PathBuf, String> {
    dirs::data_local_dir()
        .map(|dir| dir.join("Aite"))
        .ok_or_else(|| "无法确定本地应用数据目录".to_string())
}

fn get_local_cli_prefix_dir() -> Result<std::path::PathBuf, String> {
    Ok(get_local_app_dir()?.join("npm-global"))
}

fn get_local_cli_cache_dir() -> Result<std::path::PathBuf, String> {
    Ok(get_local_app_dir()?.join("npm-cache"))
}

fn get_local_node_dir() -> Result<std::path::PathBuf, String> {
    Ok(get_local_app_dir()?.join("node"))
}

pub(crate) fn get_local_node_executable_path() -> Option<PathBuf> {
    let node_bin_dir = get_local_node_bin_dir()?;
    #[cfg(target_os = "windows")]
    {
        Some(node_bin_dir.join("node.exe"))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Some(node_bin_dir.join("node"))
    }
}

fn get_local_npm_executable_path() -> Option<PathBuf> {
    let node_bin_dir = get_local_node_bin_dir()?;
    #[cfg(target_os = "windows")]
    {
        Some(node_bin_dir.join("npm.cmd"))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Some(node_bin_dir.join("npm"))
    }
}

fn get_local_cli_bin_path() -> Result<std::path::PathBuf, String> {
    let prefix = get_local_cli_prefix_dir()?;
    #[cfg(target_os = "windows")]
    {
        Ok(prefix.join("claude.cmd"))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(prefix.join("bin").join("claude"))
    }
}

fn get_local_cli_bin_dir() -> Option<PathBuf> {
    let prefix = get_local_cli_prefix_dir().ok()?;
    #[cfg(target_os = "windows")]
    {
        Some(prefix)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Some(prefix.join("bin"))
    }
}

fn get_local_node_bin_dir() -> Option<PathBuf> {
    let app_dir = get_local_app_dir().ok()?;
    #[cfg(target_os = "windows")]
    {
        let node_dir = app_dir.join("node");
        if node_dir.join("node.exe").exists() {
            Some(node_dir)
        } else {
            None
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let node_bin_dir = app_dir.join("node").join("bin");
        if node_bin_dir.join("node").exists() {
            Some(node_bin_dir)
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn login_shell_extra_path() -> &'static str {
    static CACHE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let output = Command::new(&shell)
            .args(["-l", "-c", "echo $PATH"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        match output {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            _ => String::new(),
        }
    })
}

fn push_unique_path(
    paths: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
    candidate: impl Into<String>,
) {
    let candidate = candidate.into();
    if candidate.is_empty() || !seen.insert(candidate.clone()) {
        return;
    }
    paths.push(candidate);
}

pub(crate) fn build_enriched_path() -> String {
    let current_path = std::env::var("PATH").unwrap_or_default();

    #[cfg(target_os = "windows")]
    let separator = ";";
    #[cfg(not(target_os = "windows"))]
    let separator = ":";

    let mut paths = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(path) = get_local_cli_bin_dir() {
        push_unique_path(&mut paths, &mut seen, path.to_string_lossy().to_string());
    }

    if let Some(path) = get_local_node_bin_dir() {
        push_unique_path(&mut paths, &mut seen, path.to_string_lossy().to_string());
    }

    if let Some(home) = dirs::home_dir() {
        #[cfg(target_os = "windows")]
        {
            if let Some(app_data) = dirs::data_dir() {
                push_unique_path(
                    &mut paths,
                    &mut seen,
                    app_data.join("npm").to_string_lossy().to_string(),
                );
            }

            push_unique_path(
                &mut paths,
                &mut seen,
                home.join("scoop")
                    .join("shims")
                    .to_string_lossy()
                    .to_string(),
            );
            push_unique_path(
                &mut paths,
                &mut seen,
                home.join(".volta")
                    .join("bin")
                    .to_string_lossy()
                    .to_string(),
            );
            push_unique_path(
                &mut paths,
                &mut seen,
                home.join(".fnm")
                    .join("aliases")
                    .join("default")
                    .to_string_lossy()
                    .to_string(),
            );

            if let Ok(program_files) = std::env::var("ProgramFiles") {
                push_unique_path(&mut paths, &mut seen, format!("{}\\nodejs", program_files));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            push_unique_path(
                &mut paths,
                &mut seen,
                home.join(".cargo")
                    .join("bin")
                    .to_string_lossy()
                    .to_string(),
            );
            push_unique_path(
                &mut paths,
                &mut seen,
                home.join(".local")
                    .join("bin")
                    .to_string_lossy()
                    .to_string(),
            );
            push_unique_path(
                &mut paths,
                &mut seen,
                home.join(".npm-global")
                    .join("bin")
                    .to_string_lossy()
                    .to_string(),
            );
            push_unique_path(
                &mut paths,
                &mut seen,
                home.join(".volta")
                    .join("bin")
                    .to_string_lossy()
                    .to_string(),
            );
            push_unique_path(
                &mut paths,
                &mut seen,
                home.join(".fnm")
                    .join("aliases")
                    .join("default")
                    .join("bin")
                    .to_string_lossy()
                    .to_string(),
            );

            let nvm_versions_dir = home.join(".nvm").join("versions").join("node");
            if let Ok(entries) = std::fs::read_dir(&nvm_versions_dir) {
                let mut version_dirs: Vec<PathBuf> = entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.is_dir())
                    .collect();
                version_dirs.sort();
                if let Some(latest) = version_dirs.last() {
                    push_unique_path(
                        &mut paths,
                        &mut seen,
                        latest.join("bin").to_string_lossy().to_string(),
                    );
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        push_unique_path(&mut paths, &mut seen, "/opt/homebrew/bin".to_string());
        push_unique_path(&mut paths, &mut seen, "/usr/local/bin".to_string());

        let shell_path = login_shell_extra_path();
        for entry in shell_path.split(':').filter(|entry| !entry.is_empty()) {
            push_unique_path(&mut paths, &mut seen, entry.to_string());
        }
    }

    for entry in current_path
        .split(separator)
        .filter(|entry| !entry.is_empty())
    {
        push_unique_path(&mut paths, &mut seen, entry.to_string());
    }

    paths.join(separator)
}

fn claude_binary_requires_node(path: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_ascii_lowercase();
            if matches!(ext.as_str(), "cmd" | "bat" | "ps1") {
                return true;
            }
        }
    }

    if let Ok(content) = std::fs::read(path) {
        let prefix = &content[..content.len().min(256)];
        let text = String::from_utf8_lossy(prefix);
        return text.starts_with("#!") && text.contains("node");
    }

    false
}

fn find_node_binary_in_path(path_env: &str) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("cmd")
            .args(["/C", "where", "node"])
            .env("PATH", path_env)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        return output
            .stdout
            .split(|byte| *byte == b'\n')
            .map(|line| String::from_utf8_lossy(line).trim().to_string())
            .find(|line| !line.is_empty());
    }

    #[cfg(not(target_os = "windows"))]
    {
        let output = Command::new("sh")
            .args(["-c", "command -v node"])
            .env("PATH", path_env)
            .stdin(Stdio::null())
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(path)
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ClaudeRuntimeEnv {
    pub path: String,
    pub node_path: Option<String>,
}

pub(crate) fn resolve_claude_runtime_env(cli_path: &Path) -> Result<ClaudeRuntimeEnv, String> {
    let path = build_enriched_path();
    let requires_node = claude_binary_requires_node(cli_path);
    let node_path = if requires_node {
        Some(find_node_binary_in_path(&path).ok_or_else(|| {
            format!(
                "检测到 Claude CLI 依赖 Node.js，但当前应用运行环境中未找到 node。请确认 Node.js 已安装，或确保应用启动时能够继承终端中的 PATH。Claude CLI: {}",
                cli_path.display()
            )
        })?)
    } else {
        None
    };

    tracing::info!(
        "Claude CLI 运行环境: cli={}, requires_node={}, runtime_path={}",
        cli_path.display(),
        requires_node,
        path
    );
    if let Some(node_path) = &node_path {
        tracing::info!("Claude CLI 运行环境已解析 Node.js: {}", node_path);
    }
    Ok(ClaudeRuntimeEnv { path, node_path })
}

pub(crate) fn resolve_claude_binary_path() -> Result<std::path::PathBuf, String> {
    find_claude_binary()
        .map(std::path::PathBuf::from)
        .ok_or_else(|| {
            "❌ Claude Code CLI 未找到。请先在启动向导中安装，或手动执行 npm install -g @anthropic-ai/claude-code"
                .to_string()
        })
}

/// 查找 Claude CLI 二进制文件
fn find_claude_binary() -> Option<String> {
    tracing::info!("开始定位 Claude CLI 可执行文件");

    // 1. 检查系统 PATH 中的 claude 命令
    #[cfg(target_os = "windows")]
    {
        // Windows: 使用 `where` 命令查找
        // 优先查找 .cmd 文件，避免无扩展名的 JS 脚本导致错误
        for query in ["claude.cmd", "claude"] {
            if let Ok(output) = Command::new("cmd")
                .args(["/C", "where", query])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .output()
            {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines() {
                        let path = line.trim().to_string();
                        if path.is_empty() {
                            continue;
                        }
                        let p = Path::new(&path);
                        // 跳过无扩展名的文件（JS 脚本）
                        if p.extension().is_none() {
                            continue;
                        }
                        if is_valid_executable(p) {
                            tracing::info!("在 PATH 中找到 Claude CLI: {}", path);
                            return Some(path);
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(output) = Command::new("sh")
            .args(["-l", "-c", "which claude"])
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() && is_valid_executable(Path::new(&path)) {
                    tracing::info!("在 PATH 中找到 Claude CLI: {}", path);
                    return Some(path);
                }
            }
        }
    }

    // 2. 检查 Claude 桌面应用捆绑的 CLI
    if let Some(home) = dirs::home_dir() {
        if let Ok(local_cli) = get_local_cli_bin_path() {
            if is_valid_executable(&local_cli) {
                tracing::info!("在应用本地目录找到 Claude CLI: {}", local_cli.display());
                return Some(local_cli.to_string_lossy().to_string());
            }
        }

        #[cfg(target_os = "windows")]
        {
            // %LOCALAPPDATA%\Claude\claude-code\*\claude.exe
            if let Some(local_app) = dirs::data_local_dir() {
                let claude_code_dir = local_app.join("Claude").join("claude-code");
                if let Some(bin) = find_newest_version_bin(&claude_code_dir, "claude.exe") {
                    tracing::info!("在桌面应用中找到 Claude CLI: {}", bin);
                    return Some(bin);
                }
            }
            // %APPDATA%\Claude\claude-code\*\claude.exe
            if let Some(app_data) = dirs::data_dir() {
                let claude_code_dir = app_data.join("Claude").join("claude-code");
                if let Some(bin) = find_newest_version_bin(&claude_code_dir, "claude.exe") {
                    tracing::info!("在桌面应用中找到 Claude CLI: {}", bin);
                    return Some(bin);
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            let claude_code_dir = home.join("Library/Application Support/Claude/claude-code");
            if let Some(bin) = find_newest_version_bin(&claude_code_dir, "claude") {
                tracing::info!("在桌面应用中找到 Claude CLI: {}", bin);
                return Some(bin);
            }
        }

        #[cfg(target_os = "linux")]
        {
            if let Some(data_dir) = dirs::data_dir() {
                let claude_code_dir = data_dir.join("Claude").join("claude-code");
                if let Some(bin) = find_newest_version_bin(&claude_code_dir, "claude") {
                    tracing::info!("在桌面应用中找到 Claude CLI: {}", bin);
                    return Some(bin);
                }
            }
        }

        // 3. 检查常见的全局安装路径
        #[cfg(target_os = "windows")]
        {
            // npm global: %APPDATA%\npm\claude.cmd
            if let Some(app_data) = dirs::data_dir() {
                for name in ["claude.cmd", "claude.exe", "claude.ps1"] {
                    let candidate = app_data.join("npm").join(name);
                    if is_valid_executable(&candidate) {
                        tracing::info!("在 npm 全局目录找到 Claude CLI: {}", candidate.display());
                        return Some(candidate.to_string_lossy().to_string());
                    }
                }
            }
            // Scoop: ~/scoop/shims/claude.cmd
            let scoop_candidate = home.join("scoop").join("shims").join("claude.cmd");
            if is_valid_executable(&scoop_candidate) {
                tracing::info!(
                    "在 Scoop shims 找到 Claude CLI: {}",
                    scoop_candidate.display()
                );
                return Some(scoop_candidate.to_string_lossy().to_string());
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            for candidate in [
                home.join(".npm-global/bin/claude"),
                home.join(".local/bin/claude"),
            ] {
                if is_valid_executable(&candidate) {
                    tracing::info!("在用户本地 bin 找到 Claude CLI: {}", candidate.display());
                    return Some(candidate.to_string_lossy().to_string());
                }
            }
        }
    }

    // 4. 检查系统级路径
    #[cfg(not(target_os = "windows"))]
    {
        for candidate in ["/usr/local/bin/claude", "/opt/homebrew/bin/claude"] {
            if is_valid_executable(Path::new(candidate)) {
                tracing::info!("在系统路径找到 Claude CLI: {}", candidate);
                return Some(candidate.to_string());
            }
        }
    }

    tracing::warn!("Claude CLI 可执行文件定位失败");
    None
}

/// 在版本目录中查找最新版本的可执行文件
fn find_newest_version_bin(base_dir: &Path, bin_name: &str) -> Option<String> {
    if !base_dir.exists() {
        return None;
    }

    if let Ok(entries) = std::fs::read_dir(base_dir) {
        let mut versions: Vec<_> = entries.flatten().filter(|e| e.path().is_dir()).collect();

        // 按语义版本排序（降序）
        versions.sort_by(|a, b| {
            let parse = |name: &std::ffi::OsStr| -> Vec<u64> {
                name.to_string_lossy()
                    .split('.')
                    .filter_map(|s| s.parse::<u64>().ok())
                    .collect()
            };
            parse(&b.file_name()).cmp(&parse(&a.file_name()))
        });

        for entry in &versions {
            let bin = entry.path().join(bin_name);
            if is_valid_executable(&bin) {
                return Some(bin.to_string_lossy().to_string());
            }
        }
    }

    None
}

/// 检查 Claude CLI 是否已安装并返回其路径和版本
#[tauri::command]
pub fn check_claude_cli() -> Result<CliStatus, String> {
    tracing::info!("开始检查 Claude CLI...");

    let binary = find_claude_binary();

    match binary {
        Some(path) => {
            tracing::info!("找到 Claude CLI: {}", path);

            let runtime_env = match resolve_claude_runtime_env(Path::new(&path)) {
                Ok(runtime_env) => runtime_env,
                Err(error) => {
                    tracing::warn!("Claude CLI 运行环境预检失败: {}", error);

                    #[cfg(target_os = "windows")]
                    let git_bash_missing = find_git_bash().is_none();
                    #[cfg(not(target_os = "windows"))]
                    let git_bash_missing = false;

                    return Ok(CliStatus {
                        installed: false,
                        path: Some(path),
                        version: None,
                        git_bash_missing,
                    });
                }
            };

            if let Some(node_path) = &runtime_env.node_path {
                tracing::info!("Claude CLI 运行时 Node.js: {}", node_path);
            }

            // 尝试获取版本
            let version = match get_claude_version(&path) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("获取 Claude CLI 版本失败: {}", e);

                    #[cfg(target_os = "windows")]
                    let git_bash_missing = find_git_bash().is_none();
                    #[cfg(not(target_os = "windows"))]
                    let git_bash_missing = false;

                    return Ok(CliStatus {
                        installed: false,
                        path: Some(path),
                        version: None,
                        git_bash_missing,
                    });
                }
            };

            // Windows 下检查 Git Bash
            #[cfg(target_os = "windows")]
            let git_bash_missing = find_git_bash().is_none();
            #[cfg(not(target_os = "windows"))]
            let git_bash_missing = false;

            Ok(CliStatus {
                installed: true,
                path: Some(path),
                version,
                git_bash_missing,
            })
        }
        None => {
            tracing::info!("未找到 Claude CLI");
            Ok(CliStatus {
                installed: false,
                path: None,
                version: None,
                git_bash_missing: false,
            })
        }
    }
}

/// 获取 Claude CLI 版本
fn get_claude_version(path: &str) -> Result<String, String> {
    tracing::info!("执行 {} --version...", path);
    let runtime_env = resolve_claude_runtime_env(Path::new(path))?;

    #[cfg(target_os = "windows")]
    let output_result = {
        let needs_cmd = path.ends_with(".cmd") || path.ends_with(".bat");
        if needs_cmd {
            Command::new("cmd")
                .args(["/C", path, "--version"])
                .env("PATH", &runtime_env.path)
                .creation_flags(CREATE_NO_WINDOW)
                .output()
        } else {
            Command::new(path)
                .arg("--version")
                .env("PATH", &runtime_env.path)
                .creation_flags(CREATE_NO_WINDOW)
                .output()
        }
    };

    #[cfg(not(target_os = "windows"))]
    let output_result = Command::new(path)
        .arg("--version")
        .env("PATH", &runtime_env.path)
        .output();

    match output_result {
        Ok(output) if output.status.success() => {
            let raw = strip_ansi(&String::from_utf8_lossy(&output.stdout));
            let version = raw.trim().to_string();
            tracing::info!("Claude CLI 版本: {}", version);
            Ok(version)
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!("claude --version 执行失败: {}", stderr);
            Err(format!("命令执行失败: {}", stderr))
        }
        Err(e) => {
            tracing::error!("claude --version 执行出错: {}", e);
            Err(format!("命令执行出错: {}", e))
        }
    }
}

/// Windows 下查找 Git Bash
#[cfg(target_os = "windows")]
pub(crate) fn find_git_bash() -> Option<String> {
    if let Some(local) = get_local_git_bash() {
        return Some(local);
    }

    let common_paths = [
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files\Git\usr\bin\bash.exe",
        r"C:\Program Files (x86)\Git\bin\bash.exe",
    ];

    for path in common_paths {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    // 检查 PATH
    if let Ok(output) = Command::new("cmd")
        .args(["/C", "where", "bash.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = stdout.lines().next() {
                return Some(line.trim().to_string());
            }
        }
    }

    None
}

pub(crate) fn resolve_git_bash_for_runtime() -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(path) = find_git_bash() {
            return Ok(Some(path));
        }

        return Err(
            "未检测到 Git Bash。请先在设置中安装 PortableGit，或手动安装 Git for Windows。"
                .to_string(),
        );
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(None)
    }
}

#[cfg(target_os = "windows")]
async fn download_portable_git_archive(
    client: &reqwest::Client,
    filename: &str,
) -> Result<Vec<u8>, String> {
    let sources = [
        format!("{}/{}/{}", GIT_DIST_GITHUB, GIT_RELEASE_TAG, filename),
        format!("{}/{}/{}", GIT_DIST_HUAWEI, GIT_RELEASE_TAG, filename),
        format!("{}/{}/{}", GIT_DIST_NPMMIRROR, GIT_RELEASE_TAG, filename),
    ];

    let mut last_error = String::new();

    for url in &sources {
        tracing::info!("尝试下载 PortableGit: {}", url);
        match client.get(url).send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    last_error = format!("下载失败 {}: HTTP {}", url, response.status());
                    tracing::warn!("{}", last_error);
                    continue;
                }

                match response.bytes().await {
                    Ok(bytes) => return Ok(bytes.to_vec()),
                    Err(error) => {
                        last_error = format!("读取下载内容失败 {}: {}", url, error);
                        tracing::warn!("{}", last_error);
                    }
                }
            }
            Err(error) => {
                last_error = format!("请求失败 {}: {}", url, error);
                tracing::warn!("{}", last_error);
            }
        }
    }

    Err(format!("PortableGit 下载失败：{}", last_error))
}

#[cfg(target_os = "windows")]
fn extract_portable_git(archive_path: &Path, install_dir: &Path) -> Result<(), String> {
    let output = Command::new(archive_path)
        .args([&format!("-o{}", install_dir.display()), "-y"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("启动 PortableGit 解压程序失败: {}", error))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(format!("PortableGit 解压失败，退出码: {}", output.status))
    } else {
        Err(format!(
            "PortableGit 解压失败，退出码: {}，错误: {}",
            output.status, stderr
        ))
    }
}

fn emit_setup_progress(app: &AppHandle, percent: u8, phase: &str) {
    let _ = app.emit(
        SETUP_PROGRESS_EVENT,
        serde_json::json!({
            "downloaded": 0,
            "total": 0,
            "percent": percent,
            "phase": phase
        }),
    );
}

async fn detect_china_network() -> bool {
    let client = match reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(3))
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!("创建网络探测客户端失败: {}", error);
            return false;
        }
    };

    client
        .head("https://www.google.com/generate_204")
        .send()
        .await
        .is_err()
}

fn get_node_archive_info() -> Result<(String, &'static str), String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok((format!("node-{}-darwin-arm64", NODE_LTS_VERSION), "tar.gz")),
        ("macos", "x86_64") => Ok((format!("node-{}-darwin-x64", NODE_LTS_VERSION), "tar.gz")),
        ("windows", "x86_64") => Ok((format!("node-{}-win-x64", NODE_LTS_VERSION), "zip")),
        ("windows", "aarch64") => Ok((format!("node-{}-win-arm64", NODE_LTS_VERSION), "zip")),
        ("linux", "x86_64") => Ok((format!("node-{}-linux-x64", NODE_LTS_VERSION), "tar.gz")),
        ("linux", "aarch64") => Ok((format!("node-{}-linux-arm64", NODE_LTS_VERSION), "tar.gz")),
        (os, arch) => Err(format!("不支持当前平台安装 Node.js: {}-{}", os, arch)),
    }
}

async fn download_with_progress(
    app: &AppHandle,
    client: &reqwest::Client,
    url: &str,
    phase: &str,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("请求失败: {}", error))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded = 0_u64;
    let mut bytes = Vec::with_capacity(total as usize);
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("读取下载流失败: {}", error))?;
        downloaded += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);

        let percent = if total > 0 {
            (downloaded.saturating_mul(80) / total) as u8
        } else {
            0
        };
        emit_setup_progress(app, percent, phase);
    }

    Ok(bytes)
}

fn extract_node_archive(data: &[u8], ext: &str, install_dir: &Path) -> Result<(), String> {
    match ext {
        "tar.gz" => {
            let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(data));
            let mut archive = tar::Archive::new(decoder);

            for entry in archive
                .entries()
                .map_err(|error| format!("读取 tar 失败: {}", error))?
            {
                let mut entry = entry.map_err(|error| format!("读取 tar 条目失败: {}", error))?;
                let path = entry
                    .path()
                    .map_err(|error| format!("解析路径失败: {}", error))?;
                let stripped: PathBuf = path.components().skip(1).collect();

                if stripped.as_os_str().is_empty() {
                    continue;
                }

                let target = install_dir.join(&stripped);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|error| format!("创建目录失败 {:?}: {}", parent, error))?;
                }

                entry
                    .unpack(&target)
                    .map_err(|error| format!("解压 {:?} 失败: {}", stripped, error))?;
            }

            Ok(())
        }
        "zip" => {
            let reader = std::io::Cursor::new(data);
            let mut archive = zip::ZipArchive::new(reader)
                .map_err(|error| format!("读取 zip 失败: {}", error))?;

            for index in 0..archive.len() {
                let mut file = archive
                    .by_index(index)
                    .map_err(|error| format!("读取 zip 条目失败: {}", error))?;
                let stripped = file
                    .name()
                    .split_once('/')
                    .map(|(_, value)| value.to_string())
                    .unwrap_or_default();

                if stripped.is_empty() {
                    continue;
                }

                let target = install_dir.join(&stripped);
                if file.is_dir() {
                    std::fs::create_dir_all(&target)
                        .map_err(|error| format!("创建目录失败 {:?}: {}", target, error))?;
                    continue;
                }

                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|error| format!("创建目录失败 {:?}: {}", parent, error))?;
                }

                let mut output = std::fs::File::create(&target)
                    .map_err(|error| format!("创建文件失败 {:?}: {}", target, error))?;
                std::io::copy(&mut file, &mut output)
                    .map_err(|error| format!("写入文件失败 {:?}: {}", target, error))?;
            }

            Ok(())
        }
        _ => Err(format!("不支持的 Node.js 压缩格式: {}", ext)),
    }
}

#[cfg(unix)]
fn chmod_node_binaries(install_dir: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let bin_dir = install_dir.join("bin");
    if let Ok(entries) = std::fs::read_dir(bin_dir) {
        for entry in entries.flatten() {
            let _ = std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o755));
        }
    }
}

async fn install_node_env_inner(app: &AppHandle, china: bool) -> Result<(), String> {
    tracing::info!("开始安装本地 Node.js 运行时: china_network={}", china);

    if let Some(node_path) = get_local_node_executable_path() {
        tracing::info!("本地 Node.js 已存在，跳过安装: {}", node_path.display());
        emit_setup_progress(app, 100, "node_complete");
        return Ok(());
    }

    let install_dir = get_local_node_dir()?;
    tracing::info!("本地 Node.js 安装目录: {}", install_dir.display());
    if install_dir.exists() {
        tracing::warn!(
            "检测到残缺的本地 Node.js 目录，准备清理: {}",
            install_dir.display()
        );
        std::fs::remove_dir_all(&install_dir)
            .map_err(|error| format!("清理旧的 Node.js 目录失败: {}", error))?;
    }
    std::fs::create_dir_all(&install_dir)
        .map_err(|error| format!("创建 Node.js 目录失败: {}", error))?;

    let (archive_name, ext) = get_node_archive_info()?;
    let filename = format!("{}.{}", archive_name, ext);
    tracing::info!(
        "Node.js 安装包信息: archive_name={}, ext={}, filename={}",
        archive_name,
        ext,
        filename
    );
    let sources: Vec<String> = if china {
        vec![
            format!("{}/{}/{}", NODE_DIST_NPMMIRROR, NODE_LTS_VERSION, filename),
            format!("{}/{}/{}", NODE_DIST_HUAWEI, NODE_LTS_VERSION, filename),
            format!("{}/{}/{}", NODE_DIST_OFFICIAL, NODE_LTS_VERSION, filename),
        ]
    } else {
        vec![
            format!("{}/{}/{}", NODE_DIST_OFFICIAL, NODE_LTS_VERSION, filename),
            format!("{}/{}/{}", NODE_DIST_NPMMIRROR, NODE_LTS_VERSION, filename),
        ]
    };
    tracing::info!("Node.js 下载源候选: {:?}", sources);

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|error| format!("创建 Node.js 下载客户端失败: {}", error))?;

    let mut archive_bytes: Option<Vec<u8>> = None;
    let mut last_error = String::new();
    for url in &sources {
        tracing::info!("尝试下载 Node.js: {}", url);
        emit_setup_progress(app, 0, "node_downloading");
        match download_with_progress(app, &client, url, "node_downloading").await {
            Ok(bytes) => {
                archive_bytes = Some(bytes);
                break;
            }
            Err(error) => {
                last_error = format!("下载失败 {}: {}", url, error);
                tracing::warn!("{}", last_error);
            }
        }
    }

    let bytes = archive_bytes.ok_or_else(|| format!("Node.js 下载失败：{}", last_error))?;
    emit_setup_progress(app, 85, "node_extracting");
    extract_node_archive(&bytes, ext, &install_dir)?;

    #[cfg(unix)]
    chmod_node_binaries(&install_dir);

    let node_path = get_local_node_executable_path()
        .ok_or_else(|| "Node.js 安装完成后仍未找到 node 可执行文件".to_string())?;
    tracing::info!(
        "Node.js 安装成功: node={}, npm={:?}",
        node_path.display(),
        get_local_npm_executable_path().map(|path| path.display().to_string())
    );
    emit_setup_progress(app, 100, "node_complete");
    Ok(())
}

#[cfg(target_os = "windows")]
async fn install_portable_git_inner(app: Option<&AppHandle>) -> Result<String, String> {
    let install_dir = get_local_git_dir()?;

    if install_dir.exists() && get_local_git_bash().is_none() {
        tracing::warn!(
            "检测到残缺的 PortableGit 目录，准备清理: {}",
            install_dir.display()
        );
        std::fs::remove_dir_all(&install_dir)
            .map_err(|error| format!("清理旧的 PortableGit 目录失败: {}", error))?;
    }

    if let Some(existing) = get_local_git_bash() {
        tracing::info!("PortableGit 已安装: {}", existing);
        return Ok(existing);
    }

    std::fs::create_dir_all(&install_dir)
        .map_err(|error| format!("创建 PortableGit 目录失败: {}", error))?;

    let arch_suffix = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        _ => "64",
    };
    let filename = format!(
        "PortableGit-{}-{}-bit.7z.exe",
        GIT_PORTABLE_VERSION, arch_suffix
    );

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|error| format!("创建下载客户端失败: {}", error))?;

    if let Some(app) = app {
        emit_setup_progress(app, 10, "git_downloading");
    }

    let bytes = download_portable_git_archive(&client, &filename).await?;
    let archive_path = install_dir.join(&filename);
    std::fs::write(&archive_path, bytes)
        .map_err(|error| format!("写入 PortableGit 安装包失败: {}", error))?;

    if let Some(app) = app {
        emit_setup_progress(app, 80, "git_extracting");
    }

    let extract_result = extract_portable_git(&archive_path, &install_dir);
    let _ = std::fs::remove_file(&archive_path);
    extract_result?;

    let bash_path = get_local_git_bash()
        .ok_or_else(|| "PortableGit 安装完成后仍未找到 bash.exe".to_string())?;

    if let Some(app) = app {
        emit_setup_progress(app, 100, "git_complete");
    }

    tracing::info!("PortableGit 安装成功: {}", bash_path);
    Ok(bash_path)
}

#[cfg(not(target_os = "windows"))]
async fn install_portable_git_inner(_app: Option<&AppHandle>) -> Result<String, String> {
    Err("PortableGit 安装仅支持 Windows".to_string())
}

#[tauri::command]
pub async fn install_portable_git() -> Result<String, String> {
    install_portable_git_inner(None).await
}

#[tauri::command]
pub async fn check_node_env() -> Result<NodeEnvStatus, String> {
    tracing::info!("开始检查 Node.js / npm 运行环境");

    if let Some(node_path) = get_local_node_executable_path() {
        let output = TokioCommand::new(&node_path)
            .arg("--version")
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|error| format!("检查本地 Node.js 失败: {}", error))?;

        let version = if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        };

        let status = NodeEnvStatus {
            node_available: version.is_some(),
            node_version: version,
            node_source: Some("local".to_string()),
            npm_available: get_local_npm_executable_path().is_some(),
        };
        tracing::info!(
            "Node.js 环境检查结果: source={:?}, node_available={}, node_version={:?}, npm_available={}",
            status.node_source,
            status.node_available,
            status.node_version,
            status.npm_available
        );
        return Ok(status);
    }

    #[cfg(target_os = "windows")]
    let output = {
        let mut cmd = TokioCommand::new("cmd");
        cmd.args(["/C", "node", "--version"])
            .env("PATH", build_enriched_path())
            .stdin(Stdio::null());
        cmd.creation_flags(CREATE_NO_WINDOW).output().await
    };

    #[cfg(not(target_os = "windows"))]
    let output = TokioCommand::new("node")
        .arg("--version")
        .env("PATH", build_enriched_path())
        .stdin(Stdio::null())
        .output()
        .await;

    let status = match output {
        Ok(output) if output.status.success() => NodeEnvStatus {
            node_available: true,
            node_version: Some(String::from_utf8_lossy(&output.stdout).trim().to_string()),
            node_source: Some("system".to_string()),
            npm_available: find_npm_binary().is_some(),
        },
        Ok(_) | Err(_) => NodeEnvStatus {
            node_available: false,
            node_version: None,
            node_source: None,
            npm_available: find_npm_binary().is_some(),
        },
    };

    tracing::info!(
        "Node.js 环境检查结果: source={:?}, node_available={}, node_version={:?}, npm_available={}",
        status.node_source,
        status.node_available,
        status.node_version,
        status.npm_available
    );

    Ok(status)
}

#[tauri::command]
pub async fn install_node_env(app: AppHandle, confirmed: Option<bool>) -> Result<(), String> {
    if !confirmed.unwrap_or(false) {
        return Err("安装 Node.js 运行时前需要用户确认".to_string());
    }

    let china = detect_china_network().await;
    install_node_env_inner(&app, china).await
}

fn find_npm_binary() -> Option<String> {
    if let Some(local_npm) = get_local_npm_executable_path() {
        if is_valid_executable(&local_npm) {
            tracing::info!("在应用本地目录找到 npm: {}", local_npm.display());
            return Some(local_npm.to_string_lossy().to_string());
        }
    }

    #[cfg(target_os = "windows")]
    {
        let path_env = build_enriched_path();
        for query in ["npm.cmd", "npm"] {
            if let Ok(output) = Command::new("cmd")
                .args(["/C", "where", query])
                .env("PATH", &path_env)
                .creation_flags(CREATE_NO_WINDOW)
                .output()
            {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines() {
                        let path = line.trim().to_string();
                        if !path.is_empty() && is_valid_executable(Path::new(&path)) {
                            tracing::info!("在增强 PATH 中找到 npm: {}", path);
                            return Some(path);
                        }
                    }
                }
            }
        }

        tracing::warn!("在增强 PATH 中未找到 npm");
        return None;
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(output) = Command::new("sh").args(["-l", "-c", "which npm"]).output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() && is_valid_executable(Path::new(&path)) {
                    tracing::info!("在登录 shell 中找到 npm: {}", path);
                    return Some(path);
                }
            }
        }

        if let Ok(path) = which::which("npm") {
            let path = path.to_string_lossy().to_string();
            tracing::info!("在 PATH 中找到 npm: {}", path);
            return Some(path);
        }

        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join(".npm-global/bin/npm"),
                home.join(".nvm/current/bin/npm"),
                home.join(".volta/bin/npm"),
                home.join(".fnm/current/bin/npm"),
            ];

            for candidate in candidates {
                if is_valid_executable(&candidate) {
                    tracing::info!("在用户目录找到 npm: {}", candidate.display());
                    return Some(candidate.to_string_lossy().to_string());
                }
            }
        }

        for candidate in ["/usr/local/bin/npm", "/opt/homebrew/bin/npm"] {
            if is_valid_executable(Path::new(candidate)) {
                tracing::info!("在系统路径找到 npm: {}", candidate);
                return Some(candidate.to_string());
            }
        }

        tracing::warn!("未找到 npm 可执行文件");
        None
    }
}

fn get_npm_command() -> Result<String, String> {
    find_npm_binary().ok_or_else(|| {
        "未找到 npm，可先安装 Node.js 后重试。应用启动时可能没有拿到完整 PATH。".to_string()
    })
}

async fn install_claude_cli_via_npm(app: &AppHandle, china: bool) -> Result<(), String> {
    emit_setup_progress(app, 40, "npm_fallback");

    let npm_path = get_npm_command()?;
    let enriched_path = build_enriched_path();
    let prefix_dir = get_local_cli_prefix_dir()?;
    let cache_dir = get_local_cli_cache_dir()?;
    std::fs::create_dir_all(&prefix_dir)
        .map_err(|error| format!("创建本地 CLI 目录失败: {}", error))?;
    std::fs::create_dir_all(&cache_dir)
        .map_err(|error| format!("创建 npm 缓存目录失败: {}", error))?;

    tracing::info!(
        "准备通过 npm 安装 Claude CLI: npm_path={}, prefix_dir={}, cache_dir={}, runtime_path={}",
        npm_path,
        prefix_dir.display(),
        cache_dir.display(),
        enriched_path
    );

    let registries: Vec<&str> = if china {
        vec![
            "https://registry.npmmirror.com",
            "https://mirrors.huaweicloud.com/repository/npm",
            "https://registry.npmjs.org",
        ]
    } else {
        vec![
            "https://registry.npmjs.org",
            "https://registry.npmmirror.com",
        ]
    };

    let mut last_error = String::new();
    for (index, registry) in registries.iter().enumerate() {
        let base_percent = 45u8.saturating_add((index as u8) * 10);
        emit_setup_progress(app, base_percent, "npm_fallback");

        let args: Vec<String> = vec![
            "install".to_string(),
            "-g".to_string(),
            "@anthropic-ai/claude-code".to_string(),
            format!("--registry={}", registry),
            format!("--prefix={}", prefix_dir.display()),
            format!("--cache={}", cache_dir.display()),
        ];

        tracing::info!(
            "尝试通过 npm 安装 Claude CLI: registry={}, prefix={}, cache={}",
            registry,
            prefix_dir.display(),
            cache_dir.display()
        );

        #[cfg(target_os = "windows")]
        let result = {
            let mut cmd = TokioCommand::new("cmd");
            cmd.arg("/C").arg(&npm_path);
            cmd.args(args.iter().map(|arg| arg.as_str()));
            cmd.env("PATH", &enriched_path)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .creation_flags(CREATE_NO_WINDOW);
            tokio::time::timeout(std::time::Duration::from_secs(300), cmd.output()).await
        };

        #[cfg(not(target_os = "windows"))]
        let result = {
            let mut cmd = TokioCommand::new(&npm_path);
            cmd.args(args.iter().map(|arg| arg.as_str()))
                .env("PATH", &enriched_path)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            tokio::time::timeout(std::time::Duration::from_secs(300), cmd.output()).await
        };

        match result {
            Ok(Ok(output)) if output.status.success() => {
                emit_setup_progress(app, 90, "installing");
                return Ok(());
            }
            Ok(Ok(output)) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                last_error = format!("npm 安装失败（{}）：{}", registry, stderr);
                tracing::warn!("{}", last_error);
            }
            Ok(Err(error)) => {
                last_error = format!("执行 npm 失败：{}", error);
                tracing::warn!("{}", last_error);
            }
            Err(_) => {
                last_error = format!("npm 安装超时（{}）", registry);
                tracing::warn!("{}", last_error);
            }
        }
    }

    Err(last_error)
}

async fn ensure_existing_claude_runtime(
    app: &AppHandle,
    binary: &str,
    allow_node_install: bool,
    china: bool,
) -> Result<bool, String> {
    let cli_path = Path::new(binary);

    match resolve_claude_runtime_env(cli_path) {
        Ok(_) => {
            tracing::info!("现有 Claude CLI 运行环境已就绪: {}", binary);
            emit_setup_progress(app, 100, "complete");
            Ok(true)
        }
        Err(error) => {
            tracing::warn!("现有 Claude CLI 运行环境未就绪: {}", error);

            let missing_node_runtime = claude_binary_requires_node(cli_path)
                && find_node_binary_in_path(&build_enriched_path()).is_none();

            if !missing_node_runtime {
                return Ok(false);
            }

            if !allow_node_install {
                return Err(
                    "检测到安装 Claude Code CLI 还需要补充 Node.js 运行时，请先在界面中确认后再继续。"
                        .to_string(),
                );
            }

            install_node_env_inner(app, china).await?;

            resolve_claude_runtime_env(cli_path)?;
            tracing::info!("现有 Claude CLI 的 Node.js 运行时补齐完成: {}", binary);
            emit_setup_progress(app, 100, "complete");
            Ok(true)
        }
    }
}

async fn install_claude_cli_inner(app: AppHandle, allow_node_install: bool) -> Result<(), String> {
    tracing::info!(
        "开始安装 Claude CLI: allow_node_install={}",
        allow_node_install
    );
    let china = detect_china_network().await;
    tracing::info!("Claude CLI 安装网络环境判定: china_network={}", china);

    if let Some(binary) = find_claude_binary() {
        #[cfg(target_os = "windows")]
        if find_git_bash().is_none() {
            tracing::info!("Claude CLI 已存在，但缺少 Git Bash，继续补齐依赖");
        } else {
            tracing::info!("Claude CLI 已存在，开始确认运行时依赖: {}", binary);
            if ensure_existing_claude_runtime(&app, &binary, allow_node_install, china).await? {
                return Ok(());
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            tracing::info!("Claude CLI 已存在，开始确认运行时依赖: {}", binary);
            if ensure_existing_claude_runtime(&app, &binary, allow_node_install, china).await? {
                return Ok(());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if find_git_bash().is_none() {
            install_portable_git_inner(Some(&app)).await?;
            if let Some(binary) = find_claude_binary() {
                if ensure_existing_claude_runtime(&app, &binary, allow_node_install, china).await? {
                    return Ok(());
                }
            }
        }
    }

    if find_npm_binary().is_none() {
        tracing::warn!("安装 Claude CLI 前未检测到 npm");
        if !allow_node_install {
            return Err(
                "检测到安装 Claude Code CLI 还需要补充 Node.js 运行时，请先在界面中确认后再继续。"
                    .to_string(),
            );
        }
        tracing::info!("未检测到 npm，准备安装本地 Node.js 运行时");
        install_node_env_inner(&app, china).await?;
    }

    install_claude_cli_via_npm(&app, china).await?;

    let binary = find_claude_binary()
        .ok_or_else(|| "Claude CLI 安装完成后仍未找到可执行文件".to_string())?;
    tracing::info!("Claude CLI 安装成功: {}", binary);
    emit_setup_progress(&app, 100, "complete");
    Ok(())
}

#[tauri::command]
pub async fn install_claude_cli(app: AppHandle) -> Result<(), String> {
    install_claude_cli_inner(app, false).await
}

#[tauri::command]
pub async fn install_claude_cli_with_node_runtime(app: AppHandle) -> Result<(), String> {
    install_claude_cli_inner(app, true).await
}
