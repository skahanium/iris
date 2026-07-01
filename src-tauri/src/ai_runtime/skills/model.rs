use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::manifest_impl::SkillManifestKind;
pub(super) const VALIDATION_MISSING_FRONTMATTER: &str = "_iris_missing_frontmatter";
pub(super) const VALIDATION_MANIFEST_ERROR: &str = "_iris_manifest_error";
pub(super) const VALIDATION_NAME_MISMATCH: &str = "_iris_name_mismatch";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    Global,
    Vault,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillScopeRule {
    pub kind: String,
    pub pattern: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillConfirmationStatus {
    Confirmed,
    #[default]
    NeedsConfirmation,
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
    pub content: String,
    pub scope: SkillScope,
    pub enabled: bool,
    pub file_path: String,
    #[serde(default)]
    pub scope_rules: Vec<SkillScopeRule>,
    #[serde(default)]
    pub content_hash: String,
    #[serde(default)]
    pub confirmed_hash: Option<String>,
    #[serde(default)]
    pub confirmation_status: SkillConfirmationStatus,
    /// Preserved from old-format `trigger` field for backward compatibility.
    pub legacy_trigger: Option<String>,
}

impl Default for SkillEntry {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            license: None,
            compatibility: None,
            metadata: SkillMetadata::new(),
            content: String::new(),
            scope: SkillScope::Vault,
            enabled: false,
            file_path: String::new(),
            scope_rules: Vec::new(),
            content_hash: String::new(),
            confirmed_hash: None,
            confirmation_status: SkillConfirmationStatus::NeedsConfirmation,
            legacy_trigger: None,
        }
    }
}

impl SkillEntry {
    fn metadata_string_list(&self, keys: &[&str]) -> Vec<String> {
        for key in keys {
            if let Some(value) = self.metadata.get(*key) {
                match value {
                    serde_json::Value::String(raw) => {
                        return raw
                            .split_whitespace()
                            .filter(|s| !s.trim().is_empty())
                            .map(String::from)
                            .collect();
                    }
                    serde_json::Value::Array(values) => {
                        return values
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                    }
                    _ => {}
                }
            }
        }
        Vec::new()
    }

    /// Trigger hints declared by modern skill manifests.
    pub fn trigger_hints(&self) -> Vec<String> {
        self.metadata_string_list(&["trigger-hints", "trigger_hints", "triggers"])
    }

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
            .get(VALIDATION_MANIFEST_ERROR)
            .and_then(|v| v.as_str())
            .is_some()
        {
            return SkillValidationStatus::Invalid("manifest is not prompt_only".into());
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
        if self.legacy_trigger.is_some() {
            return SkillValidationStatus::Legacy;
        }
        SkillValidationStatus::Valid
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
    #[serde(default)]
    pub confirmed_hashes: HashMap<String, String>,
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
    /// Dependencies that are not installed.
    pub missing_deps: Vec<String>,
    /// Whether this skill would be injected for the requested task (`None` if not scored).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_active: Option<bool>,
    /// Task affinity score (`None` if not scored or skill inactive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_score: Option<f64>,
    /// Manifest/runtime kind for this skill.
    pub kind: SkillManifestKind,
    /// Whether the skill can be considered during prompt injection.
    pub activation_ready: bool,
    /// Last match timestamp from diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_matched_at: Option<String>,
    /// Last use timestamp from diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    /// Last activation score from diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activation_score: Option<f64>,
    /// Last blocked reason from diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_blocked_reason: Option<String>,
    /// Last resource status from diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_resource_status: Option<String>,
}
