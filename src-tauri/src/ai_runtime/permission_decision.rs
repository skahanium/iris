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
    pub run_id: &'a str,
    /// Owning normal-domain session; required to consume a Session-scoped grant.
    pub session_id: Option<i64>,
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
            let granted_by = granted_decision(
                db,
                request.run_id,
                request.session_id,
                request.skill_id,
                &preflight.effects,
            )?;
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

    let granted_by = granted_decision(
        db,
        request.run_id,
        request.session_id,
        request.skill_id,
        &preflight.effects,
    )?;
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
    run_id: &str,
    skill_id: Option<&str>,
    outcome: &PermissionDecisionOutcome,
    result_status: &str,
) -> AppResult<()> {
    for effect in &outcome.preflight.effects {
        record_permission_audit(
            db,
            &PermissionAuditInput {
                run_id,
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
    run_id: &str,
    session_id: Option<i64>,
    skill_id: Option<&str>,
    effects: &[PermissionEffectSummary],
) -> AppResult<Option<PermissionDecision>> {
    if effects.is_empty() {
        return Ok(None);
    }

    let mut granted = None;
    for effect in effects {
        let Some(grant) = find_matching_grant(db, run_id, session_id, skill_id, effect)? else {
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
    run_id: &str,
    session_id: Option<i64>,
    skill_id: Option<&str>,
    effect: &PermissionEffectSummary,
) -> AppResult<Option<crate::ai_runtime::agent_permissions::PermissionGrantRecord>> {
    let mut candidates = vec![(
        effect.scope_kind,
        scope_value_for_effect(effect.scope_kind, run_id, session_id, skill_id),
    )];
    if effect.scope_kind != PermissionScopeKind::Session {
        if let Some(session_id) = session_id {
            candidates.push((PermissionScopeKind::Session, Some(session_id.to_string())));
        }
    }

    for (scope_kind, scope_value) in candidates {
        if scope_kind == PermissionScopeKind::Session && scope_value.is_none() {
            continue;
        }
        if let Some(grant) = find_permission_grant(
            db,
            effect.permission_name.as_str(),
            scope_kind,
            scope_value.as_deref(),
            skill_id,
        )? {
            return Ok(Some(grant));
        }
    }
    Ok(None)
}

fn scope_value_for_effect(
    scope_kind: PermissionScopeKind,
    run_id: &str,
    session_id: Option<i64>,
    skill_id: Option<&str>,
) -> Option<String> {
    match scope_kind {
        PermissionScopeKind::Request => Some(run_id.to_string()),
        PermissionScopeKind::Session => session_id.map(|id| id.to_string()),
        PermissionScopeKind::Skill => skill_id.map(ToString::to_string),
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

#[cfg(test)]
mod tests {
    use super::{find_matching_grant, PermissionEffectSummary};
    use crate::ai_runtime::agent_permissions::{
        upsert_permission_grant, PermissionDecision, PermissionGrantInput, PermissionRiskLevel,
        PermissionScopeKind,
    };
    use crate::storage::db::Database;

    fn session_effect() -> PermissionEffectSummary {
        PermissionEffectSummary {
            permission_name: "vault.read".to_string(),
            scope_kind: PermissionScopeKind::Session,
            scope_summary: "session-only".to_string(),
            risk_level: PermissionRiskLevel::Low,
            reversible_by: "none".to_string(),
            blocked_reason: None,
        }
    }

    #[test]
    fn session_grant_is_keyed_by_session_id_not_run_id() {
        let db = Database::open_in_memory().expect("database");
        upsert_permission_grant(
            &db,
            &PermissionGrantInput {
                permission_name: "vault.read",
                decision: PermissionDecision::AllowForSession,
                scope_kind: PermissionScopeKind::Session,
                scope_value: Some("42"),
                risk_level: PermissionRiskLevel::Low,
                skill_id: None,
                expires_at: None,
            },
        )
        .expect("grant");

        assert!(
            find_matching_grant(&db, "request-a", Some(42), None, &session_effect())
                .expect("same session lookup")
                .is_some()
        );
        assert!(
            find_matching_grant(&db, "request-b", Some(7), None, &session_effect())
                .expect("other session lookup")
                .is_none()
        );
    }
}
