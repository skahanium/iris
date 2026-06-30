use serde::{Deserialize, Serialize};

/// Sandbox boundary level. L2 is defined as an interface target and is not
/// currently implemented by Iris.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxLevel {
    L0AppBoundary,
    L1Subprocess,
    L2OsBoundary,
}

/// Whether a profile is actually enforced by the current runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxSupport {
    Supported,
    Unsupported,
}

/// User-facing sandbox capability summary for a tool or capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxProfile {
    pub id: String,
    pub level: SandboxLevel,
    pub support: SandboxSupport,
    pub summary: String,
    pub constraints: Vec<String>,
    pub limitations: Vec<String>,
}

impl SandboxProfile {
    fn l0(tool_name: &str, summary: &str) -> Self {
        Self {
            id: format!("{tool_name}:l0_app_boundary"),
            level: SandboxLevel::L0AppBoundary,
            support: SandboxSupport::Supported,
            summary: summary.to_string(),
            constraints: vec![
                "tool_policy_gate".to_string(),
                "permission_decision_gate".to_string(),
                "sanitized_audit".to_string(),
            ],
            limitations: vec!["application-level controls only; not an OS sandbox".to_string()],
        }
    }

    fn l1(tool_name: &str, summary: &str, extra_constraints: &[&str]) -> Self {
        let mut constraints = vec![
            "cwd_fixed".to_string(),
            "env_cleared".to_string(),
            "timeout_enforced".to_string(),
            "stdout_stderr_limited".to_string(),
            "argument_allowlist".to_string(),
            "sanitized_audit".to_string(),
        ];
        constraints.extend(extra_constraints.iter().map(|item| (*item).to_string()));
        constraints.sort();
        constraints.dedup();
        Self {
            id: format!("{tool_name}:l1_subprocess"),
            level: SandboxLevel::L1Subprocess,
            support: SandboxSupport::Supported,
            summary: summary.to_string(),
            constraints,
            limitations: vec![
                "not an OS sandbox: no seccomp, namespace, chroot, or container isolation"
                    .to_string(),
            ],
        }
    }
}

/// Current OS-level sandbox target. Kept explicit so UI and audits can avoid
/// implying that L2 isolation exists today.
pub fn os_sandbox_profile() -> SandboxProfile {
    SandboxProfile {
        id: "l2_os_boundary".to_string(),
        level: SandboxLevel::L2OsBoundary,
        support: SandboxSupport::Unsupported,
        summary: "OS-level sandbox interface is defined but not implemented".to_string(),
        constraints: Vec::new(),
        limitations: vec![
            "unsupported: no seccomp, namespace, chroot, container, or platform sandbox profile"
                .to_string(),
        ],
    }
}

/// Resolve the sandbox profile that applies to a tool.
pub fn sandbox_profile_for_tool(tool_name: &str) -> SandboxProfile {
    match tool_name {
        "git_read_status" | "git_read_diff" | "git_read_log" | "git_write_commit" => {
            SandboxProfile::l1(
                tool_name,
                "Git subprocess scoped to the current vault with hooks and filters disabled",
                &["git_hooks_disabled", "git_filters_disabled"],
            )
        }
        _ => SandboxProfile::l0(
            tool_name,
            "Application-level policy and permission boundary",
        ),
    }
}
