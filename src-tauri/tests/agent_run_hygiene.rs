use std::fs;
use std::path::Path;

fn read_source(manifest_dir: &Path, relative_path: &str) -> String {
    fs::read_to_string(manifest_dir.join(relative_path)).unwrap()
}

#[test]
fn unified_run_is_the_only_runtime_execution_policy_surface() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    assert!(
        !manifest_dir.join("src/ai_runtime/agent_task.rs").exists(),
        "legacy AgentTask runtime module must be physically removed"
    );
    assert!(
        !manifest_dir
            .join("src/ai_runtime/agent_task_policy.rs")
            .exists(),
        "legacy AgentTaskPolicy module must be physically removed"
    );

    let runtime_module = read_source(manifest_dir, "src/ai_runtime/mod.rs");
    assert!(
        !runtime_module.contains("agent_task"),
        "the runtime module must not expose an AgentTask compatibility surface"
    );

    let app_state = read_source(manifest_dir, "src/app.rs");
    assert!(
        !app_state.contains("AgentTaskPolicy"),
        "pending confirmation state must not keep a second execution policy"
    );

    let shared_types = read_source(manifest_dir, "src/ai_types/mod.rs");
    assert!(
        !shared_types.contains("assistant_execute"),
        "shared context references must name the Run admission command"
    );
}
