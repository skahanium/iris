//! Claude-compatible SKILL.md registry for prompt injection.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ai_runtime::AiScene;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    Global,
    Vault,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub trigger: Option<String>,
    pub content: String,
    pub scope: SkillScope,
    pub source_url: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub enabled: bool,
    pub file_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SkillsConfig {
    disabled: Vec<String>,
}

fn global_skills_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".iris").join("skills");
    }
    PathBuf::from(".iris").join("skills")
}

fn vault_skills_dir(vault: &Path) -> PathBuf {
    vault.join(".iris").join("skills")
}

fn config_path(scope: SkillScope, vault: &Path) -> PathBuf {
    match scope {
        SkillScope::Global => global_skills_dir()
            .parent()
            .unwrap_or(Path::new("."))
            .join("skills-config.json"),
        SkillScope::Vault => vault.join(".iris").join("skills-config.json"),
    }
}

fn load_config(scope: SkillScope, vault: &Path) -> SkillsConfig {
    let path = config_path(scope, vault);
    if !path.exists() {
        return SkillsConfig::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(scope: SkillScope, vault: &Path, config: &SkillsConfig) -> AppResult<()> {
    let path = config_path(scope, vault);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json)?;
    Ok(())
}

fn skill_key(scope: SkillScope, name: &str) -> String {
    format!("{:?}:{name}", scope)
}

/// Scan global + vault skill directories.
pub fn scan_all(vault: &Path) -> AppResult<Vec<SkillEntry>> {
    let mut entries = Vec::new();
    let global_dir = global_skills_dir();
    if global_dir.is_dir() {
        scan_dir(&global_dir, SkillScope::Global, vault, &mut entries)?;
    }
    let vault_dir = vault_skills_dir(vault);
    if vault_dir.is_dir() {
        scan_dir(&vault_dir, SkillScope::Vault, vault, &mut entries)?;
    }
    Ok(entries)
}

fn scan_dir(
    dir: &Path,
    scope: SkillScope,
    vault: &Path,
    out: &mut Vec<SkillEntry>,
) -> AppResult<()> {
    let config = load_config(scope, vault);
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_file = path.join("SKILL.md");
        if skill_file.is_file() {
            if let Ok(mut skill) = load_skill(&skill_file, scope) {
                let key = skill_key(scope, &skill.name);
                skill.enabled = !config.disabled.contains(&key);
                out.push(skill);
            }
        }
    }
    Ok(())
}

/// Parse a single SKILL.md file.
pub fn load_skill(path: &Path, scope: SkillScope) -> AppResult<SkillEntry> {
    let raw = fs::read_to_string(path)?;
    let (meta, body) = parse_frontmatter(&raw);
    let name = meta
        .get("name")
        .cloned()
        .or_else(|| {
            body.lines()
                .find(|l| l.starts_with("# "))
                .map(|l| l.trim_start_matches("# ").trim().to_string())
        })
        .unwrap_or_else(|| {
            path.parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("skill")
                .to_string()
        });
    Ok(SkillEntry {
        name,
        description: meta.get("description").cloned().unwrap_or_default(),
        trigger: meta.get("trigger").cloned(),
        content: body.trim().to_string(),
        scope,
        source_url: meta.get("source_url").cloned(),
        version: meta.get("version").cloned(),
        author: meta.get("author").cloned(),
        enabled: true,
        file_path: path.to_string_lossy().into_owned(),
    })
}

fn parse_frontmatter(raw: &str) -> (std::collections::HashMap<String, String>, String) {
    let mut map = std::collections::HashMap::new();
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return (map, raw.to_string());
    }
    let rest = trimmed.trim_start_matches("---");
    let Some(end) = rest.find("\n---") else {
        return (map, raw.to_string());
    };
    let front = &rest[..end];
    let body = rest[end + 4..].trim_start();
    for line in front.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once(':') {
            let value = v.trim().trim_matches('"').to_string();
            map.insert(k.trim().to_string(), value);
        }
    }
    (map, body.to_string())
}

/// Install skill from HTTP(S) URL (raw SKILL.md or GitHub raw link).
pub async fn install_from_url(url: &str, scope: SkillScope, vault: &Path) -> AppResult<SkillEntry> {
    crate::security::ipc_policy::validate_skill_remote_url(url)?;
    let client = crate::network::cert_pinning::create_pinned_client()?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::msg(format!("download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::msg(format!("HTTP {}", resp.status())));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| AppError::msg(format!("read body: {e}")))?;

    let (meta, _) = parse_frontmatter(&body);
    let dir_name = meta
        .get("name")
        .map(|s| slugify(s))
        .unwrap_or_else(|| format!("skill-{}", chrono::Utc::now().timestamp()));

    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    fs::create_dir_all(&base)?;
    let target_dir = base.join(&dir_name);
    fs::create_dir_all(&target_dir)?;
    let skill_path = target_dir.join("SKILL.md");
    fs::write(&skill_path, &body)?;

    let mut entry = load_skill(&skill_path, scope)?;
    entry.source_url = Some(url.to_string());
    Ok(entry)
}

/// Shallow git clone and copy SKILL.md or skill directory.
pub async fn install_from_git(
    repo_url: &str,
    subpath: Option<&str>,
    scope: SkillScope,
    vault: &Path,
) -> AppResult<Vec<SkillEntry>> {
    crate::security::ipc_policy::validate_skill_git_url(repo_url)?;
    let tmp = std::env::temp_dir().join(format!("iris-skill-{}", uuid::Uuid::new_v4()));
    let status = std::process::Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            repo_url,
            tmp.to_str().unwrap_or(""),
        ])
        .status()
        .map_err(|e| AppError::msg(format!("git not available: {e}")))?;
    if !status.success() {
        return Err(AppError::msg("git clone failed"));
    }

    let src = subpath.map(|p| tmp.join(p)).unwrap_or_else(|| tmp.clone());

    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    fs::create_dir_all(&base)?;

    let mut installed = Vec::new();
    if src.join("SKILL.md").is_file() {
        let name = slugify(src.file_name().and_then(|s| s.to_str()).unwrap_or("skill"));
        let dest = base.join(name);
        copy_dir_recursive(&src, &dest)?;
        let skill_path = dest.join("SKILL.md");
        installed.push(load_skill(&skill_path, scope)?);
    } else if src.is_dir() {
        for entry in fs::read_dir(&src)? {
            let entry = entry?;
            let p = entry.path();
            if p.join("SKILL.md").is_file() {
                let name = p
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(slugify)
                    .unwrap_or_else(|| "skill".into());
                let dest = base.join(name);
                copy_dir_recursive(&p, &dest)?;
                installed.push(load_skill(&dest.join("SKILL.md"), scope)?);
            }
        }
    }

    let _ = fs::remove_dir_all(&tmp);
    if installed.is_empty() {
        return Err(AppError::msg("no SKILL.md found in repository"));
    }
    Ok(installed)
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> AppResult<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

pub fn uninstall(name: &str, scope: SkillScope, vault: &Path) -> AppResult<()> {
    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    let slug = slugify(name);
    let target = base.join(slug);
    if target.is_dir() {
        fs::remove_dir_all(target)?;
    }
    Ok(())
}

pub fn set_enabled(name: &str, scope: SkillScope, vault: &Path, enabled: bool) -> AppResult<()> {
    let mut config = load_config(scope, vault);
    let key = skill_key(scope, name);
    if enabled {
        config.disabled.retain(|k| k != &key);
    } else if !config.disabled.contains(&key) {
        config.disabled.push(key);
    }
    save_config(scope, vault, &config)
}

/// Filter enabled skills by optional trigger / scene affinity.
pub fn skills_for_scene(skills: &[SkillEntry], scene: AiScene) -> Vec<&SkillEntry> {
    let scene_key = scene.profile();
    skills
        .iter()
        .filter(|s| {
            if !s.enabled {
                return false;
            }
            match s.trigger.as_deref() {
                None | Some("") => true,
                Some(trigger) => {
                    let t = trigger.to_lowercase();
                    t.contains(scene_key)
                        || t.contains("writing") && scene_key.contains("drafting")
                        || t.contains("research") && scene_key.contains("research")
                        || t.contains("knowledge") && scene_key.contains("knowledge")
                }
            }
        })
        .collect()
}

/// Build system prompt fragment from enabled skills.
pub fn inject_into_prompt(skills: &[SkillEntry], scene: AiScene) -> String {
    let matched = skills_for_scene(skills, scene);
    if matched.is_empty() {
        return String::new();
    }
    let mut block = String::from("## 已激活 Skills\n\n");
    for skill in matched {
        block.push_str(&format!("### Skill: {}\n\n", skill.name));
        if !skill.description.is_empty() {
            block.push_str(&format!("_{}_\n\n", skill.description));
        }
        block.push_str(&skill.content);
        block.push_str("\n\n---\n\n");
    }
    block
}

/// Install SKILL.md from a local file path (copies into skills directory).
pub fn install_from_local(source: &Path, scope: SkillScope, vault: &Path) -> AppResult<SkillEntry> {
    let source = crate::security::ipc_policy::validate_local_skill_source(source, vault)?;
    if !source.is_file() {
        return Err(AppError::msg("本地安装需要 SKILL.md 文件路径"));
    }
    let body = fs::read_to_string(&source)?;
    let (meta, _) = parse_frontmatter(&body);
    let dir_name = meta
        .get("name")
        .cloned()
        .or_else(|| {
            source
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .map(slugify)
        })
        .unwrap_or_else(|| format!("skill-{}", uuid::Uuid::new_v4()));

    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    fs::create_dir_all(&base)?;
    let target_dir = base.join(&dir_name);
    fs::create_dir_all(&target_dir)?;
    let skill_path = target_dir.join("SKILL.md");
    fs::write(&skill_path, &body)?;

    let mut entry = load_skill(&skill_path, scope)?;
    entry.source_url = Some(source.to_string_lossy().into_owned());
    Ok(entry)
}

/// Validate that a path is under a known skills directory (global or vault).
pub fn validate_skill_path(path: &Path, vault: &Path) -> AppResult<()> {
    let canonical = path
        .canonicalize()
        .map_err(|_| AppError::msg("Skill 文件路径无效或不存在"))?;
    let global_dir = global_skills_dir();
    let vault_dir = vault_skills_dir(vault);

    let under_global = global_dir
        .canonicalize()
        .is_ok_and(|g| canonical.starts_with(&g));
    let under_vault = vault_dir
        .canonicalize()
        .is_ok_and(|v| canonical.starts_with(&v));

    if !under_global && !under_vault {
        return Err(AppError::msg("Skill 文件路径必须在已知的 skills 目录下"));
    }
    Ok(())
}

/// Read skill file content for editing.
pub fn read_skill_content(path: &Path) -> AppResult<String> {
    fs::read_to_string(path).map_err(Into::into)
}

/// Write updated skill content (must be `SKILL.md`).
pub fn write_skill_content(path: &Path, scope: SkillScope, content: &str) -> AppResult<SkillEntry> {
    if path.file_name().and_then(|n| n.to_str()) != Some("SKILL.md") {
        return Err(AppError::msg("只能写入 SKILL.md"));
    }
    fs::write(path, content)?;
    load_skill(path, scope)
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
