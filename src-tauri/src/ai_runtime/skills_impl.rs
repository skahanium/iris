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
    activated_skills_from_plan, build_skill_activation_plan_for_task,
    build_skill_activation_plan_for_task_with_query_embedding,
    build_skill_activation_plan_for_task_with_runtime, enrich_list_with_task,
    filter_skill_content_to_injected_sections, load_activation_index, rank_skills_for_task,
    rebuild_activation_index, rerank_skills_with_vectors, skills_for_task,
};
pub(crate) use activation_impl::{
    activation_embedding_source, SKILL_VECTOR_RERANK_DEFAULT_ENABLED,
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
pub use prompt_impl::{inject_into_prompt, inject_selected_skills_into_prompt};
pub use scan_impl::{
    load_skill, scan_all, scan_all_metadata, scan_all_with_status, skill_content_hash_for_path,
    skill_list_entries,
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
    fn no_trigger_does_not_activate_without_task_evidence() {
        let skills = vec![make_skill("universal", None, true)];
        let matched = skills_for_task(&skills, AgentIntent::AskNotes, "", &[], None);
        assert!(matched.is_empty());
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
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].name, "a");
    }

    #[test]
    fn activation_plan_limits_prompt_overlay_to_primary_and_auxiliary_skill() {
        let skills = vec![
            make_skill("first", Some("knowledge"), true),
            make_skill("second", Some("knowledge"), true),
            make_skill("third", Some("knowledge"), true),
            make_skill("fourth", Some("knowledge"), true),
        ];

        let plan =
            build_skill_activation_plan_for_task(&skills, AgentIntent::AskNotes, "", &[], None);

        assert_eq!(plan.activated_skills.len(), 2);
        assert_eq!(
            plan.skill_overlay_summary,
            "2 prompt-only skill(s) activated."
        );
    }

    #[test]
    fn activation_index_selects_the_two_best_skills_without_loading_skill_files() {
        let skills = vec![
            make_skill("general", None, true),
            make_skill("primary-audit", None, true),
            make_skill("auxiliary-audit", None, true),
            make_skill("unrelated", None, true),
        ];
        let mut index = ActivationIndexMap::new();
        for (name, keywords) in [
            ("general", "general"),
            ("primary-audit", "forensic incident audit"),
            ("auxiliary-audit", "forensic audit evidence"),
            ("unrelated", "gardening plants"),
        ] {
            index.insert(
                (name.to_string(), SkillScope::Vault),
                SkillActivationIndexRow {
                    skill_name: name.to_string(),
                    scope: SkillScope::Vault,
                    description: None,
                    keywords: Some(keywords.to_string()),
                    embedding_json: None,
                    embedding_source_hash: String::new(),
                    embedding_model: None,
                    embedding_dimensions: None,
                },
            );
        }

        let plan = build_skill_activation_plan_for_task(
            &skills,
            AgentIntent::Chat,
            "perform a forensic audit",
            &[],
            Some(&index),
        );

        let names: Vec<_> = plan
            .activated_skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"primary-audit"));
        assert!(names.contains(&"auxiliary-audit"));
        assert!(!names.contains(&"general"));
        assert!(!names.contains(&"unrelated"));
    }

    #[test]
    fn activation_index_incremental_upsert_preserves_unchanged_embedding() {
        let db = Database::open_in_memory().unwrap();
        let skill = make_skill("stable-skill", Some("knowledge"), true);
        rebuild_activation_index(&db, std::slice::from_ref(&skill)).unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE skill_activation_index
                 SET embedding_json = '[0.25,0.75]',
                     embedding_model = 'test-model',
                     embedding_dimensions = 2
                 WHERE skill_name = 'stable-skill' AND scope = 'Vault'",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        rebuild_activation_index(&db, std::slice::from_ref(&skill)).unwrap();

        let stored = db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT embedding_json, embedding_source_hash,
                            embedding_model, embedding_dimensions
                     FROM skill_activation_index
                     WHERE skill_name = 'stable-skill' AND scope = 'Vault'",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<i64>>(3)?,
                        ))
                    },
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(stored.0.as_deref(), Some("[0.25,0.75]"));
        assert!(!stored.1.is_empty());
        assert_eq!(stored.2.as_deref(), Some("test-model"));
        assert_eq!(stored.3, Some(2));
    }

    #[test]
    fn activation_index_clears_changed_embedding_and_deletes_only_removed_rows() {
        let db = Database::open_in_memory().unwrap();
        let mut changed = make_skill("changed-skill", Some("knowledge"), true);
        let removed = make_skill("removed-skill", Some("knowledge"), true);
        rebuild_activation_index(&db, &[changed.clone(), removed]).unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE skill_activation_index
                 SET embedding_json = '[1.0]',
                     embedding_model = 'test-model',
                     embedding_dimensions = 1",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        changed.description = "A changed semantic description".into();

        rebuild_activation_index(&db, &[changed]).unwrap();

        db.with_read_conn(|conn| {
            let changed_row = conn.query_row(
                "SELECT embedding_json, embedding_model, embedding_dimensions
                 FROM skill_activation_index
                 WHERE skill_name = 'changed-skill' AND scope = 'Vault'",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                },
            )?;
            assert_eq!(changed_row, (None, None, None));
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM skill_activation_index
                     WHERE skill_name = 'removed-skill' AND scope = 'Vault'",
                    [],
                    |row| row.get::<_, i64>(0),
                )?,
                0
            );
            Ok(())
        })
        .unwrap();
    }

    fn axis_vector(axis: usize) -> Vec<f32> {
        let mut vector = vec![0.0; crate::embedding::engine::EMBEDDING_DIMENSION];
        vector[axis] = 1.0;
        vector
    }

    fn write_activation_vector(
        db: &Database,
        name: &str,
        vector: &[f32],
        model: &str,
        dimensions: i64,
    ) {
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE skill_activation_index
                 SET embedding_json = ?1,
                     embedding_model = ?2,
                     embedding_dimensions = ?3
                 WHERE skill_name = ?4 AND scope = 'Vault'",
                rusqlite::params![serde_json::to_string(vector)?, model, dimensions, name,],
            )?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn vector_rerank_only_reorders_lexical_candidates_with_matching_metadata() {
        let db = Database::open_in_memory().unwrap();
        let mut alpha = make_skill("alpha-assistant", None, true);
        alpha.description = "assistant release summary".into();
        let mut beta = make_skill("beta-assistant", None, true);
        beta.description = "assistant release analysis".into();
        let mut unrelated = make_skill("garden-guide", None, true);
        unrelated.description = "gardening and plants".into();
        let skills = vec![alpha, beta, unrelated];
        rebuild_activation_index(&db, &skills).unwrap();
        write_activation_vector(
            &db,
            "alpha-assistant",
            &axis_vector(0),
            crate::embedding::engine::EMBEDDING_MODEL_FINGERPRINT,
            crate::embedding::engine::EMBEDDING_DIMENSION as i64,
        );
        write_activation_vector(
            &db,
            "beta-assistant",
            &axis_vector(1),
            crate::embedding::engine::EMBEDDING_MODEL_FINGERPRINT,
            crate::embedding::engine::EMBEDDING_DIMENSION as i64,
        );
        write_activation_vector(
            &db,
            "garden-guide",
            &axis_vector(1),
            crate::embedding::engine::EMBEDDING_MODEL_FINGERPRINT,
            crate::embedding::engine::EMBEDDING_DIMENSION as i64,
        );
        let index = load_activation_index(&db).unwrap();
        let lexical = rank_skills_for_task(
            &skills,
            AgentIntent::Chat,
            "launch readiness",
            &[],
            Some(&index),
        );

        let reranked = rerank_skills_with_vectors(lexical, Some(&axis_vector(1)), Some(&index));

        assert_eq!(
            reranked
                .iter()
                .map(|scored| scored.skill.name.as_str())
                .collect::<Vec<_>>(),
            ["beta-assistant", "alpha-assistant"]
        );
        assert!(
            reranked
                .iter()
                .all(|scored| scored.skill.name != "garden-guide"),
            "vector similarity must not introduce a zero-lexical-correlation Skill"
        );
    }

    #[test]
    fn vector_model_or_dimension_mismatch_falls_back_to_lexical_order() {
        let db = Database::open_in_memory().unwrap();
        let mut alpha = make_skill("alpha-assistant", None, true);
        alpha.description = "assistant".into();
        let mut beta = make_skill("beta-assistant", None, true);
        beta.description = "assistant".into();
        let skills = vec![alpha, beta];
        rebuild_activation_index(&db, &skills).unwrap();
        write_activation_vector(
            &db,
            "alpha-assistant",
            &axis_vector(0),
            crate::embedding::engine::EMBEDDING_MODEL_ID,
            crate::embedding::engine::EMBEDDING_DIMENSION as i64,
        );
        write_activation_vector(
            &db,
            "beta-assistant",
            &axis_vector(1),
            crate::embedding::engine::EMBEDDING_MODEL_ID,
            crate::embedding::engine::EMBEDDING_DIMENSION as i64,
        );
        let index = load_activation_index(&db).unwrap();
        let lexical =
            rank_skills_for_task(&skills, AgentIntent::Chat, "上线评估", &[], Some(&index));
        let lexical_names = lexical
            .iter()
            .map(|scored| scored.skill.name.as_str())
            .collect::<Vec<_>>();

        let reranked = rerank_skills_with_vectors(lexical, Some(&axis_vector(1)), Some(&index));

        assert_eq!(
            reranked
                .iter()
                .map(|scored| scored.skill.name.as_str())
                .collect::<Vec<_>>(),
            lexical_names
        );

        write_activation_vector(
            &db,
            "alpha-assistant",
            &axis_vector(0),
            crate::embedding::engine::EMBEDDING_MODEL_FINGERPRINT,
            crate::embedding::engine::EMBEDDING_DIMENSION as i64,
        );
        write_activation_vector(
            &db,
            "beta-assistant",
            &axis_vector(1),
            crate::embedding::engine::EMBEDDING_MODEL_FINGERPRINT,
            (crate::embedding::engine::EMBEDDING_DIMENSION - 1) as i64,
        );
        let mismatched_dimensions_index = load_activation_index(&db).unwrap();
        let lexical = rank_skills_for_task(
            &skills,
            AgentIntent::Chat,
            "上线评估",
            &[],
            Some(&mismatched_dimensions_index),
        );
        let lexical_names = lexical
            .iter()
            .map(|scored| scored.skill.name.as_str())
            .collect::<Vec<_>>();

        let reranked = rerank_skills_with_vectors(
            lexical,
            Some(&axis_vector(1)),
            Some(&mismatched_dimensions_index),
        );

        assert_eq!(
            reranked
                .iter()
                .map(|scored| scored.skill.name.as_str())
                .collect::<Vec<_>>(),
            lexical_names
        );
    }

    #[test]
    fn partial_vector_coverage_falls_back_to_lexical_order() {
        let db = Database::open_in_memory().unwrap();
        let mut alpha = make_skill("alpha-assistant", None, true);
        alpha.description = "assistant".into();
        let mut beta = make_skill("beta-assistant", None, true);
        beta.description = "assistant".into();
        let skills = vec![alpha, beta];
        rebuild_activation_index(&db, &skills).unwrap();
        write_activation_vector(
            &db,
            "beta-assistant",
            &axis_vector(1),
            crate::embedding::engine::EMBEDDING_MODEL_FINGERPRINT,
            crate::embedding::engine::EMBEDDING_DIMENSION as i64,
        );
        let index = load_activation_index(&db).unwrap();
        let lexical =
            rank_skills_for_task(&skills, AgentIntent::Chat, "上线评估", &[], Some(&index));
        let lexical_names = lexical
            .iter()
            .map(|scored| scored.skill.name.as_str())
            .collect::<Vec<_>>();

        let reranked = rerank_skills_with_vectors(lexical, Some(&axis_vector(1)), Some(&index));

        assert_eq!(
            reranked
                .iter()
                .map(|scored| scored.skill.name.as_str())
                .collect::<Vec<_>>(),
            lexical_names
        );
    }

    #[test]
    fn explicit_name_trigger_hint_and_keyword_stay_ahead_of_vector_scores() {
        let db = Database::open_in_memory().unwrap();
        let explicit = make_skill("release-review", None, true);
        let mut hinted = make_skill("hinted-skill", None, true);
        hinted.metadata.insert(
            "trigger-hints".into(),
            serde_json::Value::Array(vec![serde_json::Value::String("混合复盘".into())]),
        );
        let mut keyword = make_skill("keyword-skill", None, true);
        keyword.metadata.insert(
            "keywords".into(),
            serde_json::Value::Array(vec![serde_json::Value::String("发布".into())]),
        );
        let mut semantic = make_skill("semantic-assistant", None, true);
        semantic.description = "assistant".into();
        let skills = vec![explicit, hinted, keyword, semantic];
        rebuild_activation_index(&db, &skills).unwrap();
        write_activation_vector(
            &db,
            "semantic-assistant",
            &axis_vector(1),
            crate::embedding::engine::EMBEDDING_MODEL_FINGERPRINT,
            crate::embedding::engine::EMBEDDING_DIMENSION as i64,
        );
        let index = load_activation_index(&db).unwrap();

        for (query, expected, reason) in [
            (
                "请使用 release-review",
                "release-review",
                "explicit_skill_mention",
            ),
            ("做一次混合复盘", "hinted-skill", "trigger_hint"),
            ("发布复盘", "keyword-skill", "keyword_match"),
        ] {
            let plan = build_skill_activation_plan_for_task_with_query_embedding(
                &skills,
                AgentIntent::Chat,
                query,
                &[],
                Some(&index),
                Some(&axis_vector(1)),
            );
            assert_eq!(plan.activated_skills[0].name, expected, "query {query}");
            assert_eq!(
                plan.activated_skills[0].match_reason, reason,
                "query {query}"
            );
        }
    }

    #[test]
    fn keyword_match_stays_ahead_of_unbounded_lexical_score() {
        let db = Database::open_in_memory().unwrap();
        let mut keyword = make_skill("keyword-skill", None, true);
        keyword.metadata.insert(
            "keywords".into(),
            serde_json::Value::Array(vec![serde_json::Value::String("发布".into())]),
        );
        let terms = [
            "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "theta", "iota", "kappa",
            "lambda", "sigma", "omega", "vector", "semantic", "ranking",
        ];
        let mut lexical = make_skill(&terms.join("-"), None, true);
        lexical.description = format!("assistant {}", terms.join(" "));
        let skills = vec![keyword, lexical];
        rebuild_activation_index(&db, &skills).unwrap();
        let index = load_activation_index(&db).unwrap();
        let query = format!("发布 {}", terms.join(" "));

        let plan = build_skill_activation_plan_for_task(
            &skills,
            AgentIntent::Chat,
            &query,
            &[],
            Some(&index),
        );

        assert_eq!(plan.activated_skills[0].name, "keyword-skill");
        assert_eq!(plan.activated_skills[0].match_reason, "keyword_match");
    }

    fn skill_activation_eval_skills() -> Vec<SkillEntry> {
        let mut chinese = make_skill("chinese-summary", None, true);
        chinese.description = "将中文内容压缩成简短摘要".into();
        chinese.metadata.insert(
            "keywords".into(),
            serde_json::Value::Array(vec![serde_json::Value::String("摘要".into())]),
        );
        let mut mixed = make_skill("mixed-review", None, true);
        mixed.description = "Review bilingual Chinese and English content".into();
        mixed.metadata.insert(
            "trigger-hints".into(),
            serde_json::Value::Array(vec![serde_json::Value::String("bilingual review".into())]),
        );
        let mut explicit = make_skill("explicit-audit", None, true);
        explicit.description = "Audit a document against explicit requirements".into();
        let mut alpha = make_skill("alpha-general", None, true);
        alpha.description = "assistant for gardening and plant care".into();
        let mut beta = make_skill("beta-general", None, true);
        beta.description = "assistant for financial bookkeeping".into();
        let mut synonym = make_skill("z-release-readiness", None, true);
        synonym.description = "assistant for evaluating release readiness and launch risk".into();
        let mut high_risk = make_skill("destroy-vault", None, true);
        high_risk.description = "irreversible destructive action".into();
        high_risk.content = "assistant".into();
        vec![chinese, mixed, explicit, alpha, beta, synonym, high_risk]
    }

    const PINNED_SKILL_ACTIVATION_EVAL_MODEL_REVISION: &str =
        "Xenova/bge-small-zh-v1.5@fcecc3c5fef6becfa2b2bdda15c1c938857be534";
    const PINNED_SKILL_ACTIVATION_SIMILARITIES: [[f32; 7]; 5] = [
        [
            0.5876449, 0.48451254, 0.39956492, 0.44972652, 0.4106296, 0.42367074, 0.35393217,
        ],
        [
            0.48023725, 0.70686156, 0.51498735, 0.46826154, 0.48906836, 0.5178797, 0.43215537,
        ],
        [
            0.44649324, 0.5241725, 0.78972745, 0.4807908, 0.5220992, 0.4969171, 0.49795252,
        ],
        [
            0.37048265, 0.41266617, 0.4126317, 0.34202388, 0.40696642, 0.44630638, 0.36816576,
        ],
        [
            0.33335194, 0.295247, 0.29608002, 0.3597544, 0.2619487, 0.29565835, 0.3024949,
        ],
    ];

    fn vector_with_cosine_to_first_axis(similarity: f32) -> Vec<f32> {
        let mut vector = vec![0.0; crate::embedding::engine::EMBEDDING_DIMENSION];
        vector[0] = similarity;
        vector[1] = (1.0 - similarity * similarity).sqrt();
        vector
    }

    #[test]
    #[ignore = "regenerates the pinned BGE similarity fixture on maintainer request"]
    fn print_pinned_skill_activation_eval_similarities() {
        let db = Database::open_in_memory().unwrap();
        let skills = skill_activation_eval_skills();
        rebuild_activation_index(&db, &skills).unwrap();
        let index = load_activation_index(&db).unwrap();
        let sources = skills
            .iter()
            .map(|skill| {
                let row = index
                    .get(&(skill.name.clone(), skill.scope))
                    .expect("activation index row");
                activation_embedding_source(
                    &skill.name,
                    &skill.description,
                    row.keywords.as_deref().unwrap_or(""),
                )
            })
            .collect::<Vec<_>>();
        let queries = [
            "写摘要",
            "请做 bilingual review 混合复盘",
            "请使用 explicit-audit",
            "发版前看看能不能上线",
            "整理旅行照片",
        ];
        let mut texts = queries.to_vec();
        texts.extend(sources.iter().map(String::as_str));
        let embeddings = crate::embedding::engine::embed_texts_batch(&texts).unwrap();
        let query_embeddings = &embeddings[..queries.len()];
        let skill_embeddings = &embeddings[queries.len()..];

        for (query, query_embedding) in queries.iter().zip(query_embeddings) {
            let similarities = skill_embeddings
                .iter()
                .map(|skill_embedding| {
                    crate::embedding::engine::cosine_similarity(query_embedding, skill_embedding)
                })
                .collect::<Vec<_>>();
            println!("{query}: {similarities:?}");
        }
    }

    #[test]
    fn skill_activation_eval_gates_default_vector_rerank_on_recall_and_high_risk_safety() {
        let db = Database::open_in_memory().unwrap();
        let skills = skill_activation_eval_skills();
        rebuild_activation_index(&db, &skills).unwrap();
        let cases = [
            ("中文短查询", "写摘要", Some("chinese-summary")),
            (
                "混合语言",
                "请做 bilingual review 混合复盘",
                Some("mixed-review"),
            ),
            ("显式提及", "请使用 explicit-audit", Some("explicit-audit")),
            (
                "同义表达",
                "发版前看看能不能上线",
                Some("z-release-readiness"),
            ),
            ("高风险误激活", "整理旅行照片", None),
        ];
        let mut lexical_recall = 0;
        let mut vector_recall = 0;
        let mut lexical_high_risk = 0;
        let mut vector_high_risk = 0;

        assert_eq!(
            crate::embedding::engine::EMBEDDING_MODEL_FINGERPRINT,
            PINNED_SKILL_ACTIVATION_EVAL_MODEL_REVISION
        );
        assert_eq!(
            crate::embedding::engine::EMBEDDING_MODEL_REVISION,
            "fcecc3c5fef6becfa2b2bdda15c1c938857be534"
        );
        for (case_index, (label, query, expected)) in cases.into_iter().enumerate() {
            for (skill, similarity) in skills
                .iter()
                .zip(PINNED_SKILL_ACTIVATION_SIMILARITIES[case_index])
            {
                write_activation_vector(
                    &db,
                    &skill.name,
                    &vector_with_cosine_to_first_axis(similarity),
                    crate::embedding::engine::EMBEDDING_MODEL_FINGERPRINT,
                    crate::embedding::engine::EMBEDDING_DIMENSION as i64,
                );
            }
            let index = load_activation_index(&db).unwrap();
            let lexical = build_skill_activation_plan_for_task(
                &skills,
                AgentIntent::Chat,
                query,
                &[],
                Some(&index),
            );
            let lexical_cohort =
                rank_skills_for_task(&skills, AgentIntent::Chat, query, &[], Some(&index));
            let vector = build_skill_activation_plan_for_task_with_query_embedding(
                &skills,
                AgentIntent::Chat,
                query,
                &[],
                Some(&index),
                Some(&axis_vector(0)),
            );
            let lexical_names = lexical
                .activated_skills
                .iter()
                .map(|skill| skill.name.as_str())
                .collect::<Vec<_>>();
            let vector_names = vector
                .activated_skills
                .iter()
                .map(|skill| skill.name.as_str())
                .collect::<Vec<_>>();
            if let Some(expected) = expected {
                lexical_recall += usize::from(lexical_names.contains(&expected));
                vector_recall += usize::from(vector_names.contains(&expected));
            }
            lexical_high_risk += usize::from(lexical_names.contains(&"destroy-vault"));
            vector_high_risk += usize::from(vector_names.contains(&"destroy-vault"));
            if label == "高风险误激活" {
                assert!(
                    lexical_cohort
                        .iter()
                        .any(|candidate| candidate.skill.name == "destroy-vault"),
                    "the high-risk fixture must exercise a real weak lexical candidate"
                );
            }
            assert!(
                vector.activated_skills.len() <= 2,
                "{label} may activate only primary + auxiliary"
            );
            assert!(vector.requested_tools.is_empty(), "{label}");
            assert!(vector.confirmation_required_tools.is_empty(), "{label}");
            assert!(vector.blocked_capabilities.is_empty(), "{label}");
        }

        let gate_passed = vector_recall > lexical_recall && vector_high_risk <= lexical_high_risk;
        assert_eq!(
            SKILL_VECTOR_RERANK_DEFAULT_ENABLED, gate_passed,
            "default vector rerank must track the activation evaluation gate"
        );
        assert_eq!((lexical_recall, vector_recall), (3, 4));
        assert_eq!((lexical_high_risk, vector_high_risk), (0, 0));
    }

    #[test]
    fn activation_plan_resolves_only_its_cached_primary_and_auxiliary_entries() {
        let skills = vec![
            make_skill("first", Some("knowledge"), true),
            make_skill("second", Some("knowledge"), true),
            make_skill("third", Some("knowledge"), true),
        ];
        let plan =
            build_skill_activation_plan_for_task(&skills, AgentIntent::AskNotes, "", &[], None);

        let resolved = activated_skills_from_plan(&plan, &skills);
        let resolved_names: Vec<_> = resolved.iter().map(|skill| skill.name.as_str()).collect();
        let planned_names: Vec<_> = plan
            .activated_skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect();

        assert_eq!(resolved_names, planned_names);
        assert_eq!(resolved.len(), 2);
    }
    // BM25 scoring

    #[test]
    fn bm25_exact_trigger_scores_highest() {
        let skills = vec![
            make_skill("universal", None, true),
            make_skill("knowledge-expert", Some("knowledge"), true),
        ];
        let ranked = rank_skills_for_task(&skills, AgentIntent::AskNotes, "", &[], None);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].skill.name, "knowledge-expert");
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
            make_skill("enabled-one", Some("knowledge"), true),
            make_skill("disabled-one", Some("knowledge"), false),
            make_skill("enabled-two", Some("knowledge"), true),
        ];
        let prompt = inject_into_prompt(vault.path(), &skills, AgentIntent::AskNotes, "");
        assert!(prompt.contains("enabled-one"));
        assert!(prompt.contains("enabled-two"));
        assert!(!prompt.contains("disabled-one"));
    }

    #[test]
    fn inject_selected_skills_is_capped_at_primary_and_auxiliary() {
        let vault = tempfile::tempdir().unwrap();
        let skills = vec![
            make_skill("primary", None, true),
            make_skill("auxiliary", None, true),
            make_skill("overflow", None, true),
        ];

        let prompt = inject_selected_skills_into_prompt(vault.path(), &skills);

        assert!(prompt.contains("primary"));
        assert!(prompt.contains("auxiliary"));
        assert!(!prompt.contains("overflow"));
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
        let mut skill = make_skill("large-skill", Some("knowledge"), true);
        skill.content = format!("start\n{}\nend", "x".repeat(80_000));

        let prompt = inject_into_prompt(vault.path(), &[skill], AgentIntent::AskNotes, "");

        assert!(prompt.contains("large-skill"));
        assert!(prompt.contains("start"));
        assert!(prompt.contains("[skill content truncated"));
        assert!(!prompt.contains("\nend\n"));
        assert!(prompt.len() < 30_000);
    }
}
