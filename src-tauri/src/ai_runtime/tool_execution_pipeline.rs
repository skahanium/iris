//! Unified gate for tool permission decisions and audit side effects.

use serde::{Deserialize, Serialize};

use crate::ai_runtime::permission_decision::{
    decide_tool_permission, record_permission_decision_audit, PermissionDecisionOutcome,
    PermissionDecisionRequest, PermissionExecutionDecision,
};
use crate::ai_runtime::tool_audit::{record_audit, ToolAuditInput};
use crate::ai_runtime::tool_catalog::ToolCatalogEntry;
use crate::ai_runtime::tool_policy::ToolPolicyContext;
use crate::ai_runtime::ToolCallResult;
use crate::error::AppResult;
use crate::storage::db::Database;

/// Input for evaluating whether a tool can enter dispatch.
#[derive(Clone, Copy)]
pub struct ToolExecutionGate<'a> {
    pub request_id: &'a str,
    pub harness_round: u32,
    pub entry: &'a ToolCatalogEntry,
    pub args: &'a serde_json::Value,
    pub policy_ctx: &'a ToolPolicyContext,
    pub skill_id: Option<&'a str>,
    pub scene: Option<&'a str>,
    pub subagent_depth: u32,
}

/// Gate result returned before dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionGateOutcome {
    pub decision: PermissionDecisionOutcome,
    pub tool_result: Option<ToolCallResult>,
}

/// Evaluate permission and policy gates, writing audit rows for blocked tools.
pub fn evaluate_tool_execution(
    db: &Database,
    gate: ToolExecutionGate<'_>,
) -> AppResult<ToolExecutionGateOutcome> {
    let decision = decide_tool_permission(
        db,
        PermissionDecisionRequest {
            request_id: gate.request_id,
            entry: gate.entry,
            args: gate.args,
            policy_ctx: gate.policy_ctx,
            skill_id: gate.skill_id,
        },
    )?;

    if decision.decision == PermissionExecutionDecision::Denied {
        record_permission_decision_audit(db, gate.request_id, gate.skill_id, &decision, "denied")?;
        let result = denied_tool_result(gate.entry.name, decision.denied_reason.as_deref());
        record_audit(
            db,
            &ToolAuditInput {
                request_id: gate.request_id,
                harness_round: gate.harness_round,
                tool_name: gate.entry.name,
                arguments: gate.args,
                result: &result.output,
                error: result.error.as_deref(),
                success: false,
                duration_ms: result.duration_ms,
                scene: gate.scene,
                subagent_depth: gate.subagent_depth,
            },
        )?;
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
pub fn audit_dispatched_tool(
    db: &Database,
    gate: &ToolExecutionGate<'_>,
    decision: &PermissionDecisionOutcome,
    result: &ToolCallResult,
) -> AppResult<()> {
    let status = if result.success { "executed" } else { "failed" };
    record_permission_decision_audit(db, gate.request_id, gate.skill_id, decision, status)?;
    record_audit(
        db,
        &ToolAuditInput {
            request_id: gate.request_id,
            harness_round: gate.harness_round,
            tool_name: gate.entry.name,
            arguments: gate.args,
            result: &result.output,
            error: result.error.as_deref(),
            success: result.success,
            duration_ms: result.duration_ms,
            scene: gate.scene,
            subagent_depth: gate.subagent_depth,
        },
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
