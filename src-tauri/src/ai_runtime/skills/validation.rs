use crate::ai_runtime::tool_catalog::TOOL_CATALOG;
use crate::error::{AppError, AppResult};

use super::SkillEntry;

/// Return true when a declared skill license is acceptable for Iris.
pub fn license_is_agpl_compatible(license: Option<&str>) -> bool {
    let Some(raw) = license.map(str::trim).filter(|s| !s.is_empty()) else {
        return true;
    };
    let normalized = raw.to_lowercase();
    let rejected = [
        "proprietary",
        "commercial",
        "all rights reserved",
        "gpl-2.0-only",
        "lgpl-2.1-only",
    ];
    if rejected.iter().any(|needle| normalized.contains(needle)) {
        return false;
    }
    let accepted = [
        "agpl-3.0",
        "gpl-3.0",
        "lgpl-3.0",
        "apache-2.0",
        "mit",
        "bsd-2-clause",
        "bsd-3-clause",
        "mpl-2.0",
        "cc0",
        "unlicense",
    ];
    accepted.iter().any(|needle| normalized.contains(needle))
}

/// Validate a skill license and return a stable error code for UI recovery.
pub fn validate_skill_license(entry: &SkillEntry) -> AppResult<()> {
    if license_is_agpl_compatible(entry.license.as_deref()) {
        Ok(())
    } else {
        Err(AppError::msg(format!(
            "license_incompatible: skill '{}' declares incompatible license '{}'",
            entry.name,
            entry.license.as_deref().unwrap_or("unknown")
        )))
    }
}

/// Tools declared by a skill that require user confirmation in the harness.
pub fn confirmation_required_tools(tools: &[String]) -> Vec<String> {
    tools
        .iter()
        .filter(|t| {
            TOOL_CATALOG
                .iter()
                .any(|e| e.name == t.as_str() && e.requires_confirmation)
        })
        .cloned()
        .collect()
}

/// Build a read-only capability preview for install confirmation and Skills UI.
pub fn capability_preview_for_entry(
    entry: &SkillEntry,
    installed_names: &[String],
) -> serde_json::Value {
    serde_json::json!({
        "name": entry.name,
        "license": entry.license,
        "requested_tools": entry.allowed_tools,
        "confirmation_required_tools": confirmation_required_tools(&entry.allowed_tools),
        "unrecognized_tools": entry.unrecognized_tools(),
        "missing_deps": entry.missing_dependencies(installed_names),
        "allows_script_execution": false,
        "script_policy": "scripts/resources may be read as text only; Iris does not execute skill scripts",
    })
}
