use super::*;

#[test]
fn scan_status_marks_confirmed_skill_as_prompt_only_available() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let skill_dir = vault.join(".iris/skills/simple-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: simple-skill
description: Simple prompt-only skill
---

# Simple Skill

Use a concise review style."#,
    )
    .unwrap();

    let mut config = load_config(SkillScope::Vault, &vault);
    let hash = skill_content_hash_for_path(&skill_dir.join("SKILL.md")).unwrap();
    config
        .confirmed_hashes
        .insert(skill_key(SkillScope::Vault, "simple-skill"), hash);
    save_config(SkillScope::Vault, &vault, &config).unwrap();

    let entries = scan_all_with_status(&vault).unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry.skill.name == "simple-skill")
        .unwrap();

    assert_eq!(entry.kind, SkillManifestKind::LegacyPromptOnly);
    assert_eq!(
        entry.skill.confirmation_status,
        SkillConfirmationStatus::Confirmed
    );
    assert_eq!(entry.activated_sections, vec!["skill_overlay"]);
    assert!(entry.activation_ready);
    assert_eq!(entry.availability, "available");
}

#[test]
fn scan_status_requires_confirmation_for_unconfirmed_skill() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let skill_dir = vault.join(".iris/skills/draft-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: draft-skill
description: Draft prompt-only skill
---

# Draft Skill

Keep responses terse."#,
    )
    .unwrap();

    let entries = scan_all_with_status(&vault).unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry.skill.name == "draft-skill")
        .unwrap();

    assert_eq!(
        entry.skill.confirmation_status,
        SkillConfirmationStatus::NeedsConfirmation
    );
    assert!(!entry.activation_ready);
    assert_eq!(entry.availability, "partial");
}
