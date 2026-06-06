//! Unified skill install / list / uninstall / toggle service.
//!
//! Shared by IPC commands and agent tool dispatch.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter};

use crate::ai_runtime::skill_registry::{InstallSpec, SkillInstallSource};
use crate::ai_runtime::skills::{
    install_from_git, install_from_local, install_from_url, scan_all_with_status, set_enabled,
    uninstall, SkillEntry, SkillListEntry, SkillScope,
};
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
) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO skill_install_sources (skill_name, scope, source_type, source_url, installed_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(skill_name, scope) DO UPDATE SET
               source_type = excluded.source_type,
               source_url = excluded.source_url,
               installed_at = datetime('now')",
            rusqlite::params![name, scope_db(scope), source_type, source_url],
        )?;
        Ok(())
    })
}

fn refresh_activation_index(db: &Database, entry: &SkillEntry) -> AppResult<()> {
    let keywords = extract_keywords(entry);
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO skill_activation_index (skill_name, scope, description, keywords, updated_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(skill_name, scope) DO UPDATE SET
               description = excluded.description,
               keywords = excluded.keywords,
               updated_at = datetime('now')",
            rusqlite::params![
                entry.name,
                scope_db(entry.scope),
                entry.description,
                keywords
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
        set_enabled(&entry.name, entry.scope, vault, true)?;
        record_install_source(db, &entry.name, entry.scope, source_type, source_url)?;
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
) -> AppResult<SkillListEntry> {
    let source_url = Some(spec.path_or_url.as_str());
    match spec.source {
        SkillInstallSource::Url => {
            let entry = install_from_url(&spec.path_or_url, scope, vault).await?;
            let list =
                install_entries(db, vault, app_handle, vec![entry], "url", source_url).await?;
            list.into_iter()
                .next()
                .ok_or_else(|| AppError::msg("安装失败"))
        }
        SkillInstallSource::Git => {
            let entries = install_from_git(
                &spec.path_or_url,
                spec.subpath.as_deref(),
                scope,
                vault,
            )
            .await?;
            let list =
                install_entries(db, vault, app_handle, entries, "git", source_url).await?;
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
pub fn list_skills(vault: &Path) -> AppResult<Vec<SkillListEntry>> {
    scan_all_with_status(vault)
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
        return install_from_spec(db, vault, app_handle, spec, req.scope).await;
    }

    let spec = InstallSpec {
        source: req.source,
        path_or_url: req.path_or_url,
        subpath: req.subpath,
        display_name: None,
    };
    install_from_spec(db, vault, app_handle, spec, req.scope).await
}

/// Preview install resolution (read-only, for confirm dialog).
pub async fn preview_install(req: &SkillInstallRequest) -> AppResult<serde_json::Value> {
    if req.source == SkillInstallSource::Registry {
        let registry = req.registry.as_deref().unwrap_or("skillhub");
        return crate::ai_runtime::skill_registry::preview_registry_install(
            registry,
            &req.path_or_url,
        )
        .await;
    }
    Ok(serde_json::json!({
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
    }))
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
    if scope == "vault" {
        SkillScope::Vault
    } else {
        SkillScope::Global
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
}
