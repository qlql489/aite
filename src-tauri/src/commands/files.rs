// File-related Tauri Commands
// 复刻 TOKENICODE 项目的文件附件功能

use serde::Serialize;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

const TREE_IGNORES: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    "coverage",
    ".next",
    ".nuxt",
];

#[derive(Debug, Clone, Serialize)]
pub struct ProjectTreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<ProjectTreeNode>,
    pub has_unloaded_children: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectTreeSummary {
    pub total_files: usize,
    pub total_dirs: usize,
    pub displayed_entries: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectTreeResponse {
    pub root_path: String,
    pub root_name: String,
    pub summary: ProjectTreeSummary,
    pub tree: Vec<ProjectTreeNode>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectTreeChildrenResponse {
    pub path: String,
    pub children: Vec<ProjectTreeNode>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectFileResponse {
    pub path: String,
    pub content: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectFileSearchItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

fn resolve_project_path(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Failed to resolve project root: {}", e))?;
    let target = canonical_root.join(relative_path);
    let canonical_target = target
        .canonicalize()
        .map_err(|e| format!("Failed to resolve file path: {}", e))?;

    if !canonical_target.starts_with(&canonical_root) {
        return Err("Invalid file path: access denied".to_string());
    }

    Ok(canonical_target)
}

#[derive(Default)]
struct TreeStats {
    total_files: usize,
    total_dirs: usize,
    displayed_entries: usize,
    truncated: bool,
}

fn should_ignore(entry_name: &str) -> bool {
    TREE_IGNORES.iter().any(|ignored| ignored == &entry_name)
}

fn normalize_search(value: &str) -> String {
    value.to_lowercase()
}

fn fuzzy_match_score(candidate: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let normalized_candidate = normalize_search(candidate);
    let normalized_query = normalize_search(query);

    let mut score = 0_i32;
    let mut last_match_index: Option<usize> = None;

    for ch in normalized_query.chars() {
        let search_from = last_match_index.map(|idx| idx + 1).unwrap_or(0);
        let Some(relative_index) = normalized_candidate[search_from..].find(ch) else {
            return None;
        };
        let match_index = search_from + relative_index;

        score += 1;
        if last_match_index
            .map(|idx| match_index == idx + 1)
            .unwrap_or(false)
        {
            score += 2;
        }

        let previous_char = normalized_candidate[..match_index].chars().last();
        if match_index == 0 || matches!(previous_char, Some('/' | '.' | '_' | '-')) {
            score += 3;
        }

        last_match_index = Some(match_index);
    }

    score += (20.0 - normalized_candidate.len() as f32 / 4.0).max(0.0) as i32;
    Some(score)
}

fn dir_has_visible_entries(path: &Path) -> bool {
    fs::read_dir(path)
        .ok()
        .map(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .map(|name| !should_ignore(name))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn collect_directory_entries(current: &Path) -> Result<(Vec<fs::DirEntry>, Vec<fs::DirEntry>), String> {
    let entries = fs::read_dir(current)
        .map_err(|error| format!("Failed to read directory {}: {}", current.display(), error))?;

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for entry in entries.flatten() {
        let entry_name = entry.file_name();
        let Some(name) = entry_name.to_str() else {
            continue;
        };
        if should_ignore(name) {
            continue;
        }

        match entry.file_type() {
            Ok(file_type) if file_type.is_dir() => dirs.push(entry),
            Ok(file_type) if file_type.is_file() => files.push(entry),
            _ => {}
        }
    }

    dirs.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());
    files.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());

    Ok((dirs, files))
}

fn build_project_tree_node(root: &Path, entry: fs::DirEntry) -> ProjectTreeNode {
    let entry_path = entry.path();
    let file_name = entry.file_name().to_string_lossy().to_string();
    let relative_path = entry_path
        .strip_prefix(root)
        .unwrap_or(&entry_path)
        .to_string_lossy()
        .replace('\\', "/");

    let file_type = entry.file_type().ok();
    let is_dir = file_type
        .as_ref()
        .map(|kind| kind.is_dir())
        .unwrap_or(false);

    ProjectTreeNode {
        name: file_name,
        path: relative_path,
        is_dir,
        children: Vec::new(),
        has_unloaded_children: is_dir && dir_has_visible_entries(&entry_path),
    }
}

fn read_directory_level(
    root: &Path,
    current: &Path,
    max_entries: usize,
) -> Result<(Vec<ProjectTreeNode>, bool), String> {
    let (dirs, files) = collect_directory_entries(current)?;
    let mut nodes = Vec::new();
    let mut truncated = false;

    for entry in dirs.into_iter().chain(files.into_iter()) {
        if nodes.len() >= max_entries {
            truncated = true;
            break;
        }

        nodes.push(build_project_tree_node(root, entry));
    }

    Ok((nodes, truncated))
}

fn scan_directory(
    root: &Path,
    current: &Path,
    depth: usize,
    max_depth: usize,
    max_entries: usize,
    stats: &mut TreeStats,
) -> Result<Vec<ProjectTreeNode>, String> {
    if stats.displayed_entries >= max_entries {
        stats.truncated = true;
        return Ok(Vec::new());
    }

    let (dirs, files) = collect_directory_entries(current)?;

    let mut nodes = Vec::new();

    for entry in dirs.into_iter().chain(files.into_iter()) {
        if stats.displayed_entries >= max_entries {
            stats.truncated = true;
            break;
        }

        let entry_path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        let relative_path = entry_path
            .strip_prefix(root)
            .unwrap_or(&entry_path)
            .to_string_lossy()
            .replace('\\', "/");

        let file_type = entry.file_type().ok();
        let is_dir = file_type
            .as_ref()
            .map(|kind| kind.is_dir())
            .unwrap_or(false);
        let mut children = Vec::new();
        let mut has_unloaded_children = false;

        stats.displayed_entries += 1;

        if is_dir {
            stats.total_dirs += 1;
            if depth < max_depth && stats.displayed_entries < max_entries {
                children =
                    scan_directory(root, &entry_path, depth + 1, max_depth, max_entries, stats)?;
                has_unloaded_children = stats.truncated
                    || (depth + 1 >= max_depth && dir_has_visible_entries(&entry_path));
            } else {
                if depth < max_depth && stats.displayed_entries >= max_entries {
                    stats.truncated = true;
                }
                has_unloaded_children = dir_has_visible_entries(&entry_path);
            }
        } else {
            stats.total_files += 1;
        }

        nodes.push(ProjectTreeNode {
            name: file_name,
            path: relative_path,
            is_dir,
            children,
            has_unloaded_children,
        });
    }

    Ok(nodes)
}

#[tauri::command]
pub async fn read_project_tree(
    path: String,
    depth: Option<usize>,
    max_entries: Option<usize>,
) -> Result<ProjectTreeResponse, String> {
    let root = PathBuf::from(&path);
    if !root.exists() {
        return Err(format!("Path does not exist: {}", path));
    }
    if !root.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }

    let max_depth = depth.unwrap_or(5);
    let max_entries = max_entries.unwrap_or(4000).max(200);

    let mut stats = TreeStats::default();
    let tree = scan_directory(&root, &root, 0, max_depth, max_entries, &mut stats)?;

    Ok(ProjectTreeResponse {
        root_name: root
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.to_string())
            .unwrap_or_else(|| path.clone()),
        root_path: path,
        summary: ProjectTreeSummary {
            total_files: stats.total_files,
            total_dirs: stats.total_dirs,
            displayed_entries: stats.displayed_entries,
        },
        tree,
        truncated: stats.truncated,
    })
}

#[tauri::command]
pub async fn read_project_tree_children(
    root_path: String,
    dir_path: String,
    max_entries: Option<usize>,
) -> Result<ProjectTreeChildrenResponse, String> {
    let root = PathBuf::from(&root_path);
    if !root.exists() {
        return Err(format!("Path does not exist: {}", root_path));
    }
    if !root.is_dir() {
        return Err(format!("Path is not a directory: {}", root_path));
    }

    let resolved_dir = if dir_path.trim().is_empty() {
        root.clone()
    } else {
        resolve_project_path(&root, &dir_path)?
    };

    if !resolved_dir.is_dir() {
        return Err(format!("Target path is not a directory: {}", dir_path));
    }

    let max_entries = max_entries.unwrap_or(5000).max(100);
    let (children, truncated) = read_directory_level(&root, &resolved_dir, max_entries)?;

    Ok(ProjectTreeChildrenResponse {
        path: dir_path,
        children,
        truncated,
    })
}

#[tauri::command]
pub async fn search_project_files(
    path: String,
    query: String,
    max_results: Option<usize>,
) -> Result<Vec<ProjectFileSearchItem>, String> {
    let root = PathBuf::from(&path);
    if !root.exists() {
        return Err(format!("Path does not exist: {}", path));
    }
    if !root.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }

    let normalized_query = query.trim().to_string();
    if normalized_query.is_empty() {
        return Ok(Vec::new());
    }

    let max_results = max_results.unwrap_or(200).max(20);
    let mut matches: Vec<(i32, ProjectFileSearchItem)> = Vec::new();

    for entry in walkdir::WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }

            entry
                .file_name()
                .to_str()
                .map(|name| !should_ignore(name))
                .unwrap_or(false)
        })
        .filter_map(Result::ok)
    {
        if entry.depth() == 0 {
            continue;
        }

        let entry_path = entry.path();
        let relative_path = entry_path
            .strip_prefix(&root)
            .unwrap_or(entry_path)
            .to_string_lossy()
            .replace('\\', "/");

        let name = entry.file_name().to_string_lossy().to_string();
        let search_text = format!("{} {}", name, relative_path);
        let Some(score) = fuzzy_match_score(&search_text, &normalized_query) else {
            continue;
        };

        matches.push((
            score,
            ProjectFileSearchItem {
                name,
                path: relative_path,
                is_dir: entry.file_type().is_dir(),
            },
        ));
    }

    matches.sort_by(|(score_a, item_a), (score_b, item_b)| {
        score_b
            .cmp(score_a)
            .then_with(|| item_a.path.len().cmp(&item_b.path.len()))
            .then_with(|| item_a.path.cmp(&item_b.path))
    });

    matches.truncate(max_results);

    Ok(matches.into_iter().map(|(_, item)| item).collect())
}

/// Get the size of a file at the given path
#[tauri::command]
pub async fn get_file_size(path: String) -> Result<u64, String> {
    let metadata = fs::metadata(&path).map_err(|e| format!("Cannot read file metadata: {}", e))?;
    Ok(metadata.len())
}

/// Save a file to a temp directory and return its path.
/// Uses a unique suffix to avoid name collisions (e.g. multiple pasted images all named "image.png").
///
/// # Arguments
/// * `name` - The original filename
/// * `data` - The file content as bytes
/// * `cwd` - Optional working directory. If provided, saves inside {cwd}/.claude/tmp/
///          so Claude CLI can access the file. Otherwise uses system temp directory.
#[tauri::command]
pub async fn save_temp_file(
    name: String,
    data: Vec<u8>,
    cwd: Option<String>,
) -> Result<String, String> {
    // If a working directory is provided, save inside it so Claude CLI can access the file.
    // Falls back to system temp if cwd is not set.
    let tmp_dir = if let Some(ref dir) = cwd {
        let p = PathBuf::from(dir).join(".claude").join("tmp");
        if fs::create_dir_all(&p).is_ok() {
            // Ensure .claude is gitignored in user's project
            let gitignore = PathBuf::from(dir).join(".claude").join(".gitignore");
            if !gitignore.exists() {
                let _ = fs::write(&gitignore, "*\n");
            }
            p
        } else {
            // Fallback to system temp
            std::env::temp_dir()
        }
    } else {
        std::env::temp_dir()
    };

    // Generate unique filename with timestamp and counter
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Extract base name and extension
    let file_path = PathBuf::from(&name);
    let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");

    // Build unique filename: {stem}_{timestamp}.{extension}
    let unique_name = if !extension.is_empty() {
        format!("{}_{}.{}", stem, timestamp, extension)
    } else {
        format!("{}_{}", stem, timestamp)
    };

    let final_path = tmp_dir.join(&unique_name);

    // Write the file
    fs::write(&final_path, &data).map_err(|e| format!("Failed to write file: {}", e))?;

    // Return the absolute path as string
    final_path
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Failed to convert path to string".to_string())
}

/// Copy a file from source to destination
#[tauri::command]
pub async fn copy_file(src: String, dest: String) -> Result<(), String> {
    fs::copy(&src, &dest)
        .map_err(|e| format!("Failed to copy file from {} to {}: {}", src, dest, e))?;
    Ok(())
}

/// Check if a file exists and is accessible
#[tauri::command]
pub async fn check_file_access(path: String) -> Result<bool, String> {
    Ok(PathBuf::from(&path).exists())
}

/// Read a file and return its content as base64
#[tauri::command]
pub async fn read_file_base64(path: String) -> Result<String, String> {
    let data = fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    use base64::prelude::*;
    Ok(BASE64_STANDARD.encode(&data))
}

#[tauri::command]
pub async fn read_project_file(
    root_path: String,
    file_path: String,
) -> Result<ProjectFileResponse, String> {
    let root = PathBuf::from(&root_path);
    if !root.exists() || !root.is_dir() {
        return Err("Project root does not exist".to_string());
    }

    let resolved = resolve_project_path(&root, &file_path)?;
    let metadata =
        fs::metadata(&resolved).map_err(|e| format!("Failed to read file metadata: {}", e))?;
    if metadata.is_dir() {
        return Err("Cannot open a directory in the editor".to_string());
    }
    if metadata.len() > 1024 * 1024 {
        return Err("File is too large to edit inline (max 1 MB)".to_string());
    }

    let bytes = fs::read(&resolved).map_err(|e| format!("Failed to read file: {}", e))?;
    let content = String::from_utf8(bytes)
        .map_err(|_| "This file is not UTF-8 text and cannot be edited inline".to_string())?;

    Ok(ProjectFileResponse {
        path: file_path,
        content,
        size: metadata.len(),
    })
}

#[tauri::command]
pub async fn write_project_file(
    root_path: String,
    file_path: String,
    content: String,
) -> Result<ProjectFileResponse, String> {
    let root = PathBuf::from(&root_path);
    if !root.exists() || !root.is_dir() {
        return Err("Project root does not exist".to_string());
    }

    let resolved = resolve_project_path(&root, &file_path)?;
    let metadata =
        fs::metadata(&resolved).map_err(|e| format!("Failed to read file metadata: {}", e))?;
    if metadata.is_dir() {
        return Err("Cannot save to a directory".to_string());
    }

    fs::write(&resolved, content.as_bytes()).map_err(|e| format!("Failed to write file: {}", e))?;

    let updated =
        fs::metadata(&resolved).map_err(|e| format!("Failed to read updated metadata: {}", e))?;

    Ok(ProjectFileResponse {
        path: file_path,
        content,
        size: updated.len(),
    })
}

/// Ensure the ~/.aite directory exists
#[tauri::command]
pub async fn ensure_data_dir() -> Result<(), String> {
    let data_dir = get_data_dir()?;

    if !data_dir.exists() {
        fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data directory: {}", e))?;
        tracing::info!("[Files] Created data directory: {:?}", data_dir);
    }

    Ok(())
}

/// Get the full path for a data file in ~/.aite directory
#[tauri::command]
pub async fn get_data_file_path(filename: String) -> Result<String, String> {
    let data_dir = get_data_dir()?;
    let file_path = data_dir.join(&filename);

    // Ensure the directory exists
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data directory: {}", e))?;
    }

    let path_str = file_path
        .to_str()
        .ok_or_else(|| "Failed to convert path to string".to_string())?;

    tracing::info!("[Files] Data file path: {}", path_str);

    Ok(path_str.to_string())
}

fn get_data_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Failed to get home directory".to_string())?;
    Ok(home.join(".aite"))
}

#[tauri::command]
pub async fn read_data_file(filename: String) -> Result<Option<String>, String> {
    let data_dir = get_data_dir()?;
    let file_path = data_dir.join(&filename);

    if !file_path.exists() {
        return Ok(None);
    }

    let content =
        fs::read_to_string(&file_path).map_err(|e| format!("Failed to read data file: {}", e))?;

    Ok(Some(content))
}

#[tauri::command]
pub async fn backup_data_file(filename: String) -> Result<Option<String>, String> {
    let data_dir = get_data_dir()?;
    let file_path = data_dir.join(&filename);

    if !file_path.exists() {
        return Ok(None);
    }

    let metadata = fs::metadata(&file_path)
        .map_err(|e| format!("Failed to read data file metadata: {}", e))?;
    if metadata.len() == 0 {
        return Ok(None);
    }

    let backup_path = data_dir.join(format!("{}.bak", filename));
    fs::copy(&file_path, &backup_path).map_err(|e| format!("Failed to backup data file: {}", e))?;

    let path_str = backup_path
        .to_str()
        .ok_or_else(|| "Failed to convert backup path to string".to_string())?;

    Ok(Some(path_str.to_string()))
}

#[tauri::command]
pub async fn append_data_log(filename: String, content: String) -> Result<String, String> {
    let data_dir = get_data_dir()?;
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data directory: {}", e))?;
    }

    let file_path = data_dir.join(&filename);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .map_err(|e| format!("Failed to open data log file: {}", e))?;

    writeln!(file, "{}", content).map_err(|e| format!("Failed to append data log: {}", e))?;

    let path_str = file_path
        .to_str()
        .ok_or_else(|| "Failed to convert log path to string".to_string())?;

    tracing::warn!("[Files] Appended data log to {}", path_str);

    Ok(path_str.to_string())
}
