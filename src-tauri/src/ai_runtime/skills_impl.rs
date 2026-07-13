//! Agent Skills runtime - SKILL.md registry, validation, matching, prompt injection.
//!
//! Compatible with Agent Skills specification while preserving Iris local-first
//! security model. Old `trigger`-based skills continue to work via `legacy_trigger`.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

#[path = "skills/activation.rs"]
mod activation_impl;
#[path = "skills/compatibility.rs"]
mod compatibility_impl;
#[path = "skills/frontmatter.rs"]
mod frontmatter_impl;
#[path = "skills/legacy.rs"]
mod legacy_impl;
#[path = "skills/manifest.rs"]
mod manifest_impl;
#[path = "skills/model.rs"]
mod model_impl;
#[path = "skills/path.rs"]
mod path_impl;
#[path = "skills/prompt.rs"]
mod prompt_impl;
#[path = "skills/scan.rs"]
mod scan_impl;
#[path = "skills/validation.rs"]
mod validation_impl;

pub use activation_impl::{
    active_skills_for_task_prompt, build_skill_activation_plan_for_task,
    build_skill_activation_plan_for_task_with_runtime, enrich_list_with_task,
    filter_skill_content_to_injected_sections, load_activation_index, rank_skills_for_task,
    rerank_skills_with_vectors, skills_for_task,
};
pub use compatibility_impl::{
    blocked_capabilities_for_skill, fallback_guidance, normalize_external_capability,
    support_status_for_capability,
};
#[cfg(test)]
use frontmatter_impl::parse_frontmatter;
pub use legacy_impl::{is_legacy_format, migrate_legacy_skill};
pub use manifest_impl::{
    load_manifest_for_skill_dir, IrisSkillManifest, ManifestLoadOutcome, SkillManifestKind,
};
pub use model_impl::{
    ActivationIndexMap, ScoredSkill, SkillActivationIndexRow, SkillConfirmationStatus, SkillEntry,
    SkillListEntry, SkillMetadata, SkillScope, SkillScopeRule, SkillValidationStatus,
};
pub use path_impl::validate_skill_path;
#[cfg(test)]
use path_impl::{atomic_copy_dir, slugify, validate_subpath};
pub(crate) use path_impl::{global_skills_dir, vault_skills_dir};
use path_impl::{load_config, save_config, skill_key};
pub use prompt_impl::inject_into_prompt;
pub use scan_impl::{
    load_skill, scan_all, scan_all_metadata, scan_all_with_status, skill_content_hash_for_path,
};
pub use validation_impl::{license_is_agpl_compatible, validate_skill_license};

#[cfg(test)]
fn uninstall(name: &str, scope: SkillScope, vault: &Path) -> AppResult<()> {
    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    if base.is_dir() {
        for entry in fs::read_dir(&base)? {
            let entry = entry?;
            let path = entry.path();
            let skill_file = path.join("SKILL.md");
            if skill_file.is_file() {
                if let Ok(skill) = load_skill(&skill_file, scope) {
                    if skill.name == name {
                        fs::remove_dir_all(path)?;
                        return Ok(());
                    }
                }
            }
        }
    }
    let slug = slugify(name);
    let target = base.join(slug);
    if target.is_dir() {
        fs::remove_dir_all(target)?;
    }
    Ok(())
}

pub fn parse_scope(scope: &str) -> SkillScope {
    if scope == "global" {
        SkillScope::Global
    } else {
        SkillScope::Vault
    }
}

pub fn normalize_skill_scope_arg(scope: Option<&str>) -> SkillScope {
    parse_scope(scope.unwrap_or("vault"))
}

pub fn list_skills(_db: &Database, vault: &Path) -> AppResult<Vec<SkillListEntry>> {
    scan_all_with_status(vault)
}

fn record_confirmed_skill_hash(
    name: &str,
    scope: SkillScope,
    vault: &Path,
    content_hash: &str,
) -> AppResult<()> {
    let mut config = load_config(scope, vault);
    let key = skill_key(scope, name);
    config.disabled.retain(|disabled| disabled != &key);
    config
        .confirmed_hashes
        .insert(key, content_hash.trim().to_string());
    save_config(scope, vault, &config)
}

/// Write updated skill content (must be `SKILL.md`).
fn write_skill_content(path: &Path, scope: SkillScope, content: &str) -> AppResult<SkillEntry> {
    if path.file_name().and_then(|n| n.to_str()) != Some("SKILL.md") {
        return Err(AppError::msg("only SKILL.md can be written"));
    }
    fs::write(path, content)?;
    load_skill(path, scope)
}

pub fn write_confirmed_skill_content(
    vault: &Path,
    path: &Path,
    scope: SkillScope,
    content: &str,
) -> AppResult<SkillEntry> {
    if path.file_name().and_then(|name| name.to_str()) != Some("SKILL.md") {
        return Err(AppError::msg("only SKILL.md can be confirmed"));
    }
    let base = match scope {
        SkillScope::Global => global_skills_dir(),
        SkillScope::Vault => vault_skills_dir(vault),
    };
    fs::create_dir_all(&base)?;
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(AppError::msg(
            "Skill target path must stay inside the skills directory",
        ));
    }
    let target_path: PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    if !target_path.starts_with(&base) {
        return Err(AppError::msg(
            "Skill target path must stay inside the skills directory",
        ));
    }
    let parent = target_path
        .parent()
        .ok_or_else(|| AppError::msg("invalid Skill target path"))?;
    fs::create_dir_all(parent)?;
    let base_canonical = base.canonicalize()?;
    let parent_canonical = parent.canonicalize()?;
    if !parent_canonical.starts_with(base_canonical) {
        return Err(AppError::msg(
            "Skill target path must stay inside the skills directory",
        ));
    }
    let entry = write_skill_content(&target_path, scope, content)?;
    record_confirmed_skill_hash(&entry.name, scope, vault, &entry.content_hash)?;
    let mut confirmed = entry;
    confirmed.confirmed_hash = Some(confirmed.content_hash.clone());
    confirmed.confirmation_status = SkillConfirmationStatus::Confirmed;
    confirmed.enabled = true;
    Ok(confirmed)
}

#[cfg(test)]
#[path = "skills/status_tests.rs"]
mod status_tests;

#[cfg(test)]
#[path = "skills/validation_tests.rs"]
mod validation_tests;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::ai_types::AgentIntent;

    use super::*;
    // validate_subpath

    #[test]
    fn subpath_rejects_dotdot() {
        let err = validate_subpath("../x").unwrap_err();
        assert!(err.to_string().contains("invalid subpath"));
    }

    #[test]
    fn subpath_rejects_dotdot_in_middle() {
        let err = validate_subpath("a/../../b").unwrap_err();
        assert!(err.to_string().contains("invalid subpath"));
    }

    #[test]
    fn subpath_rejects_absolute_path() {
        let err = validate_subpath("/etc/passwd").unwrap_err();
        assert!(err.to_string().contains("invalid subpath"));
    }

    #[test]
    fn subpath_rejects_root() {
        let err = validate_subpath("/").unwrap_err();
        assert!(err.to_string().contains("invalid subpath"));
    }

    #[test]
    fn subpath_accepts_simple_relative() {
        assert!(validate_subpath("skills/my-skill").is_ok());
    }

    #[test]
    fn subpath_accepts_single_component() {
        assert!(validate_subpath("my-skill").is_ok());
    }

    #[test]
    fn subpath_accepts_dot_slash() {
        assert!(validate_subpath("./skills").is_ok());
    }
    // atomic_copy_dir

    #[test]
    fn atomic_copy_copies_contents() {
        let src = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("skill");
        fs::write(src.path().join("SKILL.md"), "# Test Skill").unwrap();
        fs::write(src.path().join("data.txt"), "data").unwrap();
        atomic_copy_dir(src.path(), &dest).unwrap();
        assert_eq!(
            fs::read_to_string(dest.join("SKILL.md")).unwrap(),
            "# Test Skill"
        );
        assert_eq!(fs::read_to_string(dest.join("data.txt")).unwrap(), "data");
    }

    #[test]
    fn atomic_copy_overwrites_existing() {
        let src = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("skill");
        fs::write(src.path().join("SKILL.md"), "new").unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("SKILL.md"), "old").unwrap();
        atomic_copy_dir(src.path(), &dest).unwrap();
        assert_eq!(fs::read_to_string(dest.join("SKILL.md")).unwrap(), "new");
    }
    // slugify

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("My Skill"), "my-skill");
        assert_eq!(slugify("hello_world"), "hello_world");
        assert_eq!(slugify("a/b\\c"), "a-b-c");
    }

    #[test]
    fn yaml_frontmatter_supports_arrays_and_objects() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("yaml-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let path = skill_dir.join("SKILL.md");
        fs::write(
            &path,
            r#"---
name: yaml-skill
description: Parses modern Agent Skills frontmatter
metadata:
  depends:
    - helper-skill
  keywords:
    - research
    - memory
license: AGPL-3.0
---

# Body
"#,
        )
        .unwrap();

        let skill = load_skill(&path, SkillScope::Global).unwrap();
        assert_eq!(skill.depends(), vec!["helper-skill".to_string()]);
        assert_eq!(skill.license.as_deref(), Some("AGPL-3.0"));
    }

    #[test]
    fn load_skill_keeps_scope_rules_as_prompt_only_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("scoped-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_path = skill_dir.join("SKILL.md");
        fs::write(
            &skill_path,
            r#"---
name: scoped-skill
description: Reads scope rules without runtime setup
scope:
  - kind: glob
    pattern: notes/**
---

# Body
"#,
        )
        .unwrap();

        let skill = load_skill(&skill_path, SkillScope::Vault).unwrap();
        assert_eq!(skill.scope_rules.len(), 1);
        assert_eq!(skill.scope_rules[0].kind, "glob");
        assert_eq!(skill.scope_rules[0].pattern, "notes/**");
    }

    #[test]
    fn uninstall_removes_actual_skill_dir_when_name_mismatches_dir() {
        let vault_dir = tempfile::tempdir().unwrap();
        let vault = vault_dir.path();
        let skill_root = vault.join(".iris").join("skills").join("custom-dir");
        fs::create_dir_all(&skill_root).unwrap();
        fs::write(
            skill_root.join("SKILL.md"),
            r#"---
name: displayed-name
description: Directory and name intentionally differ
---

Body
"#,
        )
        .unwrap();

        uninstall("displayed-name", SkillScope::Vault, vault).unwrap();
        assert!(
            !skill_root.exists(),
            "uninstall should remove the directory containing the matching SKILL.md"
        );
    }

    #[allow(unused_variables)]
    #[test]
    fn subpath_symlink_escape_rejected_by_canonicalize() {
        let tmp = tempfile::tempdir().unwrap();
        let tmp_canonical = tmp.path().canonicalize().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("SKILL.md"), "# Escape").unwrap();
        #[cfg(unix)]
        {
            let link_path = tmp.path().join("escape-link");
            std::os::unix::fs::symlink(outside.path(), &link_path).unwrap();
            let canon = link_path.canonicalize().unwrap();
            assert!(!canon.starts_with(&tmp_canonical));
        }
    }

    #[test]
    fn subpath_stays_inside_clone() {
        let tmp = tempfile::tempdir().unwrap();
        let tmp_canonical = tmp.path().canonicalize().unwrap();
        let skill_dir = tmp.path().join("skills").join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# My Skill").unwrap();
        let subpath = "skills/my-skill";
        let canon = tmp.path().join(subpath).canonicalize().unwrap();
        assert!(canon.starts_with(&tmp_canonical));
    }
    // frontmatter parsing

    #[test]
    fn parse_frontmatter_new_format() {
        let raw = r#"---
name: my-skill
description: A test skill
license: MIT
compatibility: "Iris 1.0+"
---

# My Skill

Instructions here."#;
        let (meta, body) = parse_frontmatter(raw);
        assert_eq!(meta.get("name").unwrap(), "my-skill");
        assert_eq!(meta.get("description").unwrap(), "A test skill");
        assert_eq!(meta.get("license").unwrap(), "MIT");
        assert!(body.contains("Instructions here"));
    }

    #[test]
    fn parse_frontmatter_legacy_format() {
        let raw = r#"---
name: old-skill
description: Legacy skill
trigger: knowledge
---

# Old Skill"#;
        let (meta, body) = parse_frontmatter(raw);
        assert_eq!(meta.get("trigger").unwrap(), "knowledge");
        assert!(body.contains("# Old Skill"));
    }

    #[test]
    fn parse_frontmatter_no_frontmatter() {
        let raw = "# Just a heading\n\nBody text.";
        let (meta, body) = parse_frontmatter(raw);
        assert!(meta.is_empty());
        assert!(body.contains("# Just a heading"));
    }
    // load_skill with new fields

    #[test]
    fn load_skill_new_format() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: my-skill
description: A test skill
license: MIT
---

# My Skill"#,
        )
        .unwrap();
        let entry = load_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap();
        assert_eq!(entry.name, "my-skill");
        assert_eq!(entry.description, "A test skill");
        assert_eq!(entry.license, Some("MIT".into()));
        assert!(entry.legacy_trigger.is_none());
        assert_eq!(entry.validation_status(), SkillValidationStatus::Valid);
    }

    #[test]
    fn load_skill_legacy_format() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("old-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: old-skill
description: Legacy skill
trigger: knowledge
---

# Old Skill"#,
        )
        .unwrap();
        let entry = load_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap();
        assert_eq!(entry.name, "old-skill");
        assert_eq!(entry.legacy_trigger, Some("knowledge".into()));
        assert_eq!(entry.validation_status(), SkillValidationStatus::Legacy);
    }

    #[test]
    fn new_format_without_frontmatter_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("plain-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Plain Skill\n\nInstructions without Agent Skills frontmatter.",
        )
        .unwrap();

        let entry = load_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap();
        assert!(matches!(
            entry.validation_status(),
            SkillValidationStatus::Invalid(_)
        ));
    }

    #[test]
    fn new_format_name_mismatch_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("directory-name");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: different-name
description: Valid description
---

# Different Name"#,
        )
        .unwrap();

        let entry = load_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap();
        assert!(matches!(
            entry.validation_status(),
            SkillValidationStatus::Invalid(_)
        ));
    }

    #[test]
    fn scan_metadata_does_not_load_instruction_body() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        let skill_dir = vault.join(".iris/skills/meta-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: meta-skill
description: Valid description
---

# Meta Skill

Large instruction body."#,
        )
        .unwrap();

        let entries: Vec<_> = scan_all_metadata(&vault)
            .unwrap()
            .into_iter()
            .filter(|e| e.name == "meta-skill")
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "meta-skill");
        assert!(entries[0].content.is_empty());
    }

    #[test]
    fn load_skill_empty_description_is_invalid() {
        let entry = SkillEntry {
            name: "test".into(),
            description: String::new(),
            license: None,
            compatibility: None,
            metadata: HashMap::new(),
            content: "body".into(),
            scope: SkillScope::Vault,
            enabled: true,
            file_path: "/test".into(),
            legacy_trigger: None,
            ..SkillEntry::default()
        };
        assert!(matches!(
            entry.validation_status(),
            SkillValidationStatus::Invalid(_)
        ));
    }

    #[test]
    fn load_skill_description_too_long_is_invalid() {
        let entry = SkillEntry {
            name: "test".into(),
            description: "x".repeat(1025),
            license: None,
            compatibility: None,
            metadata: HashMap::new(),
            content: "body".into(),
            scope: SkillScope::Vault,
            enabled: true,
            file_path: "/test".into(),
            legacy_trigger: None,
            ..SkillEntry::default()
        };
        assert!(matches!(
            entry.validation_status(),
            SkillValidationStatus::Invalid(_)
        ));
    }

    // Task-intent matching

    fn make_skill(name: &str, legacy_trigger: Option<&str>, enabled: bool) -> SkillEntry {
        SkillEntry {
            name: name.into(),
            description: format!("Skill {name}"),
            license: None,
            compatibility: None,
            metadata: HashMap::new(),
            content: String::new(),
            scope: SkillScope::Vault,
            enabled,
            file_path: format!("/test/{name}"),
            legacy_trigger: legacy_trigger.map(String::from),
            confirmation_status: SkillConfirmationStatus::Confirmed,
            ..SkillEntry::default()
        }
    }

    #[test]
    fn no_trigger_matches_all_scenes() {
        let skills = vec![make_skill("universal", None, true)];
        let matched = skills_for_task(&skills, AgentIntent::AskNotes, "", &[], None);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].name, "universal");
    }

    #[test]
    fn legacy_trigger_matches_scene() {
        let skills = vec![make_skill("knowledge-skill", Some("knowledge"), true)];
        let matched = skills_for_task(&skills, AgentIntent::AskNotes, "", &[], None);
        assert_eq!(matched.len(), 1);
    }

    #[test]
    fn legacy_trigger_wrong_scene_no_match() {
        let skills = vec![make_skill("writing-skill", Some("writing"), true)];
        let matched = skills_for_task(&skills, AgentIntent::AskNotes, "", &[], None);
        assert!(matched.is_empty());
    }

    #[test]
    fn disabled_skill_excluded() {
        let skills = vec![make_skill("disabled", None, false)];
        let matched = skills_for_task(&skills, AgentIntent::AskNotes, "", &[], None);
        assert!(matched.is_empty());
    }

    #[test]
    fn confirmed_skill_hash_is_recorded_and_invalidated_on_edit() {
        let vault = tempfile::tempdir().unwrap();
        let target = PathBuf::from("demo/SKILL.md");
        let actual_target = vault.path().join(".iris/skills/demo/SKILL.md");
        let markdown = "---\nname: demo\ndescription: Demo skill\nscope:\n  - kind: glob\n    pattern: \"notes/**\"\n---\n\nUse demo behavior.\n";

        let entry =
            write_confirmed_skill_content(vault.path(), &target, SkillScope::Vault, markdown)
                .unwrap();
        assert_eq!(
            entry.confirmation_status,
            SkillConfirmationStatus::Confirmed
        );

        let scanned = scan_all(vault.path()).unwrap();
        assert_eq!(
            scanned[0].confirmation_status,
            SkillConfirmationStatus::Confirmed
        );

        std::fs::write(
            &actual_target,
            markdown.replace("demo behavior", "changed behavior"),
        )
        .unwrap();
        let changed = scan_all(vault.path()).unwrap();
        assert_eq!(
            changed[0].confirmation_status,
            SkillConfirmationStatus::NeedsConfirmation
        );
    }

    #[test]
    fn confirmed_skill_rejects_outside_target_without_creating_parent() {
        let vault = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("escape").join("SKILL.md");
        let markdown = "---\nname: escape\ndescription: Escape skill\n---\n\nNo escape.\n";

        let err = write_confirmed_skill_content(vault.path(), &target, SkillScope::Vault, markdown)
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("Skill target path must stay inside the skills directory"));
        assert!(
            !outside.path().join("escape").exists(),
            "rejecting an out-of-scope skill target must not create directories outside .iris/skills"
        );
    }

    #[test]
    fn multiple_skills_filtered() {
        let skills = vec![
            make_skill("a", Some("knowledge"), true),
            make_skill("b", Some("writing"), true),
            make_skill("c", None, true),
        ];
        let matched = skills_for_task(&skills, AgentIntent::AskNotes, "", &[], None);
        assert_eq!(matched.len(), 2); // a + c (universal)
    }
    // BM25 scoring

    #[test]
    fn bm25_exact_trigger_scores_highest() {
        let skills = vec![
            make_skill("universal", None, true),
            make_skill("knowledge-expert", Some("knowledge"), true),
        ];
        let ranked = rank_skills_for_task(&skills, AgentIntent::AskNotes, "", &[], None);
        assert_eq!(ranked.len(), 2);
        // knowledge-expert should score higher (trigger match + possible desc match)
        assert!(ranked[0].score >= ranked[1].score);
    }

    #[test]
    fn bm25_description_keyword_match() {
        let skills = vec![SkillEntry {
            name: "research-helper".into(),
            description: "Helps with research synthesis and evidence gathering".into(),
            license: None,
            compatibility: None,
            metadata: HashMap::new(),
            content: String::new(),
            scope: SkillScope::Vault,
            enabled: true,
            file_path: "/test/research".into(),
            legacy_trigger: None,
            confirmation_status: SkillConfirmationStatus::Confirmed,
            ..SkillEntry::default()
        }];
        let ranked = rank_skills_for_task(&skills, AgentIntent::Research, "", &[], None);
        assert_eq!(ranked.len(), 1);
        assert!(ranked[0].score > 1.0); // More than just the universal base score
    }

    #[test]
    fn bm25_name_match_boost() {
        let skills = vec![SkillEntry {
            name: "knowledge-graph".into(),
            description: "A tool".into(),
            license: None,
            compatibility: None,
            metadata: HashMap::new(),
            content: String::new(),
            scope: SkillScope::Vault,
            enabled: true,
            file_path: "/test/kg".into(),
            legacy_trigger: None,
            confirmation_status: SkillConfirmationStatus::Confirmed,
            ..SkillEntry::default()
        }];
        let ranked = rank_skills_for_task(&skills, AgentIntent::AskNotes, "", &[], None);
        assert_eq!(ranked.len(), 1);
        // Name contains "knowledge", so the score is boosted
        assert!(ranked[0].score > 2.0);
    }

    #[test]
    fn bm25_metadata_keywords_boost() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "keywords".to_string(),
            serde_json::Value::String("research evidence analysis".into()),
        );
        let skills = vec![SkillEntry {
            name: "my-tool".into(),
            description: "A generic tool".into(),
            license: None,
            compatibility: None,
            metadata,
            content: String::new(),
            scope: SkillScope::Vault,
            enabled: true,
            file_path: "/test/tool".into(),
            legacy_trigger: None,
            confirmation_status: SkillConfirmationStatus::Confirmed,
            ..SkillEntry::default()
        }];
        let ranked = rank_skills_for_task(&skills, AgentIntent::Research, "", &[], None);
        assert_eq!(ranked.len(), 1);
        // Keywords match, so the score is boosted
        assert!(ranked[0].score > 2.0);
    }
    // Dependency management

    #[test]
    fn depends_from_string_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "depends".to_string(),
            serde_json::Value::String("base-skill helper-skill".into()),
        );
        let entry = SkillEntry {
            name: "child".into(),
            description: "Child skill".into(),
            license: None,
            compatibility: None,
            metadata,
            content: String::new(),
            scope: SkillScope::Vault,
            enabled: true,
            file_path: "/test/child".into(),
            legacy_trigger: None,
            ..SkillEntry::default()
        };
        assert_eq!(entry.depends(), vec!["base-skill", "helper-skill"]);
    }

    #[test]
    fn depends_from_array_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "depends".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("alpha".into()),
                serde_json::Value::String("beta".into()),
            ]),
        );
        let entry = SkillEntry {
            name: "child".into(),
            description: "Child skill".into(),
            license: None,
            compatibility: None,
            metadata,
            content: String::new(),
            scope: SkillScope::Vault,
            enabled: true,
            file_path: "/test/child".into(),
            legacy_trigger: None,
            ..SkillEntry::default()
        };
        assert_eq!(entry.depends(), vec!["alpha", "beta"]);
    }

    #[test]
    fn missing_dependencies_detected() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "depends".to_string(),
            serde_json::Value::String("installed-skill missing-skill".into()),
        );
        let entry = SkillEntry {
            name: "child".into(),
            description: "Child skill".into(),
            license: None,
            compatibility: None,
            metadata,
            content: String::new(),
            scope: SkillScope::Vault,
            enabled: true,
            file_path: "/test/child".into(),
            legacy_trigger: None,
            ..SkillEntry::default()
        };
        let installed = vec!["installed-skill".to_string(), "other".to_string()];
        let missing = entry.missing_dependencies(&installed);
        assert_eq!(missing, vec!["missing-skill"]);
    }
    // Migration

    #[test]
    fn migrate_legacy_skill_converts_format() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("old-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: old-skill
description: A legacy skill
trigger: knowledge
---

# Old Skill

Instructions here."#,
        )
        .unwrap();

        let entry = migrate_legacy_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap();
        assert_eq!(entry.name, "old-skill");
        assert!(entry.legacy_trigger.is_none()); // trigger removed
        assert_eq!(entry.validation_status(), SkillValidationStatus::Valid);

        // Backup should exist
        assert!(skill_dir.join("SKILL.md.bak").exists());

        // Content should still be there
        assert!(entry.content.contains("Instructions here"));
    }

    #[test]
    fn migrate_non_legacy_fails() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("new-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: new-skill
description: Already new format
---

# New Skill"#,
        )
        .unwrap();

        let err = migrate_legacy_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap_err();
        assert!(err.to_string().contains("new format"));
    }

    #[test]
    fn is_legacy_format_detects_trigger() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("legacy.md"),
            "---\nname: x\ndescription: y\ntrigger: knowledge\n---\n\nbody",
        )
        .unwrap();
        fs::write(
            dir.path().join("new.md"),
            "---\nname: x\ndescription: y\n---\n\nbody",
        )
        .unwrap();
        assert!(is_legacy_format(&dir.path().join("legacy.md")));
        assert!(!is_legacy_format(&dir.path().join("new.md")));
    }

    // Active skills regression

    #[test]
    fn inject_into_prompt_only_includes_enabled_skills() {
        let vault = tempfile::tempdir().unwrap();
        let skills = vec![
            make_skill("enabled-one", None, true),
            make_skill("disabled-one", None, false),
            make_skill("enabled-two", None, true),
        ];
        let prompt = inject_into_prompt(vault.path(), &skills, AgentIntent::AskNotes, "");
        assert!(prompt.contains("enabled-one"));
        assert!(prompt.contains("enabled-two"));
        assert!(!prompt.contains("disabled-one"));
    }

    #[test]
    fn inject_into_prompt_empty_when_no_skills() {
        let vault = tempfile::tempdir().unwrap();
        let skills: Vec<SkillEntry> = vec![];
        let prompt = inject_into_prompt(vault.path(), &skills, AgentIntent::AskNotes, "");
        assert!(prompt.is_empty());
    }

    #[test]
    fn inject_into_prompt_truncates_large_skill_body() {
        let vault = tempfile::tempdir().unwrap();
        let mut skill = make_skill("large-skill", None, true);
        skill.content = format!("start\n{}\nend", "x".repeat(80_000));

        let prompt = inject_into_prompt(vault.path(), &[skill], AgentIntent::AskNotes, "");

        assert!(prompt.contains("large-skill"));
        assert!(prompt.contains("start"));
        assert!(prompt.contains("[skill content truncated"));
        assert!(!prompt.contains("\nend\n"));
        assert!(prompt.len() < 30_000);
    }
}
