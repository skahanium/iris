//! Unified skill install / list / uninstall / toggle service.
//!
//! Shared by IPC commands and agent tool dispatch.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter};

use crate::ai_runtime::agent_task_policy::intent_from_legacy_scene;
use crate::ai_runtime::capability_resolver::{
    resolve_required_capability, CapabilityBlockReason, CapabilityResolutionError,
};
use crate::ai_runtime::mcp_runtime_registry::{
    clear_skill_runtime_requirement, resolve_skill_runtime, upsert_skill_runtime_requirement,
    SkillRuntimeRequirementInput,
};
use crate::ai_runtime::skill_registry::{InstallSpec, SkillInstallSource};
use crate::ai_runtime::skill_trust_policy::{
    build_skill_trust_profile, persist_skill_trust_profile, SkillSourceKind,
};
use crate::ai_runtime::skills::{
    blocked_capabilities_for_skill, capability_preview_for_entry, enrich_list_with_task,
    install_from_git, install_from_local, install_from_url, load_manifest_for_skill_dir,
    prepare_workspace_for_skill, preview_prepare_workspace, scan_all_with_status, set_enabled,
    skill_content_hash_for_path, uninstall, validate_skill_license, SkillEntry, SkillListEntry,
    SkillScope, SkillValidationStatus,
};
use crate::ai_runtime::AiScene;
use crate::ai_types::{
    BlockedCapabilitySummary, SkillActivationPlanSummary, SkillCapabilitySupportStatus,
};
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

#[derive(Debug, Clone, Copy)]
struct InstallTrustContext<'a> {
    install_source_type: &'a str,
    trust_source_type: SkillSourceKind,
    source_url: Option<&'a str>,
    expected_sha256: Option<&'a str>,
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

fn manifest_for_entry(
    entry: &SkillListEntry,
) -> Option<crate::ai_runtime::skills::IrisSkillManifest> {
    PathBuf::from(&entry.skill.file_path)
        .parent()
        .and_then(|skill_dir| load_manifest_for_skill_dir(skill_dir, None).ok())
        .map(|outcome| outcome.manifest)
}

fn manifest_required_capabilities(entry: &SkillListEntry) -> Vec<String> {
    let Some(manifest) = manifest_for_entry(entry) else {
        return Vec::new();
    };
    let mut capabilities = manifest.capabilities.requires;
    capabilities.extend(
        manifest
            .prompt
            .sections
            .iter()
            .flat_map(|section| section.requires_capabilities.iter().cloned()),
    );
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

fn missing_capability_blocks_sections(entry: &mut SkillListEntry, missing_capabilities: &[String]) {
    let Some(manifest) = manifest_for_entry(entry) else {
        return;
    };
    if manifest.prompt.sections.is_empty() {
        if !missing_capabilities.is_empty()
            && !entry
                .blocked_sections
                .iter()
                .any(|section| section == "capabilities")
        {
            entry.blocked_sections.push("capabilities".into());
        }
        return;
    }
    for section in &manifest.prompt.sections {
        if section
            .requires_capabilities
            .iter()
            .any(|capability| missing_capabilities.contains(capability))
        {
            entry.activated_sections.retain(|id| id != &section.id);
            if !entry.blocked_sections.contains(&section.id) {
                entry.blocked_sections.push(section.id.clone());
            }
        }
    }
}

fn capability_status(reason: CapabilityBlockReason) -> SkillCapabilitySupportStatus {
    match reason {
        CapabilityBlockReason::UnsupportedCapability => {
            SkillCapabilitySupportStatus::UnsupportedByProductScope
        }
        CapabilityBlockReason::PolicyBlocked => SkillCapabilitySupportStatus::BlockedByPolicy,
        _ => SkillCapabilitySupportStatus::MissingUserGrant,
    }
}

fn capability_block_summary(
    skill_name: &str,
    error: &CapabilityResolutionError,
) -> BlockedCapabilitySummary {
    let status = capability_status(error.reason);
    BlockedCapabilitySummary {
        skill_name: skill_name.to_string(),
        capability: error.capability.clone(),
        status,
        risk_level: "high".into(),
        permission: None,
        fallback_guidance: format!("{}: {}", error.reason_code(), error.message),
    }
}

fn enrich_with_capability_resolver(db: &Database, entries: &mut [SkillListEntry]) {
    for entry in entries {
        let required_capabilities = manifest_required_capabilities(entry);
        if required_capabilities.is_empty() {
            continue;
        }
        let mut missing_capabilities = Vec::new();
        for capability in required_capabilities {
            if let Err(error) = resolve_required_capability(db, &capability) {
                missing_capabilities.push(error.capability.clone());
                let summary = capability_block_summary(&entry.skill.name, &error);
                if !entry
                    .blocked_capabilities
                    .iter()
                    .any(|blocked| blocked.capability == summary.capability)
                {
                    entry.blocked_capabilities.push(summary);
                }
                let reason = format!(
                    "required capability `{}` is unavailable: {}",
                    error.capability,
                    error.reason_code()
                );
                if !entry.degraded_reasons.contains(&reason) {
                    entry.degraded_reasons.push(reason);
                }
            }
        }
        if missing_capabilities.is_empty() {
            continue;
        }
        missing_capability_blocks_sections(entry, &missing_capabilities);
        entry.activation_ready = false;
        if entry.skill.enabled && !matches!(entry.validation, SkillValidationStatus::Invalid(_)) {
            entry.availability = "partial".to_string();
        }
    }
}
fn sync_runtime_requirements(db: &Database, entries: &[SkillListEntry]) -> AppResult<()> {
    for entry in entries {
        if entry.mcp_dependencies.is_empty() {
            clear_skill_runtime_requirement(db, &entry.skill.name, entry.skill.scope)?;
            continue;
        }

        let required_profiles_json =
            serde_json::to_string(&entry.mcp_dependencies).unwrap_or_else(|_| "[]".to_string());
        let manifest_capabilities = manifest_required_capabilities(entry);
        let required_capabilities_json = if manifest_capabilities.is_empty() {
            serde_json::to_string(&entry.requested_capabilities)
                .unwrap_or_else(|_| "[]".to_string())
        } else {
            serde_json::to_string(&manifest_capabilities).unwrap_or_else(|_| "[]".to_string())
        };
        let workspace_contract_json = serde_json::json!({
            "declared": entry.workspace_declared,
            "prepared": entry.workspace_prepared,
            "root": entry.workspace_root,
            "missing_items": entry.workspace_missing_items,
        })
        .to_string();
        let degradation_policy_json = serde_json::json!({
            "reasons": entry.degraded_reasons,
        })
        .to_string();
        let kind = serde_json::to_value(entry.kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "legacy_prompt_only".to_string());

        upsert_skill_runtime_requirement(
            db,
            &SkillRuntimeRequirementInput {
                skill_name: entry.skill.name.clone(),
                scope: entry.skill.scope,
                manifest_hash: entry.content_hash.clone(),
                kind,
                runtime_kind: entry.runtime_kind.clone(),
                required_profiles_json,
                required_capabilities_json,
                workspace_contract_json,
                degradation_policy_json,
            },
        )?;
    }
    Ok(())
}

fn skill_resource_file_exists(skill_root: &Path, relative_path: &str) -> bool {
    if relative_path.trim().is_empty() || relative_path.contains("..") {
        return false;
    }
    let rel = Path::new(relative_path.trim_start_matches('/'));
    if rel.is_absolute() {
        return false;
    }
    let Some(top) = rel.components().next().and_then(|c| c.as_os_str().to_str()) else {
        return false;
    };
    if !matches!(top, "references" | "resources" | "assets") {
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

fn refresh_prompt_sections_from_current_status(entry: &mut SkillListEntry) {
    let Some(manifest) = manifest_for_entry(entry) else {
        return;
    };
    if manifest.prompt.sections.is_empty() {
        return;
    }
    let Some(skill_root) = PathBuf::from(&entry.skill.file_path)
        .parent()
        .map(Path::to_path_buf)
    else {
        return;
    };
    let selected = if manifest.prompt.default_sections.is_empty() {
        manifest
            .prompt
            .sections
            .iter()
            .map(|section| section.id.clone())
            .collect::<Vec<_>>()
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
        let runtime_ok = !section.requires_runtime || entry.runtime_ready;
        let workspace_ok = !section.requires_workspace || entry.workspace_prepared;
        let resources_ok = section
            .requires_resources
            .iter()
            .all(|resource| skill_resource_file_exists(&skill_root, resource));
        if runtime_ok && workspace_ok && resources_ok {
            activated.push(section.id.clone());
        } else {
            blocked.push(section.id.clone());
        }
    }
    entry.activated_sections = activated;
    entry.blocked_sections = blocked;
}
fn enrich_with_runtime_registry(db: &Database, entries: &mut [SkillListEntry]) -> AppResult<()> {
    sync_runtime_requirements(db, entries)?;
    for entry in entries {
        let readiness = resolve_skill_runtime(db, &entry.skill.name, entry.skill.scope)?;
        if readiness.required_profiles.is_empty() && entry.mcp_dependencies.is_empty() {
            continue;
        }

        entry.runtime_kind = readiness.runtime_kind;
        entry.runtime_ready = readiness.ready;
        entry.runtime_status = readiness.status.as_str().to_string();
        if !readiness.required_profiles.is_empty() {
            entry.mcp_dependencies = readiness.required_profiles;
        }
        for reason in readiness.degraded_reasons {
            if !entry.degraded_reasons.contains(&reason) {
                entry.degraded_reasons.push(reason);
            }
        }

        if entry.runtime_ready {
            refresh_prompt_sections_from_current_status(entry);
            let otherwise_available = entry.skill.enabled
                && !matches!(entry.validation, SkillValidationStatus::Invalid(_))
                && entry.workspace_prepared
                && entry.unrecognized_tools.is_empty()
                && entry.missing_deps.is_empty();
            if otherwise_available {
                entry.availability = "available".to_string();
                entry.activation_ready = true;
            }
        } else {
            entry.activation_ready = false;
            if entry.skill.enabled && !matches!(entry.validation, SkillValidationStatus::Invalid(_))
            {
                entry.availability = "partial".to_string();
            }
        }
    }
    Ok(())
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
    trust_ctx: InstallTrustContext<'_>,
) -> AppResult<Vec<SkillListEntry>> {
    let mut out = Vec::new();
    for entry in entries {
        validate_skill_license(&entry)?;
        let content_hash =
            skill_content_hash_for_path(&std::path::PathBuf::from(&entry.file_path)).ok();
        let trust_profile = build_skill_trust_profile(
            &entry,
            trust_ctx.trust_source_type,
            trust_ctx.source_url,
            content_hash.as_deref(),
            trust_ctx.expected_sha256,
        );
        let enabled = !has_blocked_critical_capability(&entry) && !trust_profile.high_risk;
        set_enabled(&entry.name, entry.scope, vault, enabled)?;
        record_install_source(
            db,
            &entry.name,
            entry.scope,
            trust_ctx.install_source_type,
            trust_ctx.source_url,
            content_hash.as_deref(),
        )?;
        persist_skill_trust_profile(db, &trust_profile)?;
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
    trust_source_type: SkillSourceKind,
    expected_sha256: Option<&str>,
) -> AppResult<SkillListEntry> {
    let source_url = Some(spec.path_or_url.as_str());
    match spec.source {
        SkillInstallSource::Url => {
            let entry = install_from_url(&spec.path_or_url, scope, vault, expected_sha256).await?;
            let list = install_entries(
                db,
                vault,
                app_handle,
                vec![entry],
                InstallTrustContext {
                    install_source_type: "url",
                    trust_source_type,
                    source_url,
                    expected_sha256,
                },
            )
            .await?;
            list.into_iter()
                .next()
                .ok_or_else(|| AppError::msg("安装失败"))
        }
        SkillInstallSource::Git => {
            let entries =
                install_from_git(&spec.path_or_url, spec.subpath.as_deref(), scope, vault).await?;
            let list = install_entries(
                db,
                vault,
                app_handle,
                entries,
                InstallTrustContext {
                    install_source_type: "git",
                    trust_source_type,
                    source_url,
                    expected_sha256,
                },
            )
            .await?;
            list.into_iter()
                .next()
                .ok_or_else(|| AppError::msg("安装失败"))
        }
        SkillInstallSource::Local => {
            let path = PathBuf::from(&spec.path_or_url);
            let entry = install_from_local(&path, scope, vault)?;
            let list = install_entries(
                db,
                vault,
                app_handle,
                vec![entry],
                InstallTrustContext {
                    install_source_type: "local",
                    trust_source_type,
                    source_url,
                    expected_sha256,
                },
            )
            .await?;
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
    enrich_with_runtime_registry(db, &mut entries)?;
    enrich_with_capability_resolver(db, &mut entries);
    if let Some(scene) = scene {
        enrich_list_with_task(
            entries,
            intent_from_legacy_scene(scene),
            scene.profile(),
            &[],
            Some(db),
        )
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
            SkillSourceKind::Registry,
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
        match req.source {
            SkillInstallSource::Git => SkillSourceKind::Git,
            SkillInstallSource::Local => SkillSourceKind::Local,
            SkillInstallSource::Registry => SkillSourceKind::Registry,
            SkillInstallSource::Url => SkillSourceKind::Url,
        },
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
        preview["trust_profile_preview"] =
            trust_preview_for_request(req, SkillSourceKind::Registry);
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
            let profile = build_skill_trust_profile(
                &entry,
                SkillSourceKind::Local,
                Some(&req.path_or_url),
                None,
                req.expected_sha256.as_deref(),
            );
            preview["trust_profile_preview"] = serde_json::to_value(profile).unwrap_or_default();
        }
    } else {
        preview["trust_profile_preview"] = trust_preview_for_request(
            req,
            match req.source {
                SkillInstallSource::Git => SkillSourceKind::Git,
                SkillInstallSource::Registry => SkillSourceKind::Registry,
                SkillInstallSource::Url => SkillSourceKind::Url,
                SkillInstallSource::Local => SkillSourceKind::Local,
            },
        );
    }
    Ok(preview)
}

fn trust_preview_for_request(
    req: &SkillInstallRequest,
    source_type: SkillSourceKind,
) -> serde_json::Value {
    let sha256_locked = req.expected_sha256.is_some();
    let warnings = if !sha256_locked && source_type != SkillSourceKind::Local {
        vec!["skill source is not locked with expected_sha256"]
    } else {
        Vec::new()
    };
    serde_json::json!({
        "source_type": source_type.as_str(),
        "source_url": req.path_or_url,
        "sha256_locked": sha256_locked,
        "warnings": warnings,
    })
}

pub fn preview_update(
    db: &Database,
    name: &str,
    scope: SkillScope,
) -> AppResult<serde_json::Value> {
    let (source_type, source_url, content_hash): (String, Option<String>, Option<String>) =
        db.with_conn(|conn| {
            conn.query_row(
                "SELECT source_type, source_url, content_hash FROM skill_install_sources WHERE skill_name = ?1 AND scope = ?2",
                rusqlite::params![name, scope_db(scope)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(Into::into)
        })?;
    Ok(serde_json::json!({
        "name": name,
        "scope": scope_db(scope),
        "source_type": source_type,
        "source_url": source_url,
        "previous_content_hash": content_hash,
        "trust_profile_preview": {
            "sha256_locked": false,
            "warnings": ["skill update reuses the recorded source without a locked expected_sha256"],
        },
    }))
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

    #[tokio::test]
    async fn prepare_workspace_uses_typed_manifest_contract() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let source_dir = vault.join("incoming-typed-workspace");
        std::fs::create_dir_all(source_dir.join("resources")).unwrap();
        std::fs::write(source_dir.join("resources/seed.md"), "# Seed\n").unwrap();
        std::fs::write(
            source_dir.join("SKILL.md"),
            r#"---
name: typed-workspace-skill
description: Declares workspace in iris.skill.toml
license: AGPL-3.0
iris_manifest: iris.skill.toml
---

Body
"#,
        )
        .unwrap();
        std::fs::write(
            source_dir.join("iris.skill.toml"),
            r#"schema_version = "1"
name = "typed-workspace-skill"
kind = "workspace"
license = "AGPL-3.0"

[workspace]
declared = true
folders = ["inputs", "outputs"]

[[workspace.documents]]
source = "resources/seed.md"
target = "README.md"
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
                path_or_url: source_dir.join("SKILL.md").to_string_lossy().into_owned(),
                scope: SkillScope::Vault,
                subpath: None,
                registry: None,
                expected_sha256: None,
            },
        )
        .await
        .unwrap();

        let prepared = prepare_skill_workspace(
            &vault,
            Some(&state.db),
            None,
            "typed-workspace-skill",
            SkillScope::Vault,
        )
        .unwrap();
        let workspace_root = vault.join(".iris/skills-workspaces/typed-workspace-skill");

        assert_eq!(prepared.created_folders, vec!["inputs", "outputs"]);
        assert_eq!(prepared.created_documents, vec!["README.md"]);
        assert!(workspace_root.join("inputs").is_dir());
        assert!(workspace_root.join("outputs").is_dir());
        assert_eq!(
            std::fs::read_to_string(workspace_root.join("README.md")).unwrap(),
            "# Seed\n"
        );
    }

    #[test]
    fn list_skills_blocks_section_when_required_capability_has_no_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let skill_dir = vault.join(".iris/skills/capability-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: capability-skill
description: Needs web search capability
license: AGPL-3.0
iris_manifest: iris.skill.toml
---

Use search when available.
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("iris.skill.toml"),
            r#"schema_version = "1"
name = "capability-skill"
kind = "mcp_dependent"
license = "AGPL-3.0"

[prompt]
default_sections = ["search"]

[[prompt.sections]]
id = "search"
source = "SKILL.md"
requires_runtime = true
requires_capabilities = ["web.search"]

[capabilities]
requires = ["web.search"]

[[mcp.dependencies]]
profile_id = "search-profile"
required_capabilities = ["web.search"]
required = true
"#,
        )
        .unwrap();
        let db = Database::open_in_memory().unwrap();
        crate::ai_runtime::mcp_runtime_registry::upsert_server_catalog(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::McpServerCatalogInput {
                id: "search-server".into(),
                display_name: "Search Server".into(),
                transport: "stdio".into(),
                command: Some("search-mcp".into()),
                args_json: "[]".into(),
                url: None,
                env_schema_json: "{}".into(),
                capability_tags_json: "[\"web.search\"]".into(),
                source: "test".into(),
            },
        )
        .unwrap();
        crate::ai_runtime::mcp_runtime_registry::upsert_runtime_profile(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::McpRuntimeProfileInput {
                id: "search-profile".into(),
                server_id: "search-server".into(),
                vault_scope_hash: None,
                display_name: "Search profile".into(),
                enabled: true,
                transport_config_json: "{}".into(),
                env_bindings_json: "{}".into(),
                status: crate::ai_runtime::mcp_runtime_registry::McpRuntimeStatus::Ready,
                last_error: None,
            },
        )
        .unwrap();

        let entries = list_skills(&db, &vault, None).unwrap();
        let entry = entries
            .iter()
            .find(|entry| entry.skill.name == "capability-skill")
            .unwrap();

        assert!(
            entry.runtime_ready,
            "profile readiness is separate from capability mapping"
        );
        assert_eq!(entry.availability, "partial");
        assert!(entry.activated_sections.is_empty());
        assert_eq!(entry.blocked_sections, vec!["search".to_string()]);
        assert!(entry
            .blocked_capabilities
            .iter()
            .any(|blocked| blocked.capability == "web.search"));
        assert!(entry
            .degraded_reasons
            .iter()
            .any(|reason| reason.contains("web.search")));
    }

    #[test]
    fn list_skills_allows_section_when_required_capability_has_approved_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let skill_dir = vault.join(".iris/skills/capability-ready-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: capability-ready-skill
description: Uses mapped web search capability
license: AGPL-3.0
iris_manifest: iris.skill.toml
---

Use search when available.
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("iris.skill.toml"),
            r#"schema_version = "1"
name = "capability-ready-skill"
kind = "mcp_dependent"
license = "AGPL-3.0"

[prompt]
default_sections = ["search"]

[[prompt.sections]]
id = "search"
source = "SKILL.md"
requires_runtime = true
requires_capabilities = ["web.search"]

[capabilities]
requires = ["web.search"]

[[mcp.dependencies]]
profile_id = "search-profile"
required_capabilities = ["web.search"]
required = true
"#,
        )
        .unwrap();
        let db = Database::open_in_memory().unwrap();
        crate::ai_runtime::mcp_runtime_registry::upsert_server_catalog(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::McpServerCatalogInput {
                id: "search-server".into(),
                display_name: "Search Server".into(),
                transport: "stdio".into(),
                command: Some("search-mcp".into()),
                args_json: "[]".into(),
                url: None,
                env_schema_json: "{}".into(),
                capability_tags_json: "[\"web.search\"]".into(),
                source: "test".into(),
            },
        )
        .unwrap();
        crate::ai_runtime::mcp_runtime_registry::upsert_runtime_profile(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::McpRuntimeProfileInput {
                id: "search-profile".into(),
                server_id: "search-server".into(),
                vault_scope_hash: None,
                display_name: "Search profile".into(),
                enabled: true,
                transport_config_json: "{}".into(),
                env_bindings_json: "{}".into(),
                status: crate::ai_runtime::mcp_runtime_registry::McpRuntimeStatus::Ready,
                last_error: None,
            },
        )
        .unwrap();
        crate::ai_runtime::mcp_runtime_registry::record_tool_inventory(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::McpToolInventoryInput {
                profile_id: "search-profile".into(),
                tool_name: "search".into(),
                schema_hash: "sha256:test".into(),
                capability_mapping_json: "{\"capability\":\"web.search\"}".into(),
                description: Some("Search".into()),
            },
        )
        .unwrap();

        let entries = list_skills(&db, &vault, None).unwrap();
        let entry = entries
            .iter()
            .find(|entry| entry.skill.name == "capability-ready-skill")
            .unwrap();

        assert!(entry.runtime_ready);
        assert_eq!(entry.availability, "available");
        assert_eq!(entry.activated_sections, vec!["search".to_string()]);
        assert!(entry.blocked_sections.is_empty());
        assert!(entry.blocked_capabilities.is_empty());
    }
    #[test]
    fn list_skills_resolves_mcp_runtime_profile_readiness() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let skill_dir = vault.join(".iris/skills/anysearch");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: anysearch
description: Search through MCP
license: AGPL-3.0
---

Use AnySearch.
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("iris.skill.toml"),
            r#"schema_version = "1"
name = "anysearch"
version = "0.1.0"
kind = "mcp_dependent"
license = "AGPL-3.0"

[prompt]
sections = [{ id = "main", source = "SKILL.md" }]

[[mcp.dependencies]]
profile_id = "anysearch-default"
server_id = "anysearch"
required = true
"#,
        )
        .unwrap();
        let db = Database::open_in_memory().unwrap();

        let first = list_skills(&db, &vault, None).unwrap();
        let first = first
            .iter()
            .find(|entry| entry.skill.name == "anysearch")
            .unwrap();
        assert!(!first.runtime_ready);
        assert_eq!(first.runtime_status, "unavailable");
        assert_eq!(first.availability, "partial");

        crate::ai_runtime::mcp_runtime_registry::upsert_server_catalog(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::McpServerCatalogInput {
                id: "anysearch".into(),
                display_name: "AnySearch".into(),
                transport: "stdio".into(),
                command: Some("anysearch-mcp".into()),
                args_json: "[]".into(),
                url: None,
                env_schema_json: "{}".into(),
                capability_tags_json: "[\"web.search\"]".into(),
                source: "test".into(),
            },
        )
        .unwrap();
        crate::ai_runtime::mcp_runtime_registry::upsert_runtime_profile(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::McpRuntimeProfileInput {
                id: "anysearch-default".into(),
                server_id: "anysearch".into(),
                vault_scope_hash: None,
                display_name: "AnySearch default".into(),
                enabled: true,
                transport_config_json: "{}".into(),
                env_bindings_json: "{}".into(),
                status: crate::ai_runtime::mcp_runtime_registry::McpRuntimeStatus::Ready,
                last_error: None,
            },
        )
        .unwrap();

        let second = list_skills(&db, &vault, None).unwrap();
        let second = second
            .iter()
            .find(|entry| entry.skill.name == "anysearch")
            .unwrap();
        assert!(second.runtime_ready);
        assert_eq!(second.runtime_status, "ready");
        assert!(second.activation_ready);
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
