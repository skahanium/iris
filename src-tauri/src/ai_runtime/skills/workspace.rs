use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::storage::paths::has_reserved_path_root;

use super::path_impl::slugify;
use super::resources_impl::ALLOWED_RESOURCE_DIRS;
use super::{read_skill_resource, SkillEntry, SkillScope, SkillWorkspaceManifest};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillWorkspaceStatus {
    pub workspace_root: String,
    pub workspace_ready: bool,
    pub workspace_missing_items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillWorkspacePrepareResult {
    pub name: String,
    pub scope: String,
    pub workspace_root: String,
    pub created_folders: Vec<String>,
    pub created_documents: Vec<String>,
    pub skipped_existing: Vec<String>,
}

pub fn workspace_root_relative(name: &str) -> String {
    format!("Skills/{}", slugify(name))
}

pub fn workspace_root_path(vault: &Path, name: &str) -> PathBuf {
    vault.join(workspace_root_relative(name))
}

pub fn validate_workspace_target_path(relative: &str) -> AppResult<String> {
    validate_workspace_relative_path(relative, false)
}

pub fn validate_workspace_folder_path(relative: &str) -> AppResult<String> {
    validate_workspace_relative_path(relative, true)
}

fn validate_workspace_relative_path(relative: &str, directory: bool) -> AppResult<String> {
    let trimmed = relative.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err(AppError::msg("workspace path cannot be empty"));
    }
    if has_reserved_path_root(&trimmed) {
        return Err(AppError::msg(
            "workspace path cannot target .iris or .classified",
        ));
    }

    let mut normalized = PathBuf::new();
    for component in Path::new(&trimmed).components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            _ => return Err(AppError::msg("workspace path must stay relative")),
        }
    }

    let normalized = normalized.to_string_lossy().replace('\\', "/");
    if normalized.is_empty() {
        return Err(AppError::msg("workspace path cannot be empty"));
    }
    if directory {
        Ok(normalized.trim_end_matches('/').to_string())
    } else {
        Ok(normalized)
    }
}

pub fn validate_workspace_source_path(relative: &str) -> AppResult<String> {
    let normalized = validate_workspace_target_path(relative)?;
    let top = normalized.split('/').next().unwrap_or("");
    if !ALLOWED_RESOURCE_DIRS.contains(&top) {
        return Err(AppError::msg(
            "workspace source must come from references/, resources/, or assets/",
        ));
    }
    Ok(normalized)
}

pub fn workspace_status_for_skill(vault: &Path, skill: &SkillEntry) -> SkillWorkspaceStatus {
    let workspace_root = workspace_root_relative(&skill.name);
    let workspace_path = vault.join(&workspace_root);
    let Some(manifest) = skill.workspace_manifest() else {
        return SkillWorkspaceStatus {
            workspace_root,
            workspace_ready: true,
            workspace_missing_items: Vec::new(),
        };
    };

    let missing_folders = manifest.folders.iter().filter_map(|folder| {
        let folder = validate_workspace_folder_path(folder).ok()?;
        (!workspace_path.join(&folder).is_dir()).then(|| format!("{folder}/"))
    });
    let missing_documents = manifest.documents.iter().filter_map(|document| {
        let target = validate_workspace_target_path(&document.target).ok()?;
        (!workspace_path.join(&target).is_file()).then_some(target)
    });
    let workspace_missing_items: Vec<String> = missing_folders.chain(missing_documents).collect();

    SkillWorkspaceStatus {
        workspace_root,
        workspace_ready: workspace_missing_items.is_empty(),
        workspace_missing_items,
    }
}

pub fn preview_prepare_workspace(vault: &Path, skill: &SkillEntry) -> AppResult<serde_json::Value> {
    let status = workspace_status_for_skill(vault, skill);
    let manifest = skill.workspace_manifest().unwrap_or_default();
    Ok(serde_json::json!({
        "name": skill.name,
        "scope": match skill.scope {
            SkillScope::Global => "global",
            SkillScope::Vault => "vault",
        },
        "workspace_root": status.workspace_root,
        "workspace_ready": status.workspace_ready,
        "workspace_missing_items": status.workspace_missing_items,
        "create_folders": manifest.folders,
        "create_documents": manifest
            .documents
            .into_iter()
            .map(|document| document.target)
            .collect::<Vec<_>>(),
    }))
}

pub fn prepare_workspace_for_skill(
    vault: &Path,
    skill: &SkillEntry,
) -> AppResult<SkillWorkspacePrepareResult> {
    let workspace_root = workspace_root_relative(&skill.name);
    let workspace_root_path = vault.join(&workspace_root);
    std::fs::create_dir_all(&workspace_root_path)?;

    let manifest = skill.workspace_manifest().unwrap_or_default();
    let mut created_folders = Vec::new();
    let mut created_documents = Vec::new();
    let mut skipped_existing = Vec::new();

    for folder in manifest.folders {
        let folder = validate_workspace_folder_path(&folder)?;
        let path = workspace_root_path.join(&folder);
        if !path.exists() {
            std::fs::create_dir_all(&path)?;
            created_folders.push(folder);
        }
    }

    for document in manifest.documents {
        let source = validate_workspace_source_path(&document.source)?;
        let target = validate_workspace_target_path(&document.target)?;
        let path = workspace_root_path.join(&target);
        if path.exists() {
            skipped_existing.push(target);
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = read_skill_resource(vault, &skill.name, skill.scope, &source)?;
        std::fs::write(&path, content)?;
        created_documents.push(target);
    }

    Ok(SkillWorkspacePrepareResult {
        name: skill.name.clone(),
        scope: match skill.scope {
            SkillScope::Global => "global".into(),
            SkillScope::Vault => "vault".into(),
        },
        workspace_root,
        created_folders,
        created_documents,
        skipped_existing,
    })
}

pub fn workspace_manifest_items(manifest: &SkillWorkspaceManifest) -> bool {
    !manifest.folders.is_empty() || !manifest.documents.is_empty()
}
