use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::ai_runtime::tool_catalog::TOOL_CATALOG;

pub(super) const VALIDATION_MISSING_FRONTMATTER: &str = "_iris_missing_frontmatter";
pub(super) const VALIDATION_NAME_MISMATCH: &str = "_iris_name_mismatch";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    Global,
    Vault,
}

/// Metadata bag - arbitrary key-value pairs from frontmatter.
pub type SkillMetadata = HashMap<String, serde_json::Value>;

/// Validation status of a skill file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillValidationStatus {
    /// New-format skill with valid frontmatter.
    Valid,
    /// Old-format skill with `trigger` field (readable, prompts migration).
    Legacy,
    /// Missing required fields or invalid content.
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub license: Option<String>,
    pub compatibility: Option<String>,
    pub metadata: SkillMetadata,
    pub allowed_tools: Vec<String>,
    pub content: String,
    pub scope: SkillScope,
    pub source_url: Option<String>,
    pub enabled: bool,
    pub file_path: String,
    /// Preserved from old-format `trigger` field for backward compatibility.
    pub legacy_trigger: Option<String>,
}

impl SkillEntry {
    /// Validation status: Valid / Legacy / Invalid.
    pub fn validation_status(&self) -> SkillValidationStatus {
        if self.name.is_empty() {
            return SkillValidationStatus::Invalid("name is empty".into());
        }
        if self.description.is_empty() {
            return SkillValidationStatus::Invalid("description is empty".into());
        }
        if self.description.len() > 1024 {
            return SkillValidationStatus::Invalid("description exceeds 1024 chars".into());
        }
        if let Some(ref compat) = self.compatibility {
            if compat.len() > 500 {
                return SkillValidationStatus::Invalid("compatibility exceeds 500 chars".into());
            }
        }
        if self
            .metadata
            .get(VALIDATION_MISSING_FRONTMATTER)
            .and_then(|v| v.as_bool())
            == Some(true)
            && self.legacy_trigger.is_none()
        {
            return SkillValidationStatus::Invalid("missing YAML frontmatter".into());
        }
        if self
            .metadata
            .get(VALIDATION_NAME_MISMATCH)
            .and_then(|v| v.as_bool())
            == Some(true)
            && self.legacy_trigger.is_none()
        {
            return SkillValidationStatus::Invalid("name must match parent directory".into());
        }
        for tool in &self.allowed_tools {
            if TOOL_CATALOG.iter().all(|e| e.name != tool.as_str()) {
                return SkillValidationStatus::Invalid(format!(
                    "allowed-tool '{tool}' not found in ToolCatalog"
                ));
            }
        }
        if self.legacy_trigger.is_some() {
            return SkillValidationStatus::Legacy;
        }
        SkillValidationStatus::Valid
    }

    /// Whether all allowed_tools are recognized by the ToolCatalog.
    pub fn all_allowed_tools_recognized(&self) -> bool {
        self.allowed_tools.is_empty()
            || self
                .allowed_tools
                .iter()
                .all(|t| TOOL_CATALOG.iter().any(|e| e.name == t.as_str()))
    }

    /// Tools requested but NOT in the catalog (for UI display).
    pub fn unrecognized_tools(&self) -> Vec<String> {
        self.allowed_tools
            .iter()
            .filter(|t| TOOL_CATALOG.iter().all(|e| e.name != t.as_str()))
            .cloned()
            .collect()
    }

    /// Dependencies declared in `metadata.depends` (Iris extension field).
    /// Returns skill names this skill depends on.
    pub fn depends(&self) -> Vec<String> {
        match self.metadata.get("depends") {
            Some(serde_json::Value::String(s)) => s.split_whitespace().map(String::from).collect(),
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            _ => vec![],
        }
    }

    /// Check which dependencies are missing from the installed skill list.
    pub fn missing_dependencies(&self, installed_names: &[String]) -> Vec<String> {
        self.depends()
            .into_iter()
            .filter(|dep| !installed_names.contains(dep))
            .collect()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct SkillsConfig {
    pub disabled: Vec<String>,
}

/// Scored skill match result.
#[derive(Debug, Clone)]
pub struct ScoredSkill<'a> {
    pub skill: &'a SkillEntry,
    pub score: f64,
}

/// Cached activation metadata from `skill_activation_index`.
#[derive(Debug, Clone)]
pub struct SkillActivationIndexRow {
    pub skill_name: String,
    pub scope: SkillScope,
    pub description: Option<String>,
    pub keywords: Option<String>,
    pub embedding_json: Option<String>,
}

pub type ActivationIndexMap = HashMap<(String, SkillScope), SkillActivationIndexRow>;

/// DTO for `skills_list` IPC response - includes computed fields.
#[derive(Debug, Clone, Serialize)]
pub struct SkillListEntry {
    #[serde(flatten)]
    pub skill: SkillEntry,
    /// Validation status (valid / legacy / invalid).
    pub validation: SkillValidationStatus,
    /// Tools not found in the ToolCatalog.
    pub unrecognized_tools: Vec<String>,
    /// Dependencies that are not installed.
    pub missing_deps: Vec<String>,
    /// Whether this skill would be injected for the requested scene (`None` if no scene).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_active: Option<bool>,
    /// Scene affinity score (`None` if no scene requested or skill inactive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_score: Option<f64>,
    /// Subset of `allowed_tools` that require harness confirmation.
    pub confirmation_required_tools: Vec<String>,
    /// SHA-256 of the installed SKILL.md file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Capability summary shown before/after install.
    pub capability_preview: serde_json::Value,
    /// Human-readable capability status: available / partial / unavailable.
    pub availability: String,
}
