use std::fs;
use std::path::Path;

use crate::error::{AppError, AppResult};

use super::path_impl::{global_skills_dir, slugify, vault_skills_dir};
use super::SkillScope;

pub(crate) const ALLOWED_RESOURCE_DIRS: &[&str] = &["references", "resources", "assets"];
pub(crate) const MAX_SKILL_RESOURCE_CHARS: usize = 24_000;

/// Read a file under a skill's `references/`, `resources/`, or `assets/` directory.
pub fn read_skill_resource(
    vault: &Path,
    name: &str,
    scope: SkillScope,
    relative_path: &str,
) -> AppResult<String> {
    if relative_path.is_empty() || relative_path.contains("..") {
        return Err(AppError::msg("skill 资源路径无效"));
    }
    let rel = Path::new(relative_path.trim_start_matches('/'));
    if rel.is_absolute() {
        return Err(AppError::msg("skill 资源路径必须为相对路径"));
    }
    let top = rel
        .components()
        .next()
        .and_then(|c| c.as_os_str().to_str())
        .ok_or_else(|| AppError::msg("skill 资源路径无效"))?;
    if !ALLOWED_RESOURCE_DIRS.contains(&top) {
        return Err(AppError::msg(format!(
            "仅允许读取 {ALLOWED_RESOURCE_DIRS:?} 下的资源"
        )));
    }

    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    let skill_root = base.join(slugify(name));
    if !skill_root.is_dir() {
        return Err(AppError::msg(format!("未找到 skill: {name}")));
    }

    let target = skill_root.join(rel);
    let root_canonical = skill_root
        .canonicalize()
        .map_err(|_| AppError::msg("skill 目录无效"))?;
    let file_canonical = target
        .canonicalize()
        .map_err(|_| AppError::msg("skill 资源文件不存在"))?;
    if !file_canonical.starts_with(&root_canonical) {
        return Err(AppError::msg("skill 资源路径越界"));
    }
    if !file_canonical.is_file() {
        return Err(AppError::msg("skill 资源必须是文件"));
    }

    let content = fs::read_to_string(&file_canonical)?;
    Ok(content.chars().take(MAX_SKILL_RESOURCE_CHARS).collect())
}
