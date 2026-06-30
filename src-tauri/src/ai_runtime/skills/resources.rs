use std::fs;
use std::path::Path;

use crate::error::{AppError, AppResult};

use super::manifest_impl::load_manifest_for_skill_dir;
use super::path_impl::{global_skills_dir, slugify, vault_skills_dir};
use super::{SkillEntry, SkillScope};

pub(crate) const ALLOWED_RESOURCE_DIRS: &[&str] = &["references", "resources", "assets"];
pub(crate) const MAX_SKILL_RESOURCE_CHARS: usize = 24_000;

fn dedupe_resource_paths(paths: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for path in paths.into_iter().map(|path| path.trim().to_string()) {
        if !path.is_empty() && !deduped.contains(&path) {
            deduped.push(path);
        }
    }
    deduped
}

pub(super) fn effective_required_resources_for_skill(skill: &SkillEntry) -> Vec<String> {
    let mut resources = skill.required_resources();
    if let Some(skill_root) = Path::new(&skill.file_path).parent() {
        if let Ok(outcome) = load_manifest_for_skill_dir(skill_root, None) {
            resources.extend(outcome.manifest.resources.required);
        }
    }
    dedupe_resource_paths(resources)
}

pub(super) fn effective_optional_resources_for_skill(skill: &SkillEntry) -> Vec<String> {
    let mut resources = skill.optional_resources();
    if let Some(skill_root) = Path::new(&skill.file_path).parent() {
        if let Ok(outcome) = load_manifest_for_skill_dir(skill_root, None) {
            resources.extend(outcome.manifest.resources.optional);
        }
    }
    dedupe_resource_paths(resources)
}
/// Read a file under a skill's `references/`, `resources/`, or `assets/` directory.
pub fn read_skill_resource(
    vault: &Path,
    name: &str,
    scope: SkillScope,
    relative_path: &str,
) -> AppResult<String> {
    if relative_path.is_empty() || relative_path.contains("..") {
        return Err(AppError::msg("invalid skill resource path"));
    }
    let rel = Path::new(relative_path.trim_start_matches('/'));
    if rel.is_absolute() {
        return Err(AppError::msg("skill resource path must be relative"));
    }
    let top = rel
        .components()
        .next()
        .and_then(|c| c.as_os_str().to_str())
        .ok_or_else(|| AppError::msg("invalid skill resource path"))?;
    if !ALLOWED_RESOURCE_DIRS.contains(&top) {
        return Err(AppError::msg(format!(
            "only {ALLOWED_RESOURCE_DIRS:?} skill resource dirs are readable"
        )));
    }

    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    let skill_root = base.join(slugify(name));
    if !skill_root.is_dir() {
        return Err(AppError::msg(format!("skill not found: {name}")));
    }

    let target = skill_root.join(rel);
    let root_canonical = skill_root
        .canonicalize()
        .map_err(|_| AppError::msg("invalid skill directory"))?;
    let file_canonical = target
        .canonicalize()
        .map_err(|_| AppError::msg("skill resource file does not exist"))?;
    if !file_canonical.starts_with(&root_canonical) {
        return Err(AppError::msg("skill resource path escaped root"));
    }
    if !file_canonical.is_file() {
        return Err(AppError::msg("skill resource must be a file"));
    }

    let content = fs::read_to_string(&file_canonical)?;
    Ok(content.chars().take(MAX_SKILL_RESOURCE_CHARS).collect())
}
