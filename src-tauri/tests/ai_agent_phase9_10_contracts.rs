use iris_lib::ai_runtime::sandbox_profile::{
    os_sandbox_profile, sandbox_profile_for_tool, SandboxLevel, SandboxSupport,
};

#[test]
fn high_risk_tools_have_honest_sandbox_profiles() {
    let git = sandbox_profile_for_tool("git_write_commit");
    assert_eq!(git.level, SandboxLevel::L1Subprocess);
    assert!(git.constraints.contains(&"git_hooks_disabled".to_string()));
    assert!(git
        .constraints
        .contains(&"git_filters_disabled".to_string()));

    let os = os_sandbox_profile();
    assert_eq!(os.level, SandboxLevel::L2OsBoundary);
    assert_eq!(os.support, SandboxSupport::Unsupported);
}

#[test]
fn subprocess_sources_apply_l1_constraints_and_run_audit_identity() {
    let boundary = include_str!("../src/ai_runtime/tool_dispatch/boundary.rs");
    let skills = include_str!("../src/ai_runtime/skills_impl.rs");
    let audit = include_str!("../src/ai_runtime/tool_audit.rs");

    assert!(boundary.contains("core.hooksPath=/dev/null"));
    assert!(boundary.contains("filter.lfs.smudge="));
    assert!(boundary.contains("env_clear()"));
    assert!(!boundary.contains("process_run_readonly_tool"));
    assert!(skills.contains("write_confirmed_skill_content"));
    assert!(!skills.contains("SAFE_GIT_CLONE_ARGS"));
    assert!(!skills.contains("run_git_clone_with_timeout"));
    assert!(!skills.contains("Command::new(\"git\")"));
    assert!(audit.contains("pub run_id: String"));
    assert!(audit.contains("pub run_step: i64"));
    assert!(!boundary.contains("seccomp"));
    assert!(!boundary.contains("chroot"));
}
