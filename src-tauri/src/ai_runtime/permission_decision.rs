//! Unified permission decision engine for agent tool execution.

use serde::{Deserialize, Serialize};

use crate::ai_runtime::agent_permissions::{
    find_permission_grant, preflight_tool_permission, record_permission_audit,
    PermissionAuditInput, PermissionDecision, PermissionEffectSummary, PermissionPreflight,
    PermissionScopeKind,
};
use crate::ai_runtime::tool_catalog::ToolCatalogEntry;
use crate::ai_runtime::tool_policy::{self, ToolPolicyContext, ToolPolicyVerdict};
use crate::error::AppResult;
use crate::storage::db::Database;

/// Input for one permission decision.
pub struct PermissionDecisionRequest<'a> {
    /// Stable request id used for request/session scoped grants and audit.
    pub request_id: &'a str,
    /// Catalog entry for the tool being evaluated.
    pub entry: &'a ToolCatalogEntry,
    /// Tool arguments. Used only for safe preflight summaries.
    pub args: &'a serde_json::Value,
    /// Runtime tool policy context.
    pub policy_ctx: &'a ToolPolicyContext,
    /// Optional skill id when the tool is requested by a skill.
    pub skill_id: Option<&'a str>,
}

/// Execution decision after combining ToolPolicy, permission profile, and grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionExecutionDecision {
    AutoAllowed,
    RequiresConfirmation,
    Denied,
}

/// Result of the permission decision engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDecisionOutcome {
    pub tool_name: String,
    pub decision: PermissionExecutionDecision,
    pub preflight: PermissionPreflight,
    pub denied_reason: Option<String>,
    pub granted_by: Option<PermissionDecision>,
}

impl PermissionDecisionOutcome {
    /// Whether the tool can be executed immediately without another prompt.
    pub fn can_execute_now(&self) -> bool {
        self.decision == PermissionExecutionDecision::AutoAllowed
    }
}

/// Decide whether a tool call can execute, needs confirmation, or must be denied.
pub fn decide_tool_permission(
    db: &Database,
    request: PermissionDecisionRequest<'_>,
) -> AppResult<PermissionDecisionOutcome> {
    let preflight = preflight_tool_permission(request.entry, request.args, request.skill_id);

    if preflight.blocked {
        return Ok(PermissionDecisionOutcome {
            tool_name: request.entry.name.to_string(),
            decision: PermissionExecutionDecision::Denied,
            preflight,
            denied_reason: Some("unsupported by Iris Markdown workspace scope".to_string()),
            granted_by: None,
        });
    }

    match tool_policy::evaluate_tool(request.entry.name, request.policy_ctx) {
        ToolPolicyVerdict::Denied(reason) => {
            return Ok(PermissionDecisionOutcome {
                tool_name: request.entry.name.to_string(),
                decision: PermissionExecutionDecision::Denied,
                preflight,
                denied_reason: Some(format!("tool policy denied: {reason:?}")),
                granted_by: None,
            });
        }
        ToolPolicyVerdict::AutoAllowed => {}
        ToolPolicyVerdict::RequiresConfirmation => {
            let granted_by =
                granted_decision(db, request.request_id, request.skill_id, &preflight.effects)?;
            let decision = if granted_by.is_some() {
                PermissionExecutionDecision::AutoAllowed
            } else {
                PermissionExecutionDecision::RequiresConfirmation
            };
            return Ok(PermissionDecisionOutcome {
                tool_name: request.entry.name.to_string(),
                decision,
                preflight,
                denied_reason: None,
                granted_by,
            });
        }
    }

    let granted_by =
        granted_decision(db, request.request_id, request.skill_id, &preflight.effects)?;
    if granted_by.is_some() {
        return Ok(PermissionDecisionOutcome {
            tool_name: request.entry.name.to_string(),
            decision: PermissionExecutionDecision::AutoAllowed,
            preflight,
            denied_reason: None,
            granted_by,
        });
    }

    let decision = if preflight.decision == PermissionDecision::Allow {
        PermissionExecutionDecision::AutoAllowed
    } else {
        PermissionExecutionDecision::RequiresConfirmation
    };

    Ok(PermissionDecisionOutcome {
        tool_name: request.entry.name.to_string(),
        decision,
        preflight,
        denied_reason: None,
        granted_by: None,
    })
}

/// Record metadata-only audit rows for all permission effects in an outcome.
pub fn record_permission_decision_audit(
    db: &Database,
    request_id: &str,
    skill_id: Option<&str>,
    outcome: &PermissionDecisionOutcome,
    result_status: &str,
) -> AppResult<()> {
    for effect in &outcome.preflight.effects {
        record_permission_audit(
            db,
            &PermissionAuditInput {
                request_id,
                skill_id,
                tool_name: outcome.tool_name.as_str(),
                permission_name: effect.permission_name.as_str(),
                decision: permission_audit_decision(outcome),
                scope_summary: effect.scope_summary.as_str(),
                risk_level: effect.risk_level,
                result_status,
            },
        )?;
    }
    Ok(())
}

fn granted_decision(
    db: &Database,
    request_id: &str,
    skill_id: Option<&str>,
    effects: &[PermissionEffectSummary],
) -> AppResult<Option<PermissionDecision>> {
    if effects.is_empty() {
        return Ok(None);
    }

    let mut granted = None;
    for effect in effects {
        let Some(grant) = find_matching_grant(db, request_id, skill_id, effect)? else {
            return Ok(None);
        };
        match grant.decision {
            PermissionDecision::Allow | PermissionDecision::AllowForSession => {
                granted = Some(grant.decision);
            }
            _ => return Ok(None),
        }
    }
    Ok(granted)
}

fn find_matching_grant(
    db: &Database,
    request_id: &str,
    skill_id: Option<&str>,
    effect: &PermissionEffectSummary,
) -> AppResult<Option<crate::ai_runtime::agent_permissions::PermissionGrantRecord>> {
    let mut candidates = vec![(
        effect.scope_kind,
        scope_value_for_effect(effect.scope_kind, request_id, skill_id),
    )];
    if effect.scope_kind != PermissionScopeKind::Session {
        candidates.push((PermissionScopeKind::Session, Some(request_id)));
    }

    for (scope_kind, scope_value) in candidates {
        if let Some(grant) = find_permission_grant(
            db,
            effect.permission_name.as_str(),
            scope_kind,
            scope_value,
            skill_id,
        )? {
            return Ok(Some(grant));
        }
    }
    Ok(None)
}

fn scope_value_for_effect<'a>(
    scope_kind: PermissionScopeKind,
    request_id: &'a str,
    skill_id: Option<&'a str>,
) -> Option<&'a str> {
    match scope_kind {
        PermissionScopeKind::Request | PermissionScopeKind::Session => Some(request_id),
        PermissionScopeKind::Skill => skill_id,
        PermissionScopeKind::Global | PermissionScopeKind::Vault | PermissionScopeKind::Folder => {
            None
        }
    }
}

fn permission_audit_decision(outcome: &PermissionDecisionOutcome) -> PermissionDecision {
    match outcome.decision {
        PermissionExecutionDecision::AutoAllowed => {
            outcome.granted_by.unwrap_or(PermissionDecision::Allow)
        }
        PermissionExecutionDecision::RequiresConfirmation => PermissionDecision::AllowOnce,
        PermissionExecutionDecision::Denied => PermissionDecision::DenyOnce,
    }
}
