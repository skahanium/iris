use iris_lib::ai_runtime::sandbox_profile::{
    os_sandbox_profile, sandbox_profile_for_tool, SandboxLevel, SandboxSupport,
};

#[test]
fn high_risk_tools_have_honest_sandbox_profiles() {
    let process = sandbox_profile_for_tool("process_run_readonly");
    assert_eq!(process.level, SandboxLevel::L1Subprocess);
    assert_eq!(process.support, SandboxSupport::Supported);
    assert!(process.constraints.contains(&"cwd_fixed".to_string()));
    assert!(process.constraints.contains(&"env_cleared".to_string()));
    assert!(process
        .constraints
        .contains(&"timeout_enforced".to_string()));
    assert!(process
        .constraints
        .contains(&"stdout_stderr_limited".to_string()));
    assert!(process
        .constraints
        .contains(&"argument_allowlist".to_string()));

    let git = sandbox_profile_for_tool("git_write_commit");
    assert_eq!(git.level, SandboxLevel::L1Subprocess);
    assert!(git.constraints.contains(&"git_hooks_disabled".to_string()));
    assert!(git
        .constraints
        .contains(&"git_filters_disabled".to_string()));

    let skill_install = sandbox_profile_for_tool("skills_install");
    assert_eq!(skill_install.level, SandboxLevel::L1Subprocess);
    assert!(skill_install
        .limitations
        .iter()
        .any(|item| item.contains("not an OS sandbox")));

    let os = os_sandbox_profile();
    assert_eq!(os.level, SandboxLevel::L2OsBoundary);
    assert_eq!(os.support, SandboxSupport::Unsupported);
}

#[test]
fn subprocess_sources_apply_l1_constraints_without_claiming_l2() {
    let boundary = include_str!("../src/ai_runtime/tool_dispatch/boundary.rs");
    let skills = include_str!("../src/ai_runtime/skills_impl.rs");
    let confirm = include_str!("../src/ai_harness/harness/run.rs");
    let audit = include_str!("../src/ai_runtime/tool_audit.rs");
    let cert = include_str!("../src/network/cert_pinning.rs");

    assert!(boundary.contains("core.hooksPath=/dev/null"));
    assert!(boundary.contains("filter.lfs.smudge="));
    assert!(boundary.contains("env_clear()"));
    assert!(boundary.contains("Duration::from_secs(5)"));
    assert!(skills.contains("SAFE_GIT_CLONE_ARGS"));
    assert!(skills.contains("run_git_clone_with_timeout"));
    assert!(skills.contains("env_clear()"));
    assert!(skills.contains("Duration::from_secs"));
    assert!(confirm.contains("\"sandboxProfile\""));
    assert!(audit.contains("sandbox_profile_id"));
    assert!(audit.contains("sandbox_profile="));
    assert!(cert.contains("无证书固定"));
    assert!(!cert.contains("已实现证书固定"));
    assert!(!boundary.contains("seccomp"));
    assert!(!boundary.contains("chroot"));
}

#[test]
fn frontend_task_surfaces_expose_deliberation_and_verification_state() {
    let ipc_types = include_str!("../../src/types/ipc.ts");
    let panel = include_str!("../../src/components/ai/AgentTaskStatusPanel.tsx");
    let surfaces = include_str!("../../src/components/ai/AssistantTaskSurfaces.tsx");

    assert!(ipc_types.contains("deliberation_state?: DeliberationState | null"));
    assert!(ipc_types.contains("verification_summary?: VerificationSummary | null"));
    assert!(panel.contains("task.deliberation_state"));
    assert!(panel.contains("task.verification_summary"));
    assert!(panel.contains("evidence_gaps"));
    assert!(panel.contains("data-testid=\"agent-task-deliberation\""));
    assert!(surfaces.contains("WritingStatePanel"));
    assert!(surfaces.contains("ResearchStatePanel"));
    assert!(!panel.contains("checkpoint_json"));
    assert!(!panel.contains("noteContent"));
    assert!(!panel.contains("apiKey"));
}
