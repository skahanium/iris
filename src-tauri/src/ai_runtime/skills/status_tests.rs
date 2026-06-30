use super::*;

#[test]
fn scan_status_marks_skill_md_only_as_prompt_only_without_mcp_requirement() {
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

    let entries = scan_all_with_status(&vault).unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry.skill.name == "simple-skill")
        .unwrap();

    assert_eq!(entry.kind, SkillManifestKind::LegacyPromptOnly);
    assert_eq!(entry.runtime_kind, "not_applicable");
    assert!(entry.runtime_ready);
    assert!(entry.mcp_dependencies.is_empty());
    assert!(!entry.workspace_declared);
}

#[test]
fn scan_status_reports_mcp_dependent_manifest_as_runtime_unavailable_without_profile() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let skill_dir = vault.join(".iris/skills/anysearch");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: anysearch
description: Search through AnySearch MCP
iris_manifest: iris.skill.toml
---

# AnySearch"#,
    )
    .unwrap();
    fs::write(
        skill_dir.join("iris.skill.toml"),
        r#"schema_version = "1"
name = "anysearch"
kind = "mcp_dependent"

[capabilities]
requires = ["web.search"]

[[mcp.dependencies]]
profile_id = "anysearch"
required_capabilities = ["web.search"]
required = true

[degradation]
when_runtime_missing = "partial"
message = "Enable AnySearch MCP profile."
"#,
    )
    .unwrap();

    let entries = scan_all_with_status(&vault).unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry.skill.name == "anysearch")
        .unwrap();

    assert_eq!(entry.kind, SkillManifestKind::McpDependent);
    assert_eq!(entry.runtime_kind, "mcp");
    assert!(!entry.runtime_ready);
    assert_eq!(entry.availability, "partial");
    assert_eq!(entry.mcp_dependencies, vec!["anysearch".to_string()]);
    assert_eq!(entry.activated_sections, vec!["skill_overlay".to_string()]);
    assert_eq!(entry.blocked_sections, vec!["runtime".to_string()]);
    assert!(entry
        .degraded_reasons
        .iter()
        .any(|reason| reason.contains("AnySearch MCP profile")));
}

#[test]
fn scan_status_uses_typed_manifest_workspace_contract() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let skill_dir = vault.join(".iris/skills/workspace-manifest-skill");
    fs::create_dir_all(skill_dir.join("resources")).unwrap();
    fs::write(skill_dir.join("resources/seed.md"), "# Seed\n").unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: workspace-manifest-skill
description: Uses typed workspace manifest
iris_manifest: iris.skill.toml
---

# Workspace Manifest Skill"#,
    )
    .unwrap();
    fs::write(
        skill_dir.join("iris.skill.toml"),
        r#"schema_version = "1"
name = "workspace-manifest-skill"
kind = "workspace"

[workspace]
declared = true
folders = ["inputs", "outputs"]

[[workspace.documents]]
source = "resources/seed.md"
target = "README.md"
"#,
    )
    .unwrap();

    let entries = scan_all_with_status(&vault).unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry.skill.name == "workspace-manifest-skill")
        .unwrap();

    assert_eq!(entry.kind, SkillManifestKind::Workspace);
    assert!(entry.workspace_declared);
    assert!(!entry.workspace_prepared);
    assert_eq!(entry.availability, "partial");
    assert_eq!(entry.generated_files_count, 0);
    assert_eq!(
        entry.workspace_missing_items,
        vec![
            "inputs/".to_string(),
            "outputs/".to_string(),
            "README.md".to_string(),
        ]
    );
    assert_eq!(entry.activated_sections, vec!["skill_overlay".to_string()]);
    assert_eq!(entry.blocked_sections, vec!["workspace".to_string()]);
}

#[test]
fn scan_status_blocks_prompt_section_with_missing_required_resource() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let skill_dir = vault.join(".iris/skills/section-resource-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: section-resource-skill
description: Section requires a resource
iris_manifest: iris.skill.toml
---

# Section Resource Skill"#,
    )
    .unwrap();
    fs::write(
        skill_dir.join("iris.skill.toml"),
        r#"schema_version = "1"
name = "section-resource-skill"
kind = "resource"

[prompt]
default_sections = ["main"]

[[prompt.sections]]
id = "main"
source = "SKILL.md"
requires_resources = ["resources/missing.md"]
"#,
    )
    .unwrap();

    let entries = scan_all_with_status(&vault).unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry.skill.name == "section-resource-skill")
        .unwrap();

    assert!(entry.activated_sections.is_empty());
    assert_eq!(entry.blocked_sections, vec!["main".to_string()]);
    assert_eq!(entry.availability, "partial");
    assert!(entry
        .degraded_reasons
        .iter()
        .any(|reason| reason.contains("required resource `resources/missing.md`")));
}

#[test]
fn scan_status_uses_typed_manifest_required_resources() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let skill_dir = vault.join(".iris/skills/resource-manifest-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: resource-manifest-skill
description: Uses typed resource manifest
iris_manifest: iris.skill.toml
---

# Resource Manifest Skill"#,
    )
    .unwrap();
    fs::write(
        skill_dir.join("iris.skill.toml"),
        r#"schema_version = "1"
name = "resource-manifest-skill"
kind = "resource"

[resources]
required = ["resources/missing.md"]
optional = ["references/optional.md"]
"#,
    )
    .unwrap();

    let entries = scan_all_with_status(&vault).unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry.skill.name == "resource-manifest-skill")
        .unwrap();

    assert_eq!(entry.kind, SkillManifestKind::Resource);
    assert_eq!(entry.availability, "partial");
    assert!(entry
        .degraded_reasons
        .iter()
        .any(|reason| reason.contains("required resource `resources/missing.md`")));
    assert!(entry
        .degraded_reasons
        .iter()
        .any(|reason| reason.contains("optional resource `references/optional.md`")));
}
