use std::fs;
use std::path::{Path, PathBuf};

use hex;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

use super::frontmatter_impl::parse_frontmatter;
use super::model_impl::{VALIDATION_MISSING_FRONTMATTER, VALIDATION_NAME_MISMATCH};
use super::path_impl::{global_skills_dir, load_config, skill_key, slugify, vault_skills_dir};
use super::validation_impl::{capability_preview_for_entry, confirmation_required_tools};
use super::{SkillEntry, SkillListEntry, SkillMetadata, SkillScope, SkillValidationStatus};

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

/// Scan global + vault skill directories but leave instruction bodies unloaded.
pub fn scan_all_metadata(vault: &Path) -> AppResult<Vec<SkillEntry>> {
    let mut entries = scan_all(vault)?;
    for entry in &mut entries {
        entry.content.clear();
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
///
/// Supports both new Agent Skills format (YAML frontmatter with `description`,
/// `allowed-tools`, etc.) and legacy format (with `trigger`).
pub fn load_skill(path: &Path, scope: SkillScope) -> AppResult<SkillEntry> {
    let raw = fs::read_to_string(path)?;
    let has_frontmatter = raw.trim_start().starts_with("---");
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

    let dir_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let normalized_name = slugify(&name);
    let normalized_dir = slugify(dir_name);
    let name_matches_dir = normalized_name == normalized_dir || dir_name.is_empty();

    let allowed_tools: Vec<String> = meta
        .get("allowed-tools")
        .or_else(|| meta.get("allowed_tools"))
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_default();

    let mut metadata: SkillMetadata = meta
        .get("metadata")
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    if !has_frontmatter {
        metadata.insert(
            VALIDATION_MISSING_FRONTMATTER.to_string(),
            serde_json::Value::Bool(true),
        );
    }

    let legacy_trigger = meta.get("trigger").cloned();

    let description = meta.get("description").cloned().unwrap_or_default();
    if description.len() > 1024 {
        return Err(AppError::msg(format!(
            "skill description exceeds 1024 chars (got {})",
            description.len()
        )));
    }
    let compatibility = meta.get("compatibility").cloned();
    if let Some(ref compat) = compatibility {
        if compat.len() > 500 {
            return Err(AppError::msg(format!(
                "skill compatibility exceeds 500 chars (got {})",
                compat.len()
            )));
        }
    }

    let mut entry = SkillEntry {
        name,
        description,
        license: meta.get("license").cloned(),
        compatibility,
        metadata,
        allowed_tools,
        content: body.trim().to_string(),
        scope,
        source_url: meta.get("source_url").cloned(),
        enabled: true,
        file_path: path.to_string_lossy().into_owned(),
        legacy_trigger,
    };

    if !name_matches_dir && !dir_name.is_empty() {
        entry.metadata.insert(
            VALIDATION_NAME_MISMATCH.to_string(),
            serde_json::Value::Bool(true),
        );
        tracing::warn!(
            skill = %entry.name,
            dir = %dir_name,
            "skill name does not match parent directory"
        );
    }

    if entry.description.is_empty() && !body.is_empty() {
        if let Some(first_para) = body
            .lines()
            .skip_while(|l| l.starts_with('#') || l.trim().is_empty())
            .take_while(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .into()
        {
            let desc: String = first_para;
            if desc.len() <= 1024 {
                entry.description = desc;
            }
        }
    }

    Ok(entry)
}

/// Compute a stable content hash for an installed SKILL.md file.
pub fn skill_content_hash_for_path(path: &Path) -> AppResult<String> {
    let raw = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(raw);
    Ok(hex::encode(hasher.finalize()))
}

/// Build the skills list with computed validation/dependency info.
pub fn scan_all_with_status(vault: &Path) -> AppResult<Vec<SkillListEntry>> {
    let skills = scan_all(vault)?;
    let installed_names: Vec<String> = skills.iter().map(|s| s.name.clone()).collect();
    Ok(skills
        .into_iter()
        .map(|skill| {
            let unrecognized_tools = skill.unrecognized_tools();
            let missing_deps = skill.missing_dependencies(&installed_names);
            let validation = skill.validation_status();
            let confirmation_required_tools = confirmation_required_tools(&skill.allowed_tools);
            let content_hash = skill_content_hash_for_path(&PathBuf::from(&skill.file_path)).ok();
            let capability_preview = capability_preview_for_entry(&skill, &installed_names);
            let availability = if !skill.enabled {
                "disabled"
            } else if matches!(validation, SkillValidationStatus::Invalid(_)) {
                "unavailable"
            } else if !unrecognized_tools.is_empty() || !missing_deps.is_empty() {
                "partial"
            } else {
                "available"
            }
            .to_string();
            SkillListEntry {
                skill,
                validation,
                unrecognized_tools,
                missing_deps,
                scene_active: None,
                scene_score: None,
                confirmation_required_tools,
                content_hash,
                capability_preview,
                availability,
            }
        })
        .collect())
}
