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
