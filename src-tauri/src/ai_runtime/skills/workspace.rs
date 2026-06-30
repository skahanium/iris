use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::Utc;
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::storage::paths::has_reserved_path_root;

use super::manifest_impl::load_manifest_for_skill_dir;
use super::path_impl::slugify;
use super::resources_impl::{ALLOWED_RESOURCE_DIRS, MAX_SKILL_RESOURCE_CHARS};
use super::{
    read_skill_resource, SkillEntry, SkillScope, SkillWorkspaceDocument, SkillWorkspaceManifest,
};

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
    pub migrated_legacy_items: Vec<String>,
    pub legacy_conflicts: Vec<String>,
}

pub fn workspace_root_relative(name: &str) -> String {
    format!(".iris/skills-workspaces/{}", slugify(name))
}

pub fn workspace_root_path(vault: &Path, name: &str) -> PathBuf {
    vault.join(workspace_root_relative(name))
}

fn legacy_workspace_root_path(vault: &Path, name: &str) -> PathBuf {
    vault.join("Skills").join(slugify(name))
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

fn typed_workspace_manifest_for_skill(skill: &SkillEntry) -> Option<SkillWorkspaceManifest> {
    let skill_root = Path::new(&skill.file_path).parent()?;
    let outcome = load_manifest_for_skill_dir(skill_root, None).ok()?;
    let workspace = outcome.manifest.workspace;
    if !workspace.declared && workspace.folders.is_empty() && workspace.documents.is_empty() {
        return None;
    }
    Some(SkillWorkspaceManifest {
        folders: workspace.folders,
        documents: workspace
            .documents
            .into_iter()
            .map(|document| SkillWorkspaceDocument {
                source: document.source,
                target: document.target,
            })
            .collect(),
    })
}

pub(crate) fn effective_workspace_manifest_for_skill(
    skill: &SkillEntry,
) -> Option<SkillWorkspaceManifest> {
    typed_workspace_manifest_for_skill(skill).or_else(|| skill.workspace_manifest())
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
    let Some(manifest) = effective_workspace_manifest_for_skill(skill) else {
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
    let manifest = effective_workspace_manifest_for_skill(skill).unwrap_or_default();
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

fn relative_to_workspace(root: &Path, path: &Path) -> AppResult<String> {
    Ok(path
        .strip_prefix(root)
        .map_err(|_| AppError::msg("workspace migration path escaped root"))?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn collect_workspace_files(root: &Path) -> AppResult<Vec<String>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> AppResult<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out)?;
            } else if path.is_file() {
                out.push(relative_to_workspace(root, &path)?);
            }
        }
        Ok(())
    }
    walk(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn move_legacy_workspace_item(
    legacy_root: &Path,
    new_root: &Path,
    source: &Path,
    conflict_root: &Path,
    migrated: &mut Vec<String>,
    conflicts: &mut Vec<String>,
) -> AppResult<()> {
    if source.is_dir() {
        for entry in fs::read_dir(source)? {
            move_legacy_workspace_item(
                legacy_root,
                new_root,
                &entry?.path(),
                conflict_root,
                migrated,
                conflicts,
            )?;
        }
        if fs::read_dir(source)?.next().is_none() {
            fs::remove_dir(source)?;
        }
        return Ok(());
    }
    if !source.is_file() {
        return Ok(());
    }

    let rel = relative_to_workspace(legacy_root, source)?;
    let target = new_root.join(&rel);
    let final_target = if target.exists() {
        let conflict = conflict_root.join(&rel);
        conflicts.push(rel.clone());
        conflict
    } else {
        target
    };
    if let Some(parent) = final_target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(source, &final_target)?;
    migrated.push(rel);
    Ok(())
}

fn migrate_legacy_workspace(
    vault: &Path,
    name: &str,
    new_root: &Path,
) -> AppResult<(Vec<String>, Vec<String>)> {
    let legacy_root = legacy_workspace_root_path(vault, name);
    if !legacy_root.exists() {
        return Ok((Vec::new(), Vec::new()));
    }
    if !legacy_root.is_dir() {
        return Ok((Vec::new(), Vec::new()));
    }

    if !new_root.exists() {
        if let Some(parent) = new_root.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&legacy_root, new_root)?;
        return Ok((collect_workspace_files(new_root)?, Vec::new()));
    }

    let mut migrated = Vec::new();
    let mut conflicts = Vec::new();
    let conflict_root = new_root
        .join("_legacy-conflicts")
        .join(Utc::now().format("%Y%m%d%H%M%S").to_string());
    for entry in fs::read_dir(&legacy_root)? {
        move_legacy_workspace_item(
            &legacy_root,
            new_root,
            &entry?.path(),
            &conflict_root,
            &mut migrated,
            &mut conflicts,
        )?;
    }
    migrated.sort();
    conflicts.sort();
    if legacy_root.exists() && fs::read_dir(&legacy_root)?.next().is_none() {
        fs::remove_dir(&legacy_root)?;
    }
    Ok((migrated, conflicts))
}

pub fn prepare_workspace_for_skill(
    vault: &Path,
    skill: &SkillEntry,
) -> AppResult<SkillWorkspacePrepareResult> {
    let workspace_root = workspace_root_relative(&skill.name);
    let workspace_root_path = vault.join(&workspace_root);
    let (migrated_legacy_items, legacy_conflicts) =
        migrate_legacy_workspace(vault, &skill.name, &workspace_root_path)?;
    std::fs::create_dir_all(&workspace_root_path)?;

    let manifest = effective_workspace_manifest_for_skill(skill).unwrap_or_default();
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
        migrated_legacy_items,
        legacy_conflicts,
    })
}

pub fn workspace_manifest_items(manifest: &SkillWorkspaceManifest) -> bool {
    !manifest.folders.is_empty() || !manifest.documents.is_empty()
}

fn workspace_file_path(vault: &Path, name: &str, relative_path: &str) -> AppResult<PathBuf> {
    let rel = validate_workspace_target_path(relative_path)?;
    Ok(workspace_root_path(vault, name).join(rel))
}

fn ensure_inside_workspace(root: &Path, target: &Path) -> AppResult<()> {
    let root = root
        .canonicalize()
        .map_err(|_| AppError::msg("skill workspace does not exist"))?;
    let target = target
        .canonicalize()
        .map_err(|_| AppError::msg("skill workspace file does not exist"))?;
    if !target.starts_with(&root) {
        return Err(AppError::msg("skill workspace path escaped root"));
    }
    Ok(())
}

fn ensure_write_target_inside_workspace(root: &Path, target: &Path) -> AppResult<()> {
    let root = root
        .canonicalize()
        .map_err(|_| AppError::msg("skill workspace does not exist"))?;
    let parent = target
        .parent()
        .ok_or_else(|| AppError::msg("skill workspace file path has no parent"))?
        .canonicalize()
        .map_err(|_| AppError::msg("skill workspace file parent does not exist"))?;
    if !parent.starts_with(&root) {
        return Err(AppError::msg("skill workspace path escaped root"));
    }
    if let Ok(metadata) = fs::symlink_metadata(target) {
        if metadata.file_type().is_symlink() {
            return Err(AppError::msg(
                "skill workspace symlink writes are not allowed",
            ));
        }
        let target = target
            .canonicalize()
            .map_err(|_| AppError::msg("skill workspace file does not exist"))?;
        if !target.starts_with(&root) {
            return Err(AppError::msg("skill workspace path escaped root"));
        }
    }
    Ok(())
}

pub fn read_workspace_file(vault: &Path, name: &str, relative_path: &str) -> AppResult<String> {
    let root = workspace_root_path(vault, name);
    let target = workspace_file_path(vault, name, relative_path)?;
    ensure_inside_workspace(&root, &target)?;
    let content = fs::read_to_string(target)?;
    Ok(content.chars().take(MAX_SKILL_RESOURCE_CHARS).collect())
}

pub fn write_workspace_file(
    vault: &Path,
    name: &str,
    relative_path: &str,
    content: &str,
) -> AppResult<String> {
    let target = workspace_file_path(vault, name, relative_path)?;
    let root = workspace_root_path(vault, name);
    fs::create_dir_all(&root)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    ensure_write_target_inside_workspace(&root, &target)?;
    fs::write(&target, content)?;
    relative_to_workspace(&root, &target)
}

pub fn list_workspace_files(vault: &Path, name: &str, path: &str) -> AppResult<Vec<String>> {
    let root = workspace_root_path(vault, name);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let base = if path.trim().is_empty() {
        root.clone()
    } else {
        root.join(validate_workspace_folder_path(path)?)
    };
    ensure_inside_workspace(&root, &base)?;
    collect_workspace_files(&base).map(|items| {
        items
            .into_iter()
            .map(|item| {
                if path.trim().is_empty() {
                    item
                } else {
                    let prefix = validate_workspace_folder_path(path).unwrap_or_default();
                    format!("{}/{}", prefix.trim_end_matches('/'), item)
                }
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_file_ops_stay_inside_skill_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();

        write_workspace_file(&vault, "demo-skill", "notes/state.md", "# State\n").unwrap();
        assert_eq!(
            read_workspace_file(&vault, "demo-skill", "notes/state.md").unwrap(),
            "# State\n"
        );
        let listed = list_workspace_files(&vault, "demo-skill", "").unwrap();
        assert_eq!(listed, vec!["notes/state.md"]);

        assert!(write_workspace_file(&vault, "demo-skill", "../escape.md", "x").is_err());
        assert!(read_workspace_file(&vault, "demo-skill", "../escape.md").is_err());
    }
}
