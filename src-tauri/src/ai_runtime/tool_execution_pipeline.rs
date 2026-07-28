//! Unified gate for tool permission decisions and audit side effects.

use serde::{Deserialize, Serialize};

use crate::ai_runtime::permission_decision::{
    decide_tool_permission, record_permission_decision_audit, PermissionDecisionOutcome,
    PermissionDecisionRequest, PermissionExecutionDecision,
};
use crate::ai_runtime::run_contract::CapabilityId;
use crate::ai_runtime::tool_audit::{record_audit, ToolAuditInput};
use crate::ai_runtime::tool_catalog::ToolCatalogEntry;
use crate::ai_runtime::ToolCallResult;
use crate::error::AppResult;
use crate::storage::db::Database;

/// Input for evaluating whether a tool can enter dispatch.
#[derive(Clone, Copy)]
pub(crate) struct ToolExecutionGate<'a> {
    pub run_id: &'a str,
    /// Real session identity for Session-scoped grants; never substitute run_id.
    pub session_id: Option<i64>,
    pub run_step: u32,
    pub entry: &'a ToolCatalogEntry,
    pub args: &'a serde_json::Value,
    /// Immutable capabilities authorized by the Run policy decision.
    pub authorized_capabilities: &'a [CapabilityId],
    pub skill_id: Option<&'a str>,
    pub subagent_depth: u32,
}

/// Gate result returned before dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionGateOutcome {
    pub decision: PermissionDecisionOutcome,
    pub tool_result: Option<ToolCallResult>,
}

/// Evaluate permission, capability, and argument-shape gates before dispatch.
///
/// The caller writes the single audit outcome after the corresponding lifecycle
/// event is persisted, so rejected calls cannot produce duplicate audit rows.
pub(crate) fn evaluate_tool_execution(
    db: &Database,
    gate: ToolExecutionGate<'_>,
) -> AppResult<ToolExecutionGateOutcome> {
    let decision = decide_tool_permission(
        db,
        PermissionDecisionRequest {
            run_id: gate.run_id,
            session_id: gate.session_id,
            entry: gate.entry,
            args: gate.args,
            authorized_capabilities: gate.authorized_capabilities,
            skill_id: gate.skill_id,
        },
    )?;

    if decision.decision == PermissionExecutionDecision::Denied {
        let result = denied_tool_result(gate.entry.name, decision.denied_reason.as_deref());
        return Ok(ToolExecutionGateOutcome {
            decision,
            tool_result: Some(result),
        });
    }

    if let crate::ai_runtime::guardrails::GuardResult::Block { .. } =
        crate::ai_runtime::guardrails::verify_tool_args(
            gate.entry.name,
            gate.args,
            &gate.entry.input_schema,
        )
    {
        let result = invalid_arguments_tool_result(gate.entry.name);
        return Ok(ToolExecutionGateOutcome {
            decision,
            tool_result: Some(result),
        });
    }

    Ok(ToolExecutionGateOutcome {
        decision,
        tool_result: None,
    })
}

/// Record successful or failed dispatch in the unified permission and tool audit streams.
pub(crate) fn audit_dispatched_tool(
    db: &Database,
    gate: &ToolExecutionGate<'_>,
    decision: &PermissionDecisionOutcome,
    result: &ToolCallResult,
) -> AppResult<()> {
    let status = if result.success { "executed" } else { "failed" };
    record_permission_decision_audit(db, gate.run_id, gate.skill_id, decision, status)?;
    record_audit(
        db,
        &ToolAuditInput {
            run_id: gate.run_id,
            run_step: gate.run_step,
            tool_name: gate.entry.name,
            arguments: gate.args,
            result: &result.output,
            error: result.error.as_deref(),
            success: result.success,
            duration_ms: result.duration_ms,
            subagent_depth: gate.subagent_depth,
        },
    )
}

/// Record a confirmed-only tool request that was frozen before dispatch.
///
/// The plan itself is the authorization artifact; no effect has run yet, so
/// this deliberately does not pretend that the tool completed.
pub(crate) fn audit_tool_confirmation_requested(
    db: &Database,
    gate: &ToolExecutionGate<'_>,
    decision: &PermissionDecisionOutcome,
) -> AppResult<()> {
    record_permission_decision_audit(
        db,
        gate.run_id,
        gate.skill_id,
        decision,
        "pending_confirmation",
    )
}

fn denied_tool_result(tool_name: &str, reason: Option<&str>) -> ToolCallResult {
    let message = reason.unwrap_or("tool execution denied");
    ToolCallResult {
        tool_name: tool_name.to_string(),
        success: false,
        output: serde_json::json!({ "error": message }),
        duration_ms: 0,
        tokens_used: None,
        error: Some(message.to_string()),
    }
}

fn invalid_arguments_tool_result(tool_name: &str) -> ToolCallResult {
    ToolCallResult {
        tool_name: tool_name.to_string(),
        success: false,
        output: serde_json::json!({ "error": "tool_arguments_invalid" }),
        duration_ms: 0,
        tokens_used: None,
        error: Some("tool_arguments_invalid".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::agent_run_repository::{AcceptRunInput, AgentRunRepository};
    use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
    use crate::ai_runtime::run_contract::{
        ContextMode, Effect, Effort, ExecutionEnvelope, Freshness, RiskClass, SecurityDomain,
        WebDecisionReason,
    };
    use crate::ai_runtime::tool_catalog::catalog_find;

    #[test]
    fn malformed_arguments_never_reach_dispatch() {
        let db = Database::open_in_memory().expect("database");
        let session = NormalSessionRepository::create(&db).expect("session");
        AgentRunRepository::accept(
            &db,
            AcceptRunInput {
                session_id: session.session_id,
                session_key: session.session_key,
                client_request_id: "argument-validation-request".into(),
                run_id: "argument-validation-run".into(),
                turn_id: "argument-validation-turn".into(),
                message: "search".into(),
                content_parts: None,
                explicit_references: vec![],
                context_scope: Default::default(),
                display_mentions: vec![],
                explicit_action: None,
                envelope: ExecutionEnvelope {
                    effect: Effect::Answer,
                    context: ContextMode::None,
                    freshness: Freshness::WebPreferred,
                    web_reason: WebDecisionReason::DefaultOnline,
                    verification_requirement:
                        crate::ai_runtime::run_contract::VerificationRequirement::None,
                    effort: Effort::ToolLoop,
                    security_domain: SecurityDomain::Normal,
                    risk: RiskClass::ReadOnly,
                    modalities: vec![],
                    material_needs: vec![],
                    required_capabilities: vec![CapabilityId::new("web.search")],
                    explicit_constraints: vec![],
                },
            },
        )
        .expect("accepted run");
        let entry = catalog_find("web_search").expect("web search catalog entry");
        let args = serde_json::json!({"query": 42});

        let outcome = evaluate_tool_execution(
            &db,
            ToolExecutionGate {
                run_id: "argument-validation-run",
                session_id: None,
                run_step: 1,
                entry,
                args: &args,
                authorized_capabilities: &[CapabilityId::new("web.search")],
                skill_id: None,
                subagent_depth: 0,
            },
        )
        .expect("gate outcome");

        let result = outcome
            .tool_result
            .expect("invalid arguments must stop dispatch");
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("tool_arguments_invalid"));
    }
}
