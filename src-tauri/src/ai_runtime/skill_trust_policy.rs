use serde::{Deserialize, Serialize};

use crate::ai_runtime::skills::{SkillEntry, SkillScope};
use crate::ai_runtime::tool_catalog::catalog_find;
use crate::ai_runtime::ToolAccessLevel;
use crate::ai_types::SkillRuntimeCapability;
use crate::error::AppResult;
use crate::storage::db::Database;

/// Source kind used for skill trust profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillSourceKind {
    Registry,
    Git,
    Url,
    Local,
}

impl SkillSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Registry => "registry",
            Self::Git => "git",
            Self::Url => "url",
            Self::Local => "local",
        }
    }
}

/// Coarse risk band for a skill trust profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillTrustRiskLevel {
    Low,
    Medium,
    High,
}

impl SkillTrustRiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Persisted skill trust profile generated at install/update time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillTrustProfile {
    pub skill_name: String,
    pub scope: SkillScope,
    pub source_type: SkillSourceKind,
    pub source_url: Option<String>,
    pub integrity_hash: Option<String>,
    pub declared_capabilities: Vec<String>,
    pub requested_tools: Vec<String>,
    pub risk_level: SkillTrustRiskLevel,
    pub high_risk: bool,
    pub allowed_tools_narrowing_only: bool,
    pub sha256_locked: bool,
    pub warnings: Vec<String>,
}

/// Execution decision derived from a trust profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillTrustDecision {
    pub auto_activate: bool,
    pub requires_confirmation: bool,
    pub warnings: Vec<String>,
}

pub fn build_skill_trust_profile(
    entry: &SkillEntry,
    source_type: SkillSourceKind,
    source_url: Option<&str>,
    content_hash: Option<&str>,
    expected_sha256: Option<&str>,
) -> SkillTrustProfile {
    let declared_capabilities = entry
        .requested_capabilities()
        .into_iter()
        .map(SkillRuntimeCapability::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let requested_tools = entry.allowed_tools.clone();
    let allowed_tools_narrowing_only = requested_tools
        .iter()
        .all(|tool_name| catalog_find(tool_name).is_some());
    let has_write_or_manage_tool = requested_tools.iter().any(|tool_name| {
        catalog_find(tool_name).is_some_and(|entry| {
            matches!(
                entry.access_level,
                ToolAccessLevel::WriteCache
                    | ToolAccessLevel::WriteMarkdown
                    | ToolAccessLevel::WriteSettings
                    | ToolAccessLevel::ManageSkills
            ) || entry.requires_confirmation
        })
    });
    let has_high_risk_capability = entry.requested_capabilities().iter().any(|capability| {
        matches!(
            capability,
            SkillRuntimeCapability::ExecuteScriptSandboxed
                | SkillRuntimeCapability::InstallDependency
                | SkillRuntimeCapability::McpBridge
        )
    });
    let sha256_locked = expected_sha256.is_some();
    let mut warnings = Vec::new();
    if !allowed_tools_narrowing_only {
        warnings.push("allowed-tools contains tools outside Iris ToolCatalog".to_string());
    }
    if !sha256_locked && !matches!(source_type, SkillSourceKind::Local) {
        warnings.push("skill source is not locked with expected_sha256".to_string());
    }

    let risk_level = if has_high_risk_capability || !allowed_tools_narrowing_only {
        SkillTrustRiskLevel::High
    } else if has_write_or_manage_tool
        || declared_capabilities
            .iter()
            .any(|capability| capability == "skill.write_storage")
    {
        SkillTrustRiskLevel::Medium
    } else {
        SkillTrustRiskLevel::Low
    };
    let high_risk = risk_level == SkillTrustRiskLevel::High;

    SkillTrustProfile {
        skill_name: entry.name.clone(),
        scope: entry.scope,
        source_type,
        source_url: source_url
            .map(str::to_string)
            .or_else(|| entry.source_url.clone()),
        integrity_hash: content_hash.map(str::to_string),
        declared_capabilities,
        requested_tools,
        risk_level,
        high_risk,
        allowed_tools_narrowing_only,
        sha256_locked,
        warnings,
    }
}

pub fn evaluate_skill_trust(profile: &SkillTrustProfile) -> SkillTrustDecision {
    let requires_confirmation = profile.high_risk || !profile.warnings.is_empty();
    SkillTrustDecision {
        auto_activate: !profile.high_risk,
        requires_confirmation,
        warnings: profile.warnings.clone(),
    }
}

pub fn persist_skill_trust_profile(db: &Database, profile: &SkillTrustProfile) -> AppResult<()> {
    let declared_capabilities_json = serde_json::to_string(&profile.declared_capabilities)?;
    let requested_tools_json = serde_json::to_string(&profile.requested_tools)?;
    let warnings_json = serde_json::to_string(&profile.warnings)?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO skill_trust_profiles
             (skill_name, scope, source_type, source_url, integrity_hash,
              declared_capabilities_json, requested_tools_json, risk_level,
              high_risk, sha256_locked, allowed_tools_narrowing_only, warnings_json,
              created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                     datetime('now'), datetime('now'))
             ON CONFLICT(skill_name, scope, source_type) DO UPDATE SET
               source_url = excluded.source_url,
               integrity_hash = excluded.integrity_hash,
               declared_capabilities_json = excluded.declared_capabilities_json,
               requested_tools_json = excluded.requested_tools_json,
               risk_level = excluded.risk_level,
               high_risk = excluded.high_risk,
               sha256_locked = excluded.sha256_locked,
               allowed_tools_narrowing_only = excluded.allowed_tools_narrowing_only,
               warnings_json = excluded.warnings_json,
               updated_at = datetime('now')",
            rusqlite::params![
                profile.skill_name,
                scope_db(profile.scope),
                profile.source_type.as_str(),
                profile.source_url.as_deref(),
                profile.integrity_hash.as_deref(),
                declared_capabilities_json,
                requested_tools_json,
                profile.risk_level.as_str(),
                profile.high_risk,
                profile.sha256_locked,
                profile.allowed_tools_narrowing_only,
                warnings_json,
            ],
        )?;
        Ok(())
    })
}

fn scope_db(scope: SkillScope) -> &'static str {
    match scope {
        SkillScope::Global => "Global",
        SkillScope::Vault => "Vault",
    }
}
