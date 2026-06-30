use crate::ai_runtime::tool_catalog::{catalog_find, ToolImplementationStatus};
use crate::ai_types::{BlockedCapabilitySummary, SkillCapabilitySupportStatus};

use super::SkillEntry;

/// Normalize external tool/capability names used by Iris, Claude, and Hermes skills.
pub fn normalize_external_capability(raw: &str) -> String {
    raw.trim()
        .trim_matches('`')
        .replace([' ', '-'], "_")
        .to_lowercase()
}

/// Map a requested tool/capability to an Iris support status.
pub fn support_status_for_capability(raw: &str) -> SkillCapabilitySupportStatus {
    let normalized = normalize_external_capability(raw);
    if let Some(entry) = catalog_find(&normalized) {
        return if entry.implementation == ToolImplementationStatus::Planned {
            SkillCapabilitySupportStatus::Planned
        } else if entry.requires_confirmation {
            SkillCapabilitySupportStatus::SupportedWithConfirmation
        } else {
            SkillCapabilitySupportStatus::Supported
        };
    }

    match normalized.as_str() {
        "read" | "grep" | "glob" | "ls" | "notebookread" => SkillCapabilitySupportStatus::Supported,
        "write" | "edit" | "notebookedit" | "multiedit" => {
            SkillCapabilitySupportStatus::SupportedWithConfirmation
        }
        "webfetch" | "web_search" | "websearch" => {
            SkillCapabilitySupportStatus::SupportedWithConfirmation
        }
        "bash" | "shell" | "computer" | "computer_control" => {
            SkillCapabilitySupportStatus::BlockedByPolicy
        }
        "execute_script_sandboxed" | "install_dependency" | "dependency_install" | "mcp_bridge" => {
            SkillCapabilitySupportStatus::BlockedByPolicy
        }
        _ => SkillCapabilitySupportStatus::UnsupportedByProductScope,
    }
}

/// Human-readable fallback guidance for a support status.
pub fn fallback_guidance(raw: &str, status: SkillCapabilitySupportStatus) -> String {
    match status {
        SkillCapabilitySupportStatus::Supported => {
            "This capability maps to an Iris tool and can be considered by ToolPolicy.".into()
        }
        SkillCapabilitySupportStatus::SupportedWithConfirmation => {
            "This capability maps to an Iris tool, but execution requires user confirmation.".into()
        }
        SkillCapabilitySupportStatus::Planned => {
            "This capability is a future extension point and is not executed in Phase4.".into()
        }
        SkillCapabilitySupportStatus::UnsupportedByProductScope => format!(
            "{raw} has no supported Iris workspace capability; use Markdown-safe Iris alternatives."
        ),
        SkillCapabilitySupportStatus::BlockedByPolicy => {
            "This high-risk capability is blocked by Iris policy in Phase4.".into()
        }
        SkillCapabilitySupportStatus::MissingUserGrant => {
            "This capability needs a user grant before Iris can expose it.".into()
        }
    }
}

fn risk_level(status: SkillCapabilitySupportStatus) -> &'static str {
    match status {
        SkillCapabilitySupportStatus::Supported => "low",
        SkillCapabilitySupportStatus::SupportedWithConfirmation
        | SkillCapabilitySupportStatus::MissingUserGrant => "medium",
        SkillCapabilitySupportStatus::Planned
        | SkillCapabilitySupportStatus::UnsupportedByProductScope
        | SkillCapabilitySupportStatus::BlockedByPolicy => "high",
    }
}

/// Build blocked/degraded capability summaries for a skill.
pub fn blocked_capabilities_for_skill(entry: &SkillEntry) -> Vec<BlockedCapabilitySummary> {
    let mut blocked = Vec::new();
    for tool in &entry.allowed_tools {
        let status = support_status_for_capability(tool);
        if matches!(
            status,
            SkillCapabilitySupportStatus::Planned
                | SkillCapabilitySupportStatus::UnsupportedByProductScope
                | SkillCapabilitySupportStatus::BlockedByPolicy
                | SkillCapabilitySupportStatus::MissingUserGrant
        ) {
            blocked.push(BlockedCapabilitySummary {
                skill_name: entry.name.clone(),
                capability: tool.clone(),
                status,
                risk_level: risk_level(status).into(),
                permission: catalog_find(&normalize_external_capability(tool))
                    .map(|tool| tool.access_level),
                fallback_guidance: fallback_guidance(tool, status),
            });
        }
    }
    blocked
}
