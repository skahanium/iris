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
