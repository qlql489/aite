// Worktree management commands
//
// Provides worktree creation, tracking, and cleanup functionality

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Worktree mapping information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeMapping {
    pub session_id: String,
    pub repo_root: String,
    pub branch: String,
    pub worktree_path: String,
    pub created_at: u64,
}

/// Worktree list item
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    pub path: String,
    pub branch: String,
    pub head: String,
    pub is_main_worktree: bool,
    pub is_dirty: bool,
}

/// Git repo info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRepoInfo {
    pub repo_root: String,
    pub repo_name: String,
    pub current_branch: String,
    pub default_branch: String,
    pub is_worktree: bool,
}

/// Branch info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub worktree_path: Option<String>,
    pub ahead: usize,
    pub behind: usize,
}

/// Worktree create result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCreateResult {
    pub worktree_path: String,
    pub branch: String,
    pub is_new: bool,
}

/// Get the worktree tracker file path (~/.aite/worktrees.json)
fn get_tracker_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let aite_dir = home.join(".aite");
    fs::create_dir_all(&aite_dir).ok();
    aite_dir.join("worktrees.json")
}

/// Sanitize branch name for use in path
fn sanitize_branch(branch: &str) -> String {
    branch.replace('/', "--")
}

/// Get worktrees base directory (~/.aite/worktrees)
fn get_worktrees_base() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".aite").join("worktrees")
}

/// Load worktree mappings from disk
fn load_mappings() -> Vec<WorktreeMapping> {
    let path = get_tracker_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(mappings) = serde_json::from_str::<Vec<WorktreeMapping>>(&content) {
                return mappings;
            }
        }
    }
    Vec::new()
}

/// Save worktree mappings to disk
fn save_mappings(mappings: &[WorktreeMapping]) -> Result<(), String> {
    let path = get_tracker_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(mappings).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

/// Run a git command and return output
fn git_run(cwd: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Run a git command safely (returns None on error)
fn git_safe(cwd: &str, args: &[&str]) -> Option<String> {
    git_run(cwd, args).ok()
}

/// Get repository information
#[tauri::command]
pub fn get_repo_info(path: String) -> Result<GitRepoInfo, String> {
    let repo_root =
        git_safe(&path, &["rev-parse", "--show-toplevel"]).ok_or("Not a git repository")?;

    let current_branch = git_safe(&repo_root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|| "HEAD".to_string());

    let git_dir = git_safe(&repo_root, &["rev-parse", "--git-dir"]).unwrap_or_default();
    let is_worktree = git_dir.contains("/worktrees/");

    let default_branch = resolve_default_branch(&repo_root);

    let repo_name = PathBuf::from(&repo_root)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(GitRepoInfo {
        repo_root,
        repo_name,
        current_branch,
        default_branch,
        is_worktree,
    })
}

/// Resolve the default branch for a repository
fn resolve_default_branch(repo_root: &str) -> String {
    // Try origin HEAD
    if let Some(origin_ref) = git_safe(repo_root, &["symbolic-ref", "refs/remotes/origin/HEAD"]) {
        return origin_ref.replace("refs/remotes/origin/", "");
    }

    // Fallback: check if main or master exists
    let branches = git_safe(repo_root, &["branch", "--list", "main", "master"]).unwrap_or_default();
    if branches.contains("main") {
        return "main".to_string();
    }
    if branches.contains("master") {
        return "master".to_string();
    }

    "main".to_string()
}

/// List branches for a repository
#[tauri::command]
pub fn list_branches(repo_root: String) -> Result<Vec<BranchInfo>, String> {
    let worktrees = list_worktrees_internal(&repo_root);
    let worktree_by_branch: std::collections::HashMap<String, String> = worktrees
        .iter()
        .filter(|wt| !wt.branch.is_empty())
        .map(|wt| (wt.branch.clone(), wt.path.clone()))
        .collect();

    let mut result: Vec<BranchInfo> = Vec::new();

    // Local branches
    if let Some(local_raw) = git_safe(
        &repo_root,
        &[
            "for-each-ref",
            "--format=%(refname:short)%09%(HEAD)",
            "refs/heads/",
        ],
    ) {
        for line in local_raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.is_empty() {
                continue;
            }
            let name = parts[0].to_string();
            let is_current = parts.get(1).map(|s| s.trim() == "*").unwrap_or(false);

            let (ahead, behind) = get_branch_status(&repo_root, &name);

            result.push(BranchInfo {
                name: name.clone(),
                is_current,
                is_remote: false,
                worktree_path: worktree_by_branch.get(&name).cloned(),
                ahead,
                behind,
            });
        }
    }

    // Remote branches
    let local_names: std::collections::HashSet<String> =
        result.iter().map(|b| b.name.clone()).collect();
    if let Some(remote_raw) = git_safe(
        &repo_root,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/remotes/origin/",
        ],
    ) {
        for line in remote_raw.lines() {
            let full = line.trim();
            if full.is_empty() || full == "origin/HEAD" {
                continue;
            }
            let name = full.replace("origin/", "");
            if local_names.contains(&name) {
                continue;
            }
            result.push(BranchInfo {
                name,
                is_current: false,
                is_remote: true,
                worktree_path: None,
                ahead: 0,
                behind: 0,
            });
        }
    }

    Ok(result)
}

/// Get branch ahead/behind status
fn get_branch_status(repo_root: &str, branch_name: &str) -> (usize, usize) {
    let raw = git_safe(
        repo_root,
        &[
            "rev-list",
            "--left-right",
            &format!("origin/{branch_name}...{branch_name}"),
            "--count",
        ],
    )
    .unwrap_or_default();

    if raw.is_empty() {
        return (0, 0);
    }

    let parts: Vec<&str> = raw.split_whitespace().collect();
    if parts.len() >= 2 {
        let behind = parts[0].parse().unwrap_or(0);
        let ahead = parts[1].parse().unwrap_or(0);
        (ahead, behind)
    } else {
        (0, 0)
    }
}

/// List worktrees for a repository
fn list_worktrees_internal(repo_root: &str) -> Vec<WorktreeInfo> {
    let raw = match git_safe(repo_root, &["worktree", "list", "--porcelain"]) {
        Some(r) => r,
        None => return Vec::new(),
    };

    let mut worktrees: Vec<WorktreeInfo> = Vec::new();
    let mut current: Option<WorktreeInfo> = None;

    for line in raw.lines() {
        if line.starts_with("worktree ") {
            if let Some(wt) = current.take() {
                worktrees.push(wt);
            }
            current = Some(WorktreeInfo {
                path: line[9..].to_string(),
                branch: String::new(),
                head: String::new(),
                is_main_worktree: false,
                is_dirty: false,
            });
        } else if line.starts_with("HEAD ") {
            if let Some(ref mut wt) = current {
                wt.head = line[5..].to_string();
            }
        } else if line.starts_with("branch ") {
            if let Some(ref mut wt) = current {
                wt.branch = line[7..].replace("refs/heads/", "");
            }
        } else if line == "bare" {
            if let Some(ref mut wt) = current {
                wt.is_main_worktree = true;
            }
        }
    }

    if let Some(wt) = current {
        if worktrees.is_empty() {
            let mut wt = wt;
            wt.is_main_worktree = true;
            worktrees.push(wt);
        } else {
            worktrees.push(wt);
        }
    }

    // Check dirty status
    for wt in &mut worktrees {
        wt.is_dirty = is_worktree_dirty(&wt.path);
    }

    worktrees
}

/// Check if a worktree has uncommitted changes
fn is_worktree_dirty(worktree_path: &str) -> bool {
    let status = git_safe(worktree_path, &["status", "--porcelain"]).unwrap_or_default();
    !status.is_empty()
}

/// List worktrees (public command)
#[tauri::command]
pub fn list_worktrees(repo_root: String) -> Result<Vec<WorktreeInfo>, String> {
    Ok(list_worktrees_internal(&repo_root))
}

/// Ensure a worktree exists for the given branch
#[tauri::command]
pub fn ensure_worktree(
    repo_root: String,
    branch: String,
    base_branch: Option<String>,
    create_branch: Option<bool>,
) -> Result<WorktreeCreateResult, String> {
    let repo_name = PathBuf::from(&repo_root)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let worktrees_base = get_worktrees_base();
    let target_path = worktrees_base
        .join(&repo_name)
        .join(sanitize_branch(&branch));

    // Check if worktree already exists
    let existing = list_worktrees_internal(&repo_root);
    if let Some(found) = existing.iter().find(|wt| wt.branch == branch) {
        return Ok(WorktreeCreateResult {
            worktree_path: found.path.clone(),
            branch: branch.clone(),
            is_new: false,
        });
    }

    // Ensure parent directory exists
    fs::create_dir_all(worktrees_base.join(repo_name)).map_err(|e| e.to_string())?;

    // Check if branch exists locally or remotely
    let branch_exists = git_safe(
        &repo_root,
        &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
    )
    .is_some();
    let remote_branch_exists = git_safe(
        &repo_root,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/remotes/origin/{branch}"),
        ],
    )
    .is_some();

    if branch_exists {
        // Worktree add with existing local branch
        git_run(
            &repo_root,
            &["worktree", "add", &target_path.to_string_lossy(), &branch],
        )?;
        return Ok(WorktreeCreateResult {
            worktree_path: target_path.to_string_lossy().to_string(),
            branch,
            is_new: false,
        });
    }

    if remote_branch_exists {
        // Create local tracking branch from remote
        git_run(
            &repo_root,
            &[
                "worktree",
                "add",
                "-b",
                &branch,
                &target_path.to_string_lossy(),
                &format!("origin/{branch}"),
            ],
        )?;
        return Ok(WorktreeCreateResult {
            worktree_path: target_path.to_string_lossy().to_string(),
            branch,
            is_new: false,
        });
    }

    // Create new branch from base
    if create_branch.unwrap_or(true) {
        let base = base_branch.unwrap_or_else(|| resolve_default_branch(&repo_root));
        git_run(
            &repo_root,
            &[
                "worktree",
                "add",
                "-b",
                &branch,
                &target_path.to_string_lossy(),
                &base,
            ],
        )?;
        return Ok(WorktreeCreateResult {
            worktree_path: target_path.to_string_lossy().to_string(),
            branch,
            is_new: true,
        });
    }

    Err(format!(
        "Branch '{}' does not exist and createBranch is false",
        branch
    ))
}

/// Remove a worktree
#[tauri::command]
pub fn remove_worktree(
    repo_root: String,
    worktree_path: String,
    force: Option<bool>,
) -> Result<serde_json::Value, String> {
    let path = PathBuf::from(&worktree_path);

    if !path.exists() {
        // Already gone, clean up git's reference
        git_run(&repo_root, &["worktree", "prune"])?;
        return Ok(serde_json::json!({ "removed": true }));
    }

    if !force.unwrap_or(false) && is_worktree_dirty(&worktree_path) {
        return Ok(serde_json::json!({
            "removed": false,
            "reason": "Worktree has uncommitted changes. Use force to remove anyway."
        }));
    }

    let mut args = vec!["worktree", "remove"];
    if force.unwrap_or(false) {
        args.push("--force");
    }
    args.push(&worktree_path);
    git_run(&repo_root, &args)?;

    Ok(serde_json::json!({ "removed": true }))
}

// ============ Worktree Tracker ============

/// Add a worktree mapping for a session
#[tauri::command]
pub fn track_worktree(mapping: WorktreeMapping) -> Result<(), String> {
    let mut mappings = load_mappings();

    // Remove any existing mapping for this session
    mappings.retain(|m| m.session_id != mapping.session_id);
    mappings.push(mapping);

    save_mappings(&mappings)
}

/// Remove a worktree mapping for a session
#[tauri::command]
pub fn untrack_worktree(session_id: String) -> Result<Option<WorktreeMapping>, String> {
    let mut mappings = load_mappings();

    if let Some(idx) = mappings.iter().position(|m| m.session_id == session_id) {
        let removed = mappings.remove(idx);
        save_mappings(&mappings)?;
        Ok(Some(removed))
    } else {
        Ok(None)
    }
}

/// Get worktree mapping for a session
#[tauri::command]
pub fn get_worktree_mapping(session_id: String) -> Result<Option<WorktreeMapping>, String> {
    let mappings = load_mappings();
    Ok(mappings.into_iter().find(|m| m.session_id == session_id))
}

/// Check if a worktree is in use by other sessions
#[tauri::command]
pub fn is_worktree_in_use(
    worktree_path: String,
    exclude_session_id: Option<String>,
) -> Result<bool, String> {
    let mappings = load_mappings();
    Ok(mappings.iter().any(|m| {
        m.worktree_path == worktree_path
            && exclude_session_id
                .as_ref()
                .map(|id| &m.session_id != id)
                .unwrap_or(true)
    }))
}

/// Get all worktree mappings
#[tauri::command]
pub fn get_all_worktree_mappings() -> Result<Vec<WorktreeMapping>, String> {
    Ok(load_mappings())
}
