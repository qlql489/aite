// Skills management commands
// 区分 commands 和 skills:
// - ~/.claude/commands/ - Global commands (source: "global")
// - ./.claude/commands/ - Project commands (source: "project")
// - ~/.claude/skills/ - Global skills (source: "global")
// - ~/.agents/skills/ - Installed skills (source: "installed")
// - ~/.claude/plugins/marketplaces/*/plugins/*/commands/ - Plugin commands (source: "plugin")

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    pub name: String,
    pub description: String,
    pub content: String,
    #[serde(rename = "source")]
    pub source_type: String, // "global" | "project" | "installed" | "plugin"
    #[serde(rename = "installedSource")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_source: Option<String>,
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "pluginName")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_name: Option<String>,
}

fn get_home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

fn get_workspace_root_dir(workspace_path: Option<&str>) -> PathBuf {
    workspace_path
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

// Global commands: ~/.claude/commands/
fn get_global_commands_dir() -> PathBuf {
    get_home_dir()
        .map(|p| p.join(".claude").join("commands"))
        .unwrap_or_else(|| PathBuf::from(".claude/commands"))
}

// Project commands: ./.claude/commands/
fn get_project_commands_dir(workspace_path: Option<&str>) -> PathBuf {
    get_workspace_root_dir(workspace_path)
        .join(".claude")
        .join("commands")
}

// Global skills: ~/.claude/skills/
fn get_global_skills_dir() -> PathBuf {
    get_home_dir()
        .map(|p| p.join(".claude").join("skills"))
        .unwrap_or_else(|| PathBuf::from(".claude/skills"))
}

// Project skills: ./.claude/skills/
fn get_project_skills_dir(workspace_path: Option<&str>) -> PathBuf {
    get_workspace_root_dir(workspace_path)
        .join(".claude")
        .join("skills")
}

// Installed skills: ~/.agents/skills/
fn get_installed_skills_dir() -> PathBuf {
    get_home_dir()
        .map(|p| p.join(".agents").join("skills"))
        .unwrap_or_else(|| PathBuf::from(".agents/skills"))
}

// Plugin commands: ~/.claude/plugins/marketplaces/*/plugins/*/commands/
// 返回 (commands_dir, plugin_name) 元组
fn get_plugin_commands_dirs() -> Vec<(PathBuf, String)> {
    let mut dirs = Vec::new();

    if let Some(home) = get_home_dir() {
        let marketplaces_dir = home.join(".claude").join("plugins").join("marketplaces");
        if !marketplaces_dir.exists() {
            return dirs;
        }

        if let Ok(marketplaces) = fs::read_dir(&marketplaces_dir) {
            for marketplace in marketplaces.flatten() {
                let plugins_dir = marketplace.path().join("plugins");
                if !plugins_dir.exists() {
                    continue;
                }

                if let Ok(plugins) = fs::read_dir(&plugins_dir) {
                    for plugin in plugins.flatten() {
                        let commands_dir = plugin.path().join("commands");
                        if commands_dir.exists() {
                            let plugin_name = plugin.file_name().to_string_lossy().to_string();
                            dirs.push((commands_dir, plugin_name));
                        }
                    }
                }
            }
        }
    }

    dirs
}

fn scan_directory(dir: &PathBuf, source: &str, plugin_name: Option<&str>) -> Vec<SkillFile> {
    let mut skills = Vec::new();
    if !dir.exists() {
        return skills;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            if file_name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                // Recurse into subdirectories (e.g., commands/review/pr.md)
                skills.extend(scan_directory(&path, source, plugin_name));
                continue;
            }

            if file_name.ends_with(".md") {
                let fallback_name = file_name.trim_end_matches(".md").to_string();
                if let Ok(content) = fs::read_to_string(&path) {
                    let name = parse_skill_name(&content, fallback_name);
                    let description = parse_skill_description(&content);

                    skills.push(SkillFile {
                        name,
                        description,
                        content,
                        source_type: source.to_string(),
                        installed_source: None,
                        file_path: path.to_string_lossy().to_string(),
                        plugin_name: plugin_name.map(|s| s.to_string()),
                    });
                }
            }
        }
    }

    skills
}

// Scan installed skills: ~/.agents/skills/*/SKILL.md 或 ~/.claude/skills/*/SKILL.md
fn scan_installed_skills(dir: &PathBuf, installed_source: &str) -> Vec<SkillFile> {
    let mut skills = Vec::new();
    if !dir.exists() {
        return skills;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let dir_name = entry.file_name().to_string_lossy().to_string();

            if dir_name.starts_with('.') || !path.is_dir() {
                continue;
            }

            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                if let Ok(content) = fs::read_to_string(&skill_md) {
                    let description = parse_skill_description(&content);
                    let name = parse_skill_name(&content, dir_name.clone());

                    skills.push(SkillFile {
                        name,
                        description,
                        content,
                        source_type: "installed".to_string(),
                        installed_source: Some(installed_source.to_string()),
                        file_path: skill_md.to_string_lossy().to_string(),
                        plugin_name: None,
                    });
                }
            }
        }
    }

    skills
}

fn scan_skills_directory(dir: &PathBuf, source: &str) -> Vec<SkillFile> {
    let mut skills = Vec::new();
    if !dir.exists() {
        return skills;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let entry_name = entry.file_name().to_string_lossy().to_string();

            if entry_name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    if let Ok(content) = fs::read_to_string(&skill_md) {
                        let description = parse_skill_description(&content);
                        let name = parse_skill_name(&content, entry_name.clone());

                        skills.push(SkillFile {
                            name,
                            description,
                            content,
                            source_type: source.to_string(),
                            installed_source: None,
                            file_path: skill_md.to_string_lossy().to_string(),
                            plugin_name: None,
                        });
                    }
                } else {
                    skills.extend(scan_skills_directory(&path, source));
                }
                continue;
            }

            if entry_name.ends_with(".md") {
                let fallback_name = entry_name.trim_end_matches(".md").to_string();
                if let Ok(content) = fs::read_to_string(&path) {
                    let name = parse_skill_name(&content, fallback_name);
                    let description = parse_skill_description(&content);

                    skills.push(SkillFile {
                        name,
                        description,
                        content,
                        source_type: source.to_string(),
                        installed_source: None,
                        file_path: path.to_string_lossy().to_string(),
                        plugin_name: None,
                    });
                }
            }
        }
    }

    skills
}

fn parse_skill_name(content: &str, fallback: String) -> String {
    // Extract name from front matter between --- delimiters
    if let Some(fm_match) = content.strip_prefix("---") {
        if let Some(end_idx) = fm_match.find("---") {
            let front_matter = &fm_match[..end_idx];
            for line in front_matter.lines() {
                if line.starts_with("name:") {
                    return line.trim_start_matches("name:").trim().to_string();
                }
            }
        }
    }
    fallback
}

fn parse_skill_description(content: &str) -> String {
    // Extract front matter between --- delimiters
    if let Some(fm_match) = content.strip_prefix("---") {
        if let Some(end_idx) = fm_match.find("---") {
            let front_matter = &fm_match[..end_idx];
            for line in front_matter.lines() {
                if line.starts_with("name:") {
                    continue;
                }
                if line.starts_with("description:") {
                    let desc = line.trim_start_matches("description:").trim();
                    if desc.starts_with('|') {
                        // Multi-line, get next lines
                        let mut desc_lines = Vec::new();
                        let rest = &fm_match[end_idx + 3..];
                        for l in rest.lines() {
                            if l.starts_with("```") || l.starts_with("---") {
                                break;
                            }
                            desc_lines.push(l.trim());
                        }
                        return desc_lines.join(" ");
                    }
                    return desc.to_string();
                }
            }
        }
    }
    // Fallback: use first heading
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            return trimmed.trim_start_matches('#').trim().to_string();
        }
    }
    "No description".to_string()
}

fn sanitize_folder_name(name: &str) -> String {
    let sanitized = name
        .trim()
        .chars()
        .map(|char| match char {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => char,
            _ => '-',
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if sanitized.is_empty() {
        "imported-skill".to_string()
    } else {
        sanitized
    }
}

fn copy_skill_directory(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|e| e.to_string())?;

    let entries = fs::read_dir(source).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let entry_name = entry.file_name().to_string_lossy().to_string();
        if entry_name.starts_with('.') || entry_name == "__MACOSX" {
            continue;
        }

        let source_path = entry.path();
        let target_path = target.join(&entry_name);
        let metadata = entry.metadata().map_err(|e| e.to_string())?;

        if metadata.is_dir() {
            copy_skill_directory(&source_path, &target_path)?;
        } else {
            fs::copy(&source_path, &target_path).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllSkillsResponse {
    pub commands: Vec<SkillFile>,
    pub skills: Vec<SkillFile>,
}

#[tauri::command]
pub fn get_skills(workspace_path: Option<String>) -> Result<AllSkillsResponse, String> {
    if let Some(workspace_path) = workspace_path {
        let commands_dir = get_project_commands_dir(Some(&workspace_path));
        let skills_dir = get_project_skills_dir(Some(&workspace_path));

        return Ok(AllSkillsResponse {
            commands: scan_directory(&commands_dir, "project", None),
            skills: scan_skills_directory(&skills_dir, "project"),
        });
    }

    // Commands
    let global_commands_dir = get_global_commands_dir();
    let project_commands_dir = get_project_commands_dir(None);
    let plugin_commands_dirs = get_plugin_commands_dirs();

    let global_commands = scan_directory(&global_commands_dir, "global", None);
    let project_commands = scan_directory(&project_commands_dir, "project", None);

    let mut plugin_commands = Vec::new();
    for (dir, plugin_name) in plugin_commands_dirs {
        plugin_commands.extend(scan_directory(&dir, "plugin", Some(&plugin_name)));
    }

    let mut all_commands = Vec::new();
    all_commands.extend(global_commands);
    all_commands.extend(project_commands);
    all_commands.extend(plugin_commands);

    // Skills
    let global_skills_dir = get_global_skills_dir();
    let global_skills = scan_skills_directory(&global_skills_dir, "global");

    let mut all_skills = Vec::new();
    all_skills.extend(global_skills);

    Ok(AllSkillsResponse {
        commands: all_commands,
        skills: all_skills,
    })
}

#[tauri::command]
pub fn get_skill_content(file_path: String) -> Result<String, String> {
    fs::read_to_string(&file_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_skill(file_path: String, content: String) -> Result<(), String> {
    fs::write(&file_path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_skill(
    name: String,
    content: String,
    scope: String,
    workspace_path: Option<String>,
) -> Result<SkillFile, String> {
    // skill 存储在 ~/.claude/skills/ (global) 或 ./.claude/skills/ (project)
    let dir = if scope == "global" {
        get_global_skills_dir()
    } else {
        get_project_skills_dir(workspace_path.as_deref())
    };

    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    // Skills 存储为目录，每个 skill 一个目录，包含 SKILL.md
    let skill_dir = dir.join(&name);
    if skill_dir.exists() {
        return Err("A skill with this name already exists".to_string());
    }

    fs::create_dir_all(&skill_dir).map_err(|e| e.to_string())?;

    let file_path = skill_dir.join("SKILL.md");
    fs::write(&file_path, &content).map_err(|e| e.to_string())?;

    let description = parse_skill_description(&content);

    Ok(SkillFile {
        name,
        description,
        content,
        source_type: scope,
        installed_source: None,
        file_path: file_path.to_string_lossy().to_string(),
        plugin_name: None,
    })
}

#[tauri::command]
pub fn import_skill_folder(
    folder_path: String,
    scope: String,
    workspace_path: Option<String>,
) -> Result<SkillFile, String> {
    let source_dir = PathBuf::from(&folder_path);
    if !source_dir.exists() {
        return Err("指定的文件夹不存在".to_string());
    }

    if !source_dir.is_dir() {
        return Err("指定的路径不是文件夹".to_string());
    }

    let source_skill_file = source_dir.join("SKILL.md");
    if !source_skill_file.exists() {
        return Err("文件夹中未找到 SKILL.md 文件".to_string());
    }

    let dir = if scope == "global" {
        get_global_skills_dir()
    } else {
        get_project_skills_dir(workspace_path.as_deref())
    };

    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let content = fs::read_to_string(&source_skill_file).map_err(|e| e.to_string())?;
    let fallback_name = source_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("imported-skill")
        .to_string();

    let folder_name = sanitize_folder_name(&parse_skill_name(&content, fallback_name));
    let target_dir = dir.join(&folder_name);
    if target_dir.exists() {
        return Err(format!("技能 \"{}\" 已存在", folder_name));
    }

    copy_skill_directory(&source_dir, &target_dir)?;

    let file_path = target_dir.join("SKILL.md");
    let copied_content = fs::read_to_string(&file_path).map_err(|e| e.to_string())?;
    let name = parse_skill_name(&copied_content, folder_name.clone());
    let description = parse_skill_description(&copied_content);

    Ok(SkillFile {
        name,
        description,
        content: copied_content,
        source_type: scope,
        installed_source: None,
        file_path: file_path.to_string_lossy().to_string(),
        plugin_name: None,
    })
}

#[tauri::command]
pub fn create_command(
    name: String,
    content: String,
    scope: String,
    workspace_path: Option<String>,
) -> Result<SkillFile, String> {
    let dir = if scope == "global" {
        get_global_commands_dir()
    } else {
        get_project_commands_dir(workspace_path.as_deref())
    };

    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    let file_path = dir.join(format!("{}.md", name));
    if file_path.exists() {
        return Err("A command with this name already exists".to_string());
    }

    fs::write(&file_path, &content).map_err(|e| e.to_string())?;

    let description = parse_skill_description(&content);

    Ok(SkillFile {
        name,
        description,
        content,
        source_type: scope,
        installed_source: None,
        file_path: file_path.to_string_lossy().to_string(),
        plugin_name: None,
    })
}

#[tauri::command]
pub fn delete_skill(file_path: String) -> Result<(), String> {
    let path = PathBuf::from(&file_path);
    // If it's a file, delete it; if it's a SKILL.md, delete the parent directory
    if path.file_name().map(|s| s == "SKILL.md").unwrap_or(false) {
        if let Some(parent) = path.parent() {
            fs::remove_dir_all(parent).map_err(|e| e.to_string())?;
        }
    } else {
        fs::remove_file(&file_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
