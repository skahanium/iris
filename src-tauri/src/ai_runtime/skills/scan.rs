use std::fs;
use std::path::{Component, Path, PathBuf};

use hex;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

use super::compatibility_impl::blocked_capabilities_for_skill;
use super::frontmatter_impl::parse_frontmatter;
use super::manifest_impl::{load_manifest_for_skill_dir, SkillManifestKind};
use super::model_impl::{VALIDATION_MISSING_FRONTMATTER, VALIDATION_NAME_MISMATCH};
use super::path_impl::{global_skills_dir, load_config, skill_key, slugify, vault_skills_dir};
use super::resources_impl::{
    effective_optional_resources_for_skill, effective_required_resources_for_skill,
    ALLOWED_RESOURCE_DIRS,
};
use super::validation_impl::{capability_preview_for_entry, confirmation_required_tools};
use super::workspace_impl::effective_workspace_manifest_for_skill;
use super::{
    list_workspace_files, workspace_status_for_skill, SkillEntry, SkillListEntry, SkillMetadata,
    SkillScope, SkillValidationStatus,
};

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
    for key in [
        "trigger-hints",
        "trigger_hints",
        "requested-capabilities",
        "requested_capabilities",
        "required-resources",
        "required_resources",
        "optional-resources",
        "optional_resources",
        "sandbox",
        "external-dependencies",
        "external_dependencies",
        "compatibility-source",
        "compatibility_source",
        "iris-workspace",
        "iris_workspace",
    ] {
        if let Some(value) = meta.get(key) {
            metadata.insert(
                key.to_string(),
                serde_json::from_str(value)
                    .unwrap_or_else(|_| serde_json::Value::String(value.clone())),
            );
        }
    }
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

fn prompt_section_status_for_manifest(
    manifest: Option<&super::manifest_impl::IrisSkillManifest>,
    runtime_ready: bool,
    workspace_prepared: bool,
    unavailable_resources: &[String],
) -> (Vec<String>, Vec<String>) {
    let Some(manifest) = manifest else {
        return (vec!["skill_overlay".into()], Vec::new());
    };
    if manifest.prompt.sections.is_empty() {
        let mut blocked = Vec::new();
        if !runtime_ready {
            blocked.push("runtime".into());
        }
        if !workspace_prepared {
            blocked.push("workspace".into());
        }
        if !unavailable_resources.is_empty() {
            blocked.push("resources".into());
        }
        return (vec!["skill_overlay".into()], blocked);
    }

    let selected: Vec<String> = if manifest.prompt.default_sections.is_empty() {
        manifest
            .prompt
            .sections
            .iter()
            .map(|section| section.id.clone())
            .collect()
    } else {
        manifest.prompt.default_sections.clone()
    };
    let mut activated = Vec::new();
    let mut blocked = Vec::new();
    for section_id in selected {
        let Some(section) = manifest
            .prompt
            .sections
            .iter()
            .find(|section| section.id == section_id)
        else {
            continue;
        };
        if (section.requires_runtime && !runtime_ready)
            || (section.requires_workspace && !workspace_prepared)
            || section
                .requires_resources
                .iter()
                .any(|resource| unavailable_resources.contains(resource))
        {
            blocked.push(section.id.clone());
        } else {
            activated.push(section.id.clone());
        }
    }
    if activated.is_empty() && blocked.is_empty() {
        activated.push("skill_overlay".into());
    }
    (activated, blocked)
}

fn skill_resource_available(skill_root: Option<&Path>, relative_path: &str) -> bool {
    let Some(skill_root) = skill_root else {
        return false;
    };
    if relative_path.trim().is_empty() || relative_path.contains("..") {
        return false;
    }
    let rel = Path::new(relative_path.trim_start_matches('/'));
    if rel.is_absolute()
        || rel.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return false;
    }
    let Some(top) = rel.components().next().and_then(|c| c.as_os_str().to_str()) else {
        return false;
    };
    if !ALLOWED_RESOURCE_DIRS.contains(&top) {
        return false;
    }
    let Ok(root) = skill_root.canonicalize() else {
        return false;
    };
    let Ok(candidate) = skill_root.join(rel).canonicalize() else {
        return false;
    };
    candidate.starts_with(root) && candidate.is_file()
}

fn missing_resource_paths(skill: &SkillEntry, resources: Vec<String>) -> Vec<String> {
    let skill_root = Path::new(&skill.file_path).parent();
    resources
        .into_iter()
        .filter(|relative_path| !skill_resource_available(skill_root, relative_path))
        .collect()
}

fn missing_section_resource_paths(
    skill: &SkillEntry,
    manifest: Option<&super::manifest_impl::IrisSkillManifest>,
) -> Vec<String> {
    let Some(manifest) = manifest else {
        return Vec::new();
    };
    let resources = manifest
        .prompt
        .sections
        .iter()
        .flat_map(|section| section.requires_resources.iter().cloned())
        .collect::<Vec<_>>();
    let mut missing = missing_resource_paths(skill, resources);
    missing.sort();
    missing.dedup();
    missing
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
            let requested_capabilities = skill.requested_capabilities();
            let blocked_capabilities = blocked_capabilities_for_skill(&skill);
            let compatibility_warnings = capability_preview
                .get("compatibility_warnings")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let manifest_outcome = PathBuf::from(&skill.file_path)
                .parent()
                .and_then(|skill_dir| load_manifest_for_skill_dir(skill_dir, None).ok());
            let manifest = manifest_outcome.as_ref().map(|outcome| &outcome.manifest);
            let kind = manifest
                .map(|manifest| manifest.kind)
                .unwrap_or(SkillManifestKind::LegacyPromptOnly);
            let mcp_dependencies = manifest
                .map(|manifest| {
                    manifest
                        .mcp
                        .dependencies
                        .iter()
                        .map(|dep| dep.profile_id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let workspace_declared = effective_workspace_manifest_for_skill(&skill).is_some();
            let workspace_status = workspace_status_for_skill(vault, &skill);
            let generated_files_count = list_workspace_files(vault, &skill.name, "")
                .map(|files| files.len())
                .unwrap_or(0);
            let workspace_prepared = if workspace_declared {
                workspace_status.workspace_ready
            } else {
                true
            };
            let runtime_kind = if mcp_dependencies.is_empty() {
                "not_applicable"
            } else {
                "mcp"
            }
            .to_string();
            let runtime_ready = mcp_dependencies.is_empty();
            let missing_required_resources =
                missing_resource_paths(&skill, effective_required_resources_for_skill(&skill));
            let missing_optional_resources =
                missing_resource_paths(&skill, effective_optional_resources_for_skill(&skill));
            let missing_section_resources = missing_section_resource_paths(&skill, manifest);
            let mut unavailable_section_resources = missing_required_resources.clone();
            unavailable_section_resources.extend(missing_section_resources.iter().cloned());
            unavailable_section_resources.sort();
            unavailable_section_resources.dedup();
            let mut degraded_reasons: Vec<String> = manifest_outcome
                .as_ref()
                .map(|outcome| outcome.warnings.clone())
                .unwrap_or_default();
            if !runtime_ready {
                let message = manifest
                    .and_then(|manifest| manifest.degradation.message.clone())
                    .unwrap_or_else(|| "Required MCP profile is not enabled or healthy".into());
                degraded_reasons.push(message);
            }
            if workspace_declared && !workspace_prepared {
                degraded_reasons.push("Skill workspace is not prepared".into());
            }
            for resource in missing_required_resources
                .iter()
                .chain(missing_section_resources.iter())
            {
                degraded_reasons.push(format!("required resource `{resource}` is unavailable"));
            }
            for resource in &missing_optional_resources {
                degraded_reasons.push(format!("optional resource `{resource}` is unavailable"));
            }
            let (activated_sections, blocked_sections) = prompt_section_status_for_manifest(
                manifest,
                runtime_ready,
                workspace_prepared,
                &unavailable_section_resources,
            );
            let runtime_status = if runtime_ready {
                "ready"
            } else {
                "unavailable"
            }
            .to_string();
            let availability = if !skill.enabled {
                "disabled"
            } else if matches!(validation, SkillValidationStatus::Invalid(_)) {
                "unavailable"
            } else if !runtime_ready
                || !workspace_prepared
                || !missing_required_resources.is_empty()
                || !missing_section_resources.is_empty()
                || !unrecognized_tools.is_empty()
                || !missing_deps.is_empty()
            {
                "partial"
            } else {
                "available"
            }
            .to_string();
            let activation_ready = skill.enabled
                && !matches!(validation, SkillValidationStatus::Invalid(_))
                && (runtime_ready || !matches!(kind, SkillManifestKind::McpDependent));
            SkillListEntry {
                skill,
                validation,
                unrecognized_tools,
                missing_deps,
                task_active: None,
                task_score: None,
                confirmation_required_tools,
                content_hash,
                capability_preview,
                kind,
                activation_ready,
                runtime_kind,
                runtime_ready,
                runtime_status,
                availability,
                workspace_declared,
                workspace_prepared,
                generated_files_count,
                activated_sections,
                blocked_sections,
                degraded_reasons,
                mcp_dependencies,
                last_matched_at: None,
                last_used_at: None,
                last_activation_score: None,
                last_blocked_reason: None,
                last_resource_status: None,
                requested_capabilities,
                blocked_capabilities,
                compatibility_warnings,
                workspace_root: workspace_status.workspace_root,
                workspace_ready: workspace_status.workspace_ready,
                workspace_missing_items: workspace_status.workspace_missing_items,
            }
        })
        .collect())
}
