use super::*;

#[test]
fn load_skill_rejects_long_compatibility() {
    let dir = tempfile::tempdir().unwrap();
    let skill_dir = dir.path().join("bad-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        format!(
            "---\nname: bad-skill\ndescription: test\ncompatibility: {}\n---\n\nbody",
            "x".repeat(501)
        ),
    )
    .unwrap();
    let err = load_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap_err();
    assert!(err.to_string().contains("compatibility exceeds 500"));
}

#[test]
fn load_skill_rejects_long_description() {
    let dir = tempfile::tempdir().unwrap();
    let skill_dir = dir.path().join("bad-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        format!(
            "---\nname: bad-skill\ndescription: {}\n---\n\nbody",
            "x".repeat(1025)
        ),
    )
    .unwrap();
    let err = load_skill(&skill_dir.join("SKILL.md"), SkillScope::Vault).unwrap_err();
    assert!(err.to_string().contains("description exceeds 1024"));
}

#[test]
fn read_skill_resource_allows_declared_resource_dirs_only() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let skill_root = vault.join(".iris/skills/my-skill");
    for dir in ["references", "resources", "scripts"] {
        std::fs::create_dir_all(skill_root.join(dir)).unwrap();
    }
    std::fs::write(skill_root.join("references/guide.md"), "guide body").unwrap();
    std::fs::write(skill_root.join("resources/data.md"), "resource body").unwrap();
    std::fs::write(skill_root.join("scripts/tool.sh"), "script body").unwrap();
    std::fs::write(skill_root.join("SKILL.md"), "# Skill").unwrap();
    let read = |path| read_skill_resource(&vault, "my-skill", SkillScope::Vault, path);

    assert_eq!(read("references/guide.md").unwrap(), "guide body");
    assert_eq!(read("resources/data.md").unwrap(), "resource body");
    assert!(read("../SKILL.md").is_err());
    assert!(read("scripts/tool.sh").is_err());
    assert!(read("notes/secret.md").is_err());
}
