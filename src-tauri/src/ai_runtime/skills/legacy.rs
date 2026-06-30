use std::fs;
use std::path::Path;

use crate::error::{AppError, AppResult};

use super::frontmatter_impl::parse_frontmatter;
use super::{load_skill, SkillEntry, SkillScope};

/// Migrate a legacy `trigger`-based skill to the new Agent Skills format.
///
/// - Reads the existing SKILL.md
/// - Converts `trigger` into new format fields (removes trigger, keeps description)
/// - Creates a backup at `SKILL.md.bak` before overwriting
/// - Returns the migrated SkillEntry
///
/// Does NOT auto-migrate - caller must obtain user confirmation first.
pub fn migrate_legacy_skill(path: &Path, scope: SkillScope) -> AppResult<SkillEntry> {
    let raw = fs::read_to_string(path)?;
    let (meta, body) = parse_frontmatter(&raw);

    if !meta.contains_key("trigger") {
        return Err(AppError::msg("skill is already in the new format"));
    }

    let backup_path = path.with_extension("md.bak");
    fs::copy(path, &backup_path)?;

    let mut new_front = String::from("---\n");
    for (k, v) in &meta {
        if k == "trigger" {
            continue;
        }
        if v.contains(':') || v.contains('#') || v.contains('"') {
            new_front.push_str(&format!("{}: \"{}\"\n", k, v.replace('"', "\\\"")));
        } else {
            new_front.push_str(&format!("{k}: {v}\n"));
        }
    }
    new_front.push_str("---\n\n");

    let new_content = format!("{new_front}{}", body.trim_start());
    fs::write(path, &new_content)?;

    load_skill(path, scope)
}

/// Check if a skill file is in legacy format (has `trigger` field).
pub fn is_legacy_format(path: &Path) -> bool {
    if let Ok(raw) = fs::read_to_string(path) {
        let (meta, _) = parse_frontmatter(&raw);
        meta.contains_key("trigger")
    } else {
        false
    }
}
