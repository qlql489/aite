use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Git 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub branch: Option<String>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    pub total_lines_added: Option<usize>,
    pub total_lines_removed: Option<usize>,
    pub is_worktree: Option<bool>,
    pub repo_root: Option<String>,
}

/// Git 拉取结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitPullResult {
    pub success: bool,
    pub output: Option<String>,
    pub git_ahead: Option<usize>,
    pub git_behind: Option<usize>,
}

/// 获取 Git 仓库信息
#[tauri::command]
pub async fn get_git_info(project_path: String) -> Result<GitInfo, String> {
    let path = Path::new(&project_path);

    // 检查是否在 Git 仓库中
    let repo = match git2::Repository::discover(path) {
        Ok(r) => r,
        Err(_) => {
            return Ok(GitInfo {
                branch: None,
                ahead: None,
                behind: None,
                total_lines_added: None,
                total_lines_removed: None,
                is_worktree: None,
                repo_root: None,
            })
        }
    };

    // 获取仓库根目录
    let repo_root = repo.workdir().and_then(|p| p.to_str()).map(String::from);

    // 获取当前分支
    let head = match repo.head() {
        Ok(head) => head,
        Err(_) => {
            return Ok(GitInfo {
                branch: None,
                ahead: None,
                behind: None,
                total_lines_added: None,
                total_lines_removed: None,
                is_worktree: None,
                repo_root,
            })
        }
    };

    let branch_name = head.shorthand().map(String::from);

    // 检查是否为 worktree
    let workdir = repo.workdir();
    let git_dir = repo.path();
    let is_worktree = match workdir {
        Some(wd) => wd != git_dir && !git_dir.ends_with(".git"),
        None => false,
    };

    // 获取 upstream 信息
    let (ahead, behind) = if let Ok(local_oid) = head.target().ok_or("无 HEAD OID") {
        // 尝试获取 upstream 的 OID
        let upstream_oid = repo
            .find_branch("origin/main", git2::BranchType::Local)
            .or_else(|_| repo.find_branch("origin/master", git2::BranchType::Local))
            .ok()
            .and_then(|b| b.get().target());

        match upstream_oid {
            Some(upstream_oid_val) => match repo.graph_ahead_behind(local_oid, upstream_oid_val) {
                Ok((a, b)) => (Some(a), Some(b)),
                Err(_) => (None, None),
            },
            None => (None, None),
        }
    } else {
        (None, None)
    };

    // 统计信息（暂时返回 None）
    let total_lines_added = None;
    let total_lines_removed = None;

    Ok(GitInfo {
        branch: branch_name,
        ahead,
        behind,
        total_lines_added,
        total_lines_removed,
        is_worktree: Some(is_worktree),
        repo_root,
    })
}

/// Git 拉取
#[tauri::command]
pub async fn git_pull(project_path: String) -> Result<GitPullResult, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&project_path)
        .arg("pull")
        .output();

    match output {
        Ok(result) => {
            let success = result.status.success();
            let output_str = if result.stdout.is_empty() {
                String::from_utf8_lossy(&result.stderr).to_string()
            } else {
                String::from_utf8_lossy(&result.stdout).to_string()
            };

            // 重新获取 Git 信息
            let git_info = get_git_info(project_path).await?;

            Ok(GitPullResult {
                success,
                output: Some(output_str),
                git_ahead: git_info.ahead,
                git_behind: git_info.behind,
            })
        }
        Err(e) => Err(format!("执行 git pull 失败: {}", e)),
    }
}

/// 获取 Git 分支列表
#[tauri::command]
pub async fn git_list_branches(project_path: String) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&project_path)
        .arg("branch")
        .arg("--format=%(refname:short)")
        .output();

    match output {
        Ok(result) => {
            if !result.status.success() {
                return Err(format!(
                    "git branch 失败: {}",
                    String::from_utf8_lossy(&result.stderr)
                ));
            }

            let branches = String::from_utf8_lossy(&result.stdout)
                .lines()
                .map(String::from)
                .collect();

            Ok(branches)
        }
        Err(e) => Err(format!("执行 git branch 失败: {}", e)),
    }
}

/// 获取当前分支名称
#[tauri::command]
pub async fn git_get_current_branch(project_path: String) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&project_path)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output();

    match output {
        Ok(result) => {
            let branch = String::from_utf8_lossy(&result.stdout).trim().to_string();
            if branch.is_empty() || branch == "HEAD" {
                Ok(None)
            } else {
                Ok(Some(branch))
            }
        }
        Err(e) => Err(format!("获取分支失败: {}", e)),
    }
}

/// 初始化 Git 仓库
#[tauri::command]
pub async fn git_init_repository(project_path: String) -> Result<GitInfo, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&project_path)
        .arg("init")
        .output();

    match output {
        Ok(result) => {
            if !result.status.success() {
                return Err(format!(
                    "git init 失败: {}",
                    String::from_utf8_lossy(&result.stderr)
                ));
            }
            get_git_info(project_path).await
        }
        Err(e) => Err(format!("执行 git init 失败: {}", e)),
    }
}

/// 切换或创建分支
#[tauri::command]
pub async fn git_checkout_branch(
    project_path: String,
    branch: String,
    create_branch: Option<bool>,
) -> Result<GitInfo, String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(&project_path);

    if create_branch.unwrap_or(false) {
        command.arg("checkout").arg("-b").arg(&branch);
    } else {
        command.arg("checkout").arg(&branch);
    }

    match command.output() {
        Ok(result) => {
            if !result.status.success() {
                return Err(format!(
                    "git checkout 失败: {}",
                    String::from_utf8_lossy(&result.stderr)
                ));
            }
            get_git_info(project_path).await
        }
        Err(e) => Err(format!("执行 git checkout 失败: {}", e)),
    }
}
