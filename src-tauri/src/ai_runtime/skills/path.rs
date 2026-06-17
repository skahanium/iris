use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

use super::model_impl::{SkillScope, SkillsConfig};

pub(crate) fn global_skills_dir() -> PathBuf {
    global_skills_dir_from_env(
        std::env::var("HOME").ok().as_deref(),
        std::env::var("USERPROFILE").ok().as_deref(),
    )
}

fn global_skills_dir_from_env(home: Option<&str>, user_profile: Option<&str>) -> PathBuf {
    if let Some(home) = home.filter(|value| !value.trim().is_empty()) {
        return PathBuf::from(home).join(".iris").join("skills");
    }
    if let Some(user_profile) = user_profile.filter(|value| !value.trim().is_empty()) {
        return PathBuf::from(user_profile).join(".iris").join("skills");
    }
    PathBuf::from(".iris").join("skills")
}

pub(crate) fn vault_skills_dir(vault: &Path) -> PathBuf {
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

pub(super) fn load_config(scope: SkillScope, vault: &Path) -> SkillsConfig {
    let path = config_path(scope, vault);
    if !path.exists() {
        return SkillsConfig::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(super) fn save_config(scope: SkillScope, vault: &Path, config: &SkillsConfig) -> AppResult<()> {
    let path = config_path(scope, vault);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json)?;
    Ok(())
}

pub(super) fn skill_key(scope: SkillScope, name: &str) -> String {
    format!("{:?}:{name}", scope)
}

/// Reject subpaths that attempt directory traversal or are absolute.
pub(super) fn validate_subpath(subpath: &str) -> AppResult<()> {
    use std::path::Component;
    for component in std::path::Path::new(subpath).components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => {
                return Err(AppError::msg(format!(
                    "invalid subpath: must be a relative path inside the repo ({subpath})"
                )));
            }
        }
    }
    Ok(())
}

/// Copy `src` into `dest` atomically: write to a sibling temp directory first,
/// then rename. This prevents half-written skill directories on error.
pub(super) fn atomic_copy_dir(src: &Path, dest: &Path) -> AppResult<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| AppError::msg("invalid destination parent"))?;
    let staging = parent.join(format!(
        ".tmp-{}",
        dest.file_name().unwrap_or_default().to_string_lossy()
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    copy_dir_recursive(src, &staging)?;
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::rename(&staging, dest)?;
    Ok(())
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

pub(super) fn slugify(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_skills_dir_uses_userprofile_when_home_is_empty() {
        let path = global_skills_dir_from_env(Some(""), Some(r"C:\Users\Iris"));
        let normalized = path.to_string_lossy().replace('/', "\\");

        assert_eq!(normalized, r"C:\Users\Iris\.iris\skills");
    }

    #[test]
    fn global_skills_dir_prefers_home_when_available() {
        let path = global_skills_dir_from_env(Some("/home/iris"), Some(r"C:\Users\Iris"));

        assert_eq!(path, PathBuf::from("/home/iris/.iris/skills"));
    }
}
