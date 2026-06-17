//! Unified skill install / list / uninstall / toggle service.
//!
//! Shared by IPC commands and agent tool dispatch.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter};

use crate::ai_runtime::skill_registry::{InstallSpec, SkillInstallSource};
use crate::ai_runtime::skills::{
    blocked_capabilities_for_skill, capability_preview_for_entry, enrich_list_with_scene,
    install_from_git, install_from_local, install_from_url, prepare_workspace_for_skill,
    preview_prepare_workspace, scan_all_with_status, set_enabled, skill_content_hash_for_path,
    uninstall, validate_skill_license, SkillEntry, SkillListEntry, SkillScope,
};
use crate::ai_runtime::AiScene;
use crate::ai_types::SkillActivationPlanSummary;
use crate::embedding::engine::embed_text;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

/// Install request — IPC and tools share this shape.
#[derive(Debug, Clone)]
pub struct SkillInstallRequest {
    pub source: SkillInstallSource,
    pub path_or_url: String,
    pub scope: SkillScope,
    pub subpath: Option<String>,
    pub registry: Option<String>,
    pub expected_sha256: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillPrepareWorkspacePreview {
    pub name: String,
    pub scope: String,
    pub workspace_root: String,
    pub workspace_ready: bool,
    pub workspace_missing_items: Vec<String>,
    pub create_folders: Vec<String>,
    pub create_documents: Vec<String>,
}

fn scope_db(scope: SkillScope) -> &'static str {
    match scope {
        SkillScope::Global => "Global",
        SkillScope::Vault => "Vault",
    }
}

fn extract_keywords(entry: &SkillEntry) -> String {
    let mut parts: Vec<String> = entry
        .name
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    for word in entry.description.split_whitespace().take(12) {
        let w = word.trim_matches(|c: char| !c.is_alphanumeric());
        if w.len() >= 2 {
            parts.push(w.to_lowercase());
        }
    }
    parts.sort();
    parts.dedup();
    parts.join(" ")
}

fn record_install_source(
    db: &Database,
    name: &str,
    scope: SkillScope,
    source_type: &str,
    source_url: Option<&str>,
    content_hash: Option<&str>,
) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO skill_install_sources (skill_name, scope, source_type, source_url, installed_at, updated_at, content_hash)
             VALUES (?1, ?2, ?3, ?4, datetime('now'), datetime('now'), ?5)
             ON CONFLICT(skill_name, scope) DO UPDATE SET
               source_type = excluded.source_type,
               source_url = excluded.source_url,
               updated_at = datetime('now'),
               content_hash = excluded.content_hash",
            rusqlite::params![name, scope_db(scope), source_type, source_url, content_hash],
        )?;
        Ok(())
    })
}

fn refresh_activation_index(db: &Database, entry: &SkillEntry) -> AppResult<()> {
    let keywords = extract_keywords(entry);
    let embedding_json = if !entry.description.is_empty() {
        embed_text(&format!("{} {}", entry.name, entry.description))
            .ok()
            .and_then(|v| serde_json::to_string(&v).ok())
    } else {
        None
    };
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO skill_activation_index (skill_name, scope, description, keywords, embedding_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(skill_name, scope) DO UPDATE SET
               description = excluded.description,
               keywords = excluded.keywords,
               embedding_json = excluded.embedding_json,
               updated_at = datetime('now')",
            rusqlite::params![
                entry.name,
                scope_db(entry.scope),
                entry.description,
                keywords,
                embedding_json
            ],
        )?;
        Ok(())
    })
}

fn remove_skill_db_records(db: &Database, name: &str, scope: SkillScope) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM skill_install_sources WHERE skill_name = ?1 AND scope = ?2",
            rusqlite::params![name, scope_db(scope)],
        )?;
        conn.execute(
            "DELETE FROM skill_activation_index WHERE skill_name = ?1 AND scope = ?2",
            rusqlite::params![name, scope_db(scope)],
        )?;
        Ok(())
    })
}

fn emit_skills_changed(app_handle: Option<&AppHandle>) {
    if let Some(app) = app_handle {
        let _ = app.emit("skills:changed", ());
    }
}

fn has_blocked_critical_capability(entry: &SkillEntry) -> bool {
    blocked_capabilities_for_skill(entry)
        .iter()
        .any(|capability| {
            matches!(
                capability.capability.as_str(),
                "skill.execute_script_sandboxed"
                    | "skill.install_dependency"
                    | "skill.mcp_bridge"
                    | "execute_script_sandboxed"
                    | "install_dependency"
                    | "mcp_bridge"
                    | "bash"
                    | "shell"
                    | "computer"
                    | "computer_control"
            ) || capability.status == crate::ai_types::SkillCapabilitySupportStatus::BlockedByPolicy
        })
}

fn enrich_with_diagnostics(db: &Database, entries: &mut [SkillListEntry]) -> AppResult<()> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT skill_name, scope, last_matched_at, last_used_at,
                    last_activation_score, last_blocked_reason, last_resource_status
             FROM skill_diagnostics",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<f64>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?;
        let rows: Vec<_> = rows.collect::<Result<_, _>>()?;
        for entry in entries {
            let scope = scope_db(entry.skill.scope);
            if let Some(row) = rows
                .iter()
                .find(|row| row.0 == entry.skill.name && row.1 == scope)
            {
                entry.last_matched_at = row.2.clone();
                entry.last_used_at = row.3.clone();
                entry.last_activation_score = row.4;
                entry.last_blocked_reason = row.5.clone();
                entry.last_resource_status = row.6.clone();
            }
        }
        Ok(())
    })
}

/// Record safe per-run skill activation diagnostics.
pub fn record_skill_activation_matched(
    db: &Database,
    plan: &SkillActivationPlanSummary,
) -> AppResult<()> {
    db.with_conn(|conn| {
        for skill in &plan.activated_skills {
            let blocked_reason = skill
                .blocked_capabilities
                .first()
                .map(|blocked| blocked.fallback_guidance.clone());
            let resource_status = if skill.resources.is_empty() {
                None
            } else {
                Some(format!("{} resource(s) declared", skill.resources.len()))
            };
            conn.execute(
                "INSERT INTO skill_diagnostics
                 (skill_name, scope, last_matched_at, last_activation_score,
                  last_blocked_reason, last_resource_status, updated_at)
                 VALUES (?1, ?2, datetime('now'), ?3, ?4, ?5, datetime('now'))
                 ON CONFLICT(skill_name, scope) DO UPDATE SET
                   last_matched_at = excluded.last_matched_at,
                   last_activation_score = excluded.last_activation_score,
                   last_blocked_reason = excluded.last_blocked_reason,
                   last_resource_status = excluded.last_resource_status,
                   updated_at = datetime('now')",
                rusqlite::params![
                    skill.name,
                    skill.scope,
                    skill.score,
                    blocked_reason,
                    resource_status
                ],
            )?;
        }
        Ok(())
    })
}

/// Record that the harness consumed a skill activation plan in execution.
pub fn record_skill_activation_used(
    db: &Database,
    plan: &SkillActivationPlanSummary,
) -> AppResult<()> {
    db.with_conn(|conn| {
        for skill in &plan.activated_skills {
            conn.execute(
                "INSERT INTO skill_diagnostics
                 (skill_name, scope, last_used_at, last_activation_score, updated_at)
                 VALUES (?1, ?2, datetime('now'), ?3, datetime('now'))
                 ON CONFLICT(skill_name, scope) DO UPDATE SET
                   last_used_at = excluded.last_used_at,
                   last_activation_score = excluded.last_activation_score,
                   updated_at = datetime('now')",
                rusqlite::params![skill.name, skill.scope, skill.score],
            )?;
        }
        Ok(())
    })
}

/// Backward-compatible helper for older call sites.
pub fn record_skill_activation_diagnostics(
    db: &Database,
    plan: &SkillActivationPlanSummary,
) -> AppResult<()> {
    record_skill_activation_matched(db, plan)?;
    record_skill_activation_used(db, plan)
}

async fn install_entries(
    db: &Database,
    vault: &Path,
    app_handle: Option<&AppHandle>,
    entries: Vec<SkillEntry>,
    source_type: &str,
    source_url: Option<&str>,
) -> AppResult<Vec<SkillListEntry>> {
    let mut out = Vec::new();
    for entry in entries {
        validate_skill_license(&entry)?;
        let enabled = !has_blocked_critical_capability(&entry);
        set_enabled(&entry.name, entry.scope, vault, enabled)?;
        let content_hash =
            skill_content_hash_for_path(&std::path::PathBuf::from(&entry.file_path)).ok();
        record_install_source(
            db,
            &entry.name,
            entry.scope,
            source_type,
            source_url,
            content_hash.as_deref(),
        )?;
        refresh_activation_index(db, &entry)?;
        out.push(entry_to_list_entry(&entry, vault)?);
    }
    emit_skills_changed(app_handle);
    Ok(out)
}

fn entry_to_list_entry(entry: &SkillEntry, vault: &Path) -> AppResult<SkillListEntry> {
    let all = scan_all_with_status(vault)?;
    all.into_iter()
        .find(|e| e.skill.name == entry.name && e.skill.scope == entry.scope)
        .ok_or_else(|| AppError::msg("安装后未找到 skill"))
}

async fn install_from_spec(
    db: &Database,
    vault: &Path,
    app_handle: Option<&AppHandle>,
    spec: InstallSpec,
    scope: SkillScope,
    expected_sha256: Option<&str>,
) -> AppResult<SkillListEntry> {
    let source_url = Some(spec.path_or_url.as_str());
    match spec.source {
        SkillInstallSource::Url => {
            let entry = install_from_url(&spec.path_or_url, scope, vault, expected_sha256).await?;
            let list =
                install_entries(db, vault, app_handle, vec![entry], "url", source_url).await?;
            list.into_iter()
                .next()
                .ok_or_else(|| AppError::msg("安装失败"))
        }
        SkillInstallSource::Git => {
            let entries =
                install_from_git(&spec.path_or_url, spec.subpath.as_deref(), scope, vault).await?;
            let list = install_entries(db, vault, app_handle, entries, "git", source_url).await?;
            list.into_iter()
                .next()
                .ok_or_else(|| AppError::msg("安装失败"))
        }
        SkillInstallSource::Local => {
            let path = PathBuf::from(&spec.path_or_url);
            let entry = install_from_local(&path, scope, vault)?;
            let list =
                install_entries(db, vault, app_handle, vec![entry], "local", source_url).await?;
            list.into_iter()
                .next()
                .ok_or_else(|| AppError::msg("安装失败"))
        }
        SkillInstallSource::Registry => Err(AppError::msg("内部错误：未解析 registry")),
    }
}

/// List installed skills with validation metadata.
pub fn list_skills(
    db: &Database,
    vault: &Path,
    scene: Option<AiScene>,
) -> AppResult<Vec<SkillListEntry>> {
    let mut entries = scan_all_with_status(vault)?;
    enrich_with_diagnostics(db, &mut entries)?;
    if let Some(scene) = scene {
        enrich_list_with_scene(entries, scene, Some(db))
    } else {
        Ok(entries)
    }
}

/// Install a skill from url / git / local / registry.
pub async fn install_skill(
    db: &Database,
    vault: &Path,
    app_handle: Option<&AppHandle>,
    req: SkillInstallRequest,
) -> AppResult<SkillListEntry> {
    if req.source == SkillInstallSource::Registry {
        let registry = req.registry.as_deref().unwrap_or("skillhub");
        let spec =
            crate::ai_runtime::skill_registry::resolve_registry_named(registry, &req.path_or_url)
                .await?;
        return install_from_spec(
            db,
            vault,
            app_handle,
            spec,
            req.scope,
            req.expected_sha256.as_deref(),
        )
        .await;
    }

    let spec = InstallSpec {
        source: req.source,
        path_or_url: req.path_or_url,
        subpath: req.subpath,
        display_name: None,
    };
    install_from_spec(
        db,
        vault,
        app_handle,
        spec,
        req.scope,
        req.expected_sha256.as_deref(),
    )
    .await
}

/// Update an installed skill by reinstalling from its recorded source.
pub async fn update_skill(
    db: &Database,
    vault: &Path,
    app_handle: Option<&AppHandle>,
    name: &str,
    scope: SkillScope,
) -> AppResult<SkillListEntry> {
    let (source_type, source_url): (String, Option<String>) = db.with_conn(|conn| {
        conn.query_row(
            "SELECT source_type, source_url FROM skill_install_sources WHERE skill_name = ?1 AND scope = ?2",
            rusqlite::params![name, scope_db(scope)],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(Into::into)
    })?;
    let Some(path_or_url) = source_url else {
        return Err(AppError::msg(format!(
            "skill_update_source_missing: {name} has no recorded source"
        )));
    };
    let source = SkillInstallSource::parse(&source_type)
        .ok_or_else(|| AppError::msg(format!("skill_update_source_invalid: {source_type}")))?;
    install_skill(
        db,
        vault,
        app_handle,
        SkillInstallRequest {
            source,
            path_or_url,
            scope,
            subpath: None,
            registry: None,
            expected_sha256: None,
        },
    )
    .await
}

pub fn normalize_skill_scope_arg(scope: Option<&str>) -> SkillScope {
    parse_scope(scope.unwrap_or("vault"))
}

fn find_skill_entry(vault: &Path, name: &str, scope: SkillScope) -> AppResult<SkillEntry> {
    scan_all_with_status(vault)?
        .into_iter()
        .find(|entry| entry.skill.name == name && entry.skill.scope == scope)
        .map(|entry| entry.skill)
        .ok_or_else(|| AppError::msg(format!("skill_not_found: {name}")))
}

/// Preview install resolution (read-only, for confirm dialog).
pub async fn preview_install(
    vault: &Path,
    req: &SkillInstallRequest,
) -> AppResult<serde_json::Value> {
    if req.source == SkillInstallSource::Registry {
        let registry = req.registry.as_deref().unwrap_or("skillhub");
        let mut preview =
            crate::ai_runtime::skill_registry::preview_registry_install(registry, &req.path_or_url)
                .await?;
        preview["target_install_dir"] = serde_json::json!(match req.scope {
            SkillScope::Global => crate::ai_runtime::skills::global_skills_dir()
                .to_string_lossy()
                .into_owned(),
            SkillScope::Vault => crate::ai_runtime::skills::vault_skills_dir(vault)
                .to_string_lossy()
                .into_owned(),
        });
        return Ok(preview);
    }
    let mut preview = serde_json::json!({
        "source": match req.source {
            SkillInstallSource::Url => "url",
            SkillInstallSource::Git => "git",
            SkillInstallSource::Local => "local",
            SkillInstallSource::Registry => "registry",
        },
        "path_or_url": req.path_or_url,
        "subpath": req.subpath,
        "scope": match req.scope {
            SkillScope::Global => "global",
            SkillScope::Vault => "vault",
        },
        "target_install_dir": match req.scope {
            SkillScope::Global => crate::ai_runtime::skills::global_skills_dir()
                .to_string_lossy()
                .into_owned(),
            SkillScope::Vault => crate::ai_runtime::skills::vault_skills_dir(vault)
                .to_string_lossy()
                .into_owned(),
        },
    });
    if req.source == SkillInstallSource::Local {
        let path = std::path::PathBuf::from(&req.path_or_url);
        if path.is_file() {
            let entry = crate::ai_runtime::skills::load_skill(&path, req.scope)?;
            validate_skill_license(&entry)?;
            preview["capability_diff"] = capability_preview_for_entry(&entry, &[]);
        }
    }
    Ok(preview)
}

pub fn preview_skill_workspace(
    vault: &Path,
    name: &str,
    scope: SkillScope,
) -> AppResult<serde_json::Value> {
    let skill = find_skill_entry(vault, name, scope)?;
    preview_prepare_workspace(vault, &skill)
}

pub fn prepare_skill_workspace(
    vault: &Path,
    _db: Option<&Database>,
    app_handle: Option<&AppHandle>,
    name: &str,
    scope: SkillScope,
) -> AppResult<crate::ai_runtime::skills::SkillWorkspacePrepareResult> {
    let skill = find_skill_entry(vault, name, scope)?;
    let prepared = prepare_workspace_for_skill(vault, &skill)?;
    emit_skills_changed(app_handle);
    Ok(prepared)
}

/// Uninstall a skill by name and scope.
pub fn uninstall_skill(
    db: &Database,
    vault: &Path,
    app_handle: Option<&AppHandle>,
    name: &str,
    scope: SkillScope,
) -> AppResult<()> {
    uninstall(name, scope, vault)?;
    remove_skill_db_records(db, name, scope)?;
    emit_skills_changed(app_handle);
    Ok(())
}

/// Enable or disable a skill.
pub fn toggle_skill(
    vault: &Path,
    app_handle: Option<&AppHandle>,
    name: &str,
    scope: SkillScope,
    enabled: bool,
) -> AppResult<()> {
    set_enabled(name, scope, vault, enabled)?;
    emit_skills_changed(app_handle);
    Ok(())
}

pub fn parse_scope(scope: &str) -> SkillScope {
    if scope == "global" {
        SkillScope::Global
    } else {
        SkillScope::Vault
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;

    #[test]
    fn extract_keywords_dedupes() {
        let entry = SkillEntry {
            name: "web-scraper".into(),
            description: "Scrape web pages for research".into(),
            license: None,
            compatibility: None,
            metadata: Default::default(),
            allowed_tools: vec![],
            content: String::new(),
            scope: SkillScope::Global,
            source_url: None,
            enabled: true,
            file_path: String::new(),
            legacy_trigger: None,
        };
        let kw = extract_keywords(&entry);
        assert!(kw.contains("web-scraper"));
        assert!(kw.contains("scrape"));
    }

    #[test]
    fn normalize_skill_scope_defaults_to_vault() {
        assert_eq!(normalize_skill_scope_arg(None), SkillScope::Vault);
        assert_eq!(normalize_skill_scope_arg(Some("vault")), SkillScope::Vault);
        assert_eq!(
            normalize_skill_scope_arg(Some("global")),
            SkillScope::Global
        );
    }

    #[tokio::test]
    async fn local_install_writes_db_records() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let incoming = vault.join("incoming");
        std::fs::create_dir_all(&incoming).unwrap();
        let skill_md = incoming.join("SKILL.md");
        std::fs::write(
            &skill_md,
            "---\nname: test-skill\ndescription: For integration testing\n---\n\n# Test\n",
        )
        .unwrap();

        let state = AppState::new(dir.path().to_path_buf()).unwrap();
        state.set_vault(vault.clone()).unwrap();

        let req = SkillInstallRequest {
            source: SkillInstallSource::Local,
            path_or_url: skill_md.to_string_lossy().into_owned(),
            scope: SkillScope::Vault,
            subpath: None,
            registry: None,
            expected_sha256: None,
        };
        let entry = install_skill(&state.db, &vault, None, req)
            .await
            .expect("local install should succeed");
        assert_eq!(entry.skill.name, "test-skill");

        state.db.with_conn(|conn| {
            let source_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM skill_install_sources WHERE skill_name = 'test-skill' AND scope = 'Vault'",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(source_count, 1);
            let index_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM skill_activation_index WHERE skill_name = 'test-skill' AND scope = 'Vault'",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(index_count, 1);
            Ok(())
        }).unwrap();
    }

    #[tokio::test]
    async fn local_install_rejects_incompatible_license() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let source_dir = vault.join("incoming-bad");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("SKILL.md");
        std::fs::write(
            &source,
            r#"---
name: bad-license
description: Not compatible
license: Proprietary
---

Body
"#,
        )
        .unwrap();
        let state = AppState::new(dir.path().to_path_buf()).unwrap();
        state.set_vault(vault.clone()).unwrap();

        let err = install_skill(
            &state.db,
            &vault,
            None,
            SkillInstallRequest {
                source: SkillInstallSource::Local,
                path_or_url: source.to_string_lossy().into_owned(),
                scope: SkillScope::Vault,
                subpath: None,
                registry: None,
                expected_sha256: None,
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("license_incompatible"));
    }

    #[tokio::test]
    async fn local_install_with_blocked_critical_capability_defaults_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let source_dir = vault.join("incoming-critical");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("SKILL.md");
        std::fs::write(
            &source,
            r#"---
name: critical-skill
description: Requests unsupported execution capability
license: AGPL-3.0
requested-capabilities:
  - skill.execute_script_sandboxed
---

Body
"#,
        )
        .unwrap();
        let state = AppState::new(dir.path().to_path_buf()).unwrap();
        state.set_vault(vault.clone()).unwrap();

        let entry = install_skill(
            &state.db,
            &vault,
            None,
            SkillInstallRequest {
                source: SkillInstallSource::Local,
                path_or_url: source.to_string_lossy().into_owned(),
                scope: SkillScope::Vault,
                subpath: None,
                registry: None,
                expected_sha256: None,
            },
        )
        .await
        .expect("critical skill should install for inspection");

        assert_eq!(entry.skill.name, "critical-skill");
        assert!(!entry.skill.enabled);
        assert!(entry
            .blocked_capabilities
            .iter()
            .any(|capability| capability.capability == "skill.execute_script_sandboxed"));
        assert_eq!(entry.availability, "disabled");
    }

    #[tokio::test]
    async fn prepare_workspace_creates_declared_items_without_overwriting_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let source_dir = vault.join("incoming-workspace");
        std::fs::create_dir_all(source_dir.join("resources")).unwrap();
        let source = source_dir.join("SKILL.md");
        std::fs::write(
            source_dir.join("resources/default-note.md"),
            "# Default note\n",
        )
        .unwrap();
        std::fs::write(
            &source,
            r#"---
name: workspace-skill
description: Declares a workspace
license: AGPL-3.0
iris-workspace:
  folders:
    - inputs
    - outputs
  documents:
    - source: resources/default-note.md
      target: README.md
---

Body
"#,
        )
        .unwrap();
        let state = AppState::new(dir.path().to_path_buf()).unwrap();
        state.set_vault(vault.clone()).unwrap();

        install_skill(
            &state.db,
            &vault,
            None,
            SkillInstallRequest {
                source: SkillInstallSource::Local,
                path_or_url: source.to_string_lossy().into_owned(),
                scope: SkillScope::Vault,
                subpath: None,
                registry: None,
                expected_sha256: None,
            },
        )
        .await
        .unwrap();

        let legacy_workspace_root = vault.join("Skills/workspace-skill");
        std::fs::create_dir_all(&legacy_workspace_root).unwrap();
        std::fs::write(legacy_workspace_root.join("README.md"), "# Existing\n").unwrap();
        std::fs::write(legacy_workspace_root.join("legacy.md"), "# Legacy\n").unwrap();

        let prepared = prepare_skill_workspace(
            &vault,
            Some(&state.db),
            None,
            "workspace-skill",
            SkillScope::Vault,
        )
        .unwrap();

        let workspace_root = vault.join(".iris/skills-workspaces/workspace-skill");
        assert_eq!(
            prepared.workspace_root,
            ".iris/skills-workspaces/workspace-skill"
        );
        assert!(workspace_root.join("inputs").is_dir());
        assert!(workspace_root.join("outputs").is_dir());
        assert_eq!(
            std::fs::read_to_string(workspace_root.join("README.md")).unwrap(),
            "# Existing\n"
        );
        assert_eq!(
            std::fs::read_to_string(workspace_root.join("legacy.md")).unwrap(),
            "# Legacy\n"
        );
        assert!(!legacy_workspace_root.exists());
        assert_eq!(prepared.created_folders, vec!["inputs", "outputs"]);
        assert_eq!(prepared.skipped_existing, vec!["README.md"]);
        assert!(prepared.migrated_legacy_items.contains(&"legacy.md".into()));
    }

    #[test]
    fn workspace_root_is_internal_reserved_path() {
        assert_eq!(
            crate::ai_runtime::skills::workspace_root_relative("Workspace Skill"),
            ".iris/skills-workspaces/workspace-skill"
        );
    }

    #[tokio::test]
    async fn update_reinstalls_from_recorded_local_source_and_refreshes_hash() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let source_dir = vault.join("incoming-update");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join("SKILL.md");
        std::fs::write(
            &source,
            r#"---
name: updatable
description: First version
license: AGPL-3.0
---

First
"#,
        )
        .unwrap();
        let state = AppState::new(dir.path().to_path_buf()).unwrap();
        state.set_vault(vault.clone()).unwrap();

        let first = install_skill(
            &state.db,
            &vault,
            None,
            SkillInstallRequest {
                source: SkillInstallSource::Local,
                path_or_url: source.to_string_lossy().into_owned(),
                scope: SkillScope::Vault,
                subpath: None,
                registry: None,
                expected_sha256: None,
            },
        )
        .await
        .unwrap();
        let first_hash = first.content_hash.clone().expect("content hash");

        std::fs::write(
            &source,
            r#"---
name: updatable
description: Second version
license: AGPL-3.0
allowed-tools:
  - memory_read
---

Second
"#,
        )
        .unwrap();

        let updated = update_skill(&state.db, &vault, None, "updatable", SkillScope::Vault)
            .await
            .unwrap();

        assert_eq!(updated.skill.description, "Second version");
        assert_ne!(updated.content_hash.as_deref(), Some(first_hash.as_str()));
        assert!(updated.capability_preview["requested_tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "memory_read"));
    }
}
