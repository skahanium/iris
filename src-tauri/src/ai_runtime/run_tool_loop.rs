//! Bounded model-driven Web evidence for normal-domain Runs.
//!
//! This module owns the `web_search` tool path used by `NormalRunToolExecutor`: policy/audit
//! gates, bounded evidence registration, and deferred `CapabilityDegraded` emission when Online
//! search fails without usable evidence. Offline Runs never enable the tool.

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::ai_runtime::agent_evidence_repository::{
    AgentEvidenceRepository, MaterialRole, WebEvidenceInput,
};
use crate::ai_runtime::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
use crate::ai_runtime::agent_tool_loop::ToolLoopExecutor;
use crate::ai_runtime::run_context::RunContext;
use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, RunEventPayload, RunEventType, SafeRunErrorCode,
    WebEvidenceFailureReason,
};
use crate::ai_runtime::run_engine::RunEventSink;
use crate::ai_runtime::tool_catalog::catalog_find;
use crate::ai_runtime::tool_dispatch::{dispatch_tool_with_retry, ToolDispatchContext};
use crate::ai_runtime::tool_execution_pipeline::{
    audit_dispatched_tool, audit_tool_confirmation_requested, evaluate_tool_execution,
    ToolExecutionGate,
};
use crate::ai_runtime::tool_policy::ToolPolicyContext;
use crate::ai_runtime::ToolCallResult;
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

const WEB_TOOL_NAME: &str = "web_search";
const MAX_WEB_EVIDENCE_PER_RUN: usize = 8;
/// Required-run and diagnostic search limit. Keeping this shared prevents a one-row smoke probe
/// from passing while the actual evidence request exceeds a provider's output budget.
pub(crate) const INITIAL_WEB_SEARCH_RESULTS: usize = 5;
const MAX_WEB_EXCERPT_CHARS: usize = 2_000;
/// Model-requested follow-up searches retain their own bounded interaction budget.
const MODEL_WEB_EVIDENCE_DEADLINE: Duration = Duration::from_secs(10);
/// Internal control-flow signal: the Run was durably moved to confirmation,
/// so the model loop must stop without terminalizing it.
pub(crate) const CONFIRMATION_PENDING_ERROR: &str = "agent_run_confirmation_pending";
const CHANGE_CONFIRMATION_TTL_MS: i64 = 10 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WebFailure {
    code: SafeRunErrorCode,
    retryable: bool,
    reason: WebEvidenceFailureReason,
}

impl WebFailure {
    const fn new(code: SafeRunErrorCode, retryable: bool) -> Self {
        Self {
            code,
            retryable,
            reason: failure_reason_for_code(code),
        }
    }

    const fn with_reason(
        code: SafeRunErrorCode,
        retryable: bool,
        reason: WebEvidenceFailureReason,
    ) -> Self {
        Self {
            code,
            retryable,
            reason,
        }
    }
}

const fn failure_reason_for_code(code: SafeRunErrorCode) -> WebEvidenceFailureReason {
    match code {
        SafeRunErrorCode::WebProviderUnavailable => WebEvidenceFailureReason::ProviderUnavailable,
        SafeRunErrorCode::WebProviderTimeout => WebEvidenceFailureReason::ProviderTimeout,
        SafeRunErrorCode::WebProviderAuthFailed => WebEvidenceFailureReason::ProviderAuthentication,
        SafeRunErrorCode::WebProviderFailed => WebEvidenceFailureReason::ProviderTransport,
        SafeRunErrorCode::WebEvidenceInvalid => WebEvidenceFailureReason::Unknown,
        _ => WebEvidenceFailureReason::Unknown,
    }
}

#[derive(Debug, Default)]
struct RunWebBudget {
    started: Mutex<Option<Instant>>,
}

impl RunWebBudget {
    fn started(&self) -> AppResult<Instant> {
        let mut started = self
            .started
            .lock()
            .map_err(|_| AppError::msg("agent_run_web_budget_lock_failed"))?;
        Ok(*started.get_or_insert_with(Instant::now))
    }
}

/// Concrete normal-domain executor for the model tool loop.
///
/// It owns no policy decisions: every call re-enters the catalog, permission
/// gate and audit trail before it reaches the existing typed dispatcher.
pub(crate) struct NormalRunToolExecutor<'a> {
    state: &'a Arc<AppState>,
    app_handle: tauri::AppHandle,
    accepted: &'a AssistantRunAccepted,
    context: &'a RunContext,
    policy_ctx: ToolPolicyContext,
    sink: &'a dyn RunEventSink,
    retrieval_scope: crate::ai_runtime::retrieval_scope::RetrievalScope,
    cold_start_packets: Vec<crate::ai_runtime::ContextPacket>,
    runtime_documents: Vec<crate::ai_runtime::RuntimeDocumentSnapshot>,
    evidence_ids: Mutex<Vec<i64>>,
    web_failure: Mutex<Option<WebFailure>>,
    web_attempt_count: Mutex<u32>,
    web_budget: RunWebBudget,
    web_degradation_emitted: Mutex<bool>,
    required_web_provider_snapshot:
        Option<crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary>,
}

impl<'a> NormalRunToolExecutor<'a> {
    /// Create a Run-bound executor for the already-authorized normal domain.
    pub(crate) fn new(
        state: &'a Arc<AppState>,
        app_handle: tauri::AppHandle,
        accepted: &'a AssistantRunAccepted,
        context: &'a RunContext,
        policy_ctx: ToolPolicyContext,
        sink: &'a dyn RunEventSink,
        required_web_provider_snapshot: Option<
            crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary,
        >,
    ) -> Self {
        Self {
            state,
            app_handle,
            accepted,
            context,
            policy_ctx,
            sink,
            retrieval_scope: context.retrieval_scope.clone(),
            cold_start_packets: context.local_retrieval_packets.clone(),
            runtime_documents: Vec::new(),
            evidence_ids: Mutex::new(Vec::new()),
            web_failure: Mutex::new(None),
            web_attempt_count: Mutex::new(0),
            web_budget: RunWebBudget::default(),
            web_degradation_emitted: Mutex::new(false),
            required_web_provider_snapshot,
        }
    }

    async fn execute_web_search(
        &self,
        args: &serde_json::Value,
        state_version: u64,
    ) -> AppResult<ToolCallResult> {
        let query = args
            .get("query")
            .and_then(serde_json::Value::as_str)
            .filter(|query| !query.trim().is_empty())
            .ok_or_else(|| AppError::msg("tool_arguments_invalid"))?;
        let urls = args
            .get("urls")
            .and_then(serde_json::Value::as_array)
            .map(|urls| {
                urls.iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let remaining = MAX_WEB_EVIDENCE_PER_RUN.saturating_sub(self.evidence_ids().len());
        if remaining == 0 {
            self.set_web_failure(Some(WebFailure::new(
                SafeRunErrorCode::WebEvidenceInvalid,
                false,
            )))?;
            return Ok(failed_tool_call(
                WEB_TOOL_NAME,
                "web_evidence_budget_exhausted",
            ));
        }
        // Model web calls share MODEL_WEB_EVIDENCE_DEADLINE (10s). MCP search alone
        // commonly takes ~4s; scheduling deep page fetches (WEB_FETCH_TURN_BUDGET=8s)
        // after that exceeds the outer timeout and discards already-usable search
        // snippets. Prefer registering search snippets first.
        let broker_input = crate::ai_runtime::web_evidence_broker::WebEvidenceBrokerInput {
            query: query.to_owned(),
            urls,
            enabled: self.policy_ctx.web_search_enabled,
            max_search_results: remaining,
            max_fetches: 0,
            provider_snapshot: self.required_web_provider_snapshot.clone(),
        };
        let budget_started = self.web_budget.started()?;
        let call_started = Instant::now();
        let output = loop {
            let Some(attempt_count) = self.reserve_web_attempt()? else {
                let failure = WebFailure::new(SafeRunErrorCode::WebEvidenceInvalid, false);
                self.set_web_failure(Some(failure))?;
                return Ok(failed_web_tool_call(
                    failure,
                    self.web_attempt_count(),
                    call_started.elapsed(),
                    remaining_model_web_budget_ms(budget_started.elapsed()),
                ));
            };
            let remaining_time =
                MODEL_WEB_EVIDENCE_DEADLINE.saturating_sub(budget_started.elapsed());
            if remaining_time.is_zero() {
                let failure = WebFailure::new(SafeRunErrorCode::WebProviderTimeout, true);
                self.set_web_failure(Some(failure))?;
                return Ok(failed_web_tool_call(
                    failure,
                    attempt_count,
                    call_started.elapsed(),
                    remaining_model_web_budget_ms(budget_started.elapsed()),
                ));
            }
            let failure = match tokio::time::timeout(
                remaining_time,
                crate::ai_runtime::web_evidence_broker::collect_initial_run_web_evidence_with_usage(
                    &self.state.db,
                    broker_input.clone(),
                ),
            )
            .await
            {
                Ok(Ok(output)) if web_output_has_usable_evidence(&output) => break output,
                Ok(Ok(output)) => classify_web_evidence_output_failure(&output),
                Ok(Err(error)) => classify_web_failure(&error),
                Err(_) => WebFailure::new(SafeRunErrorCode::WebProviderTimeout, true),
            };
            if attempt_count < 2
                && failure.retryable
                && budget_started.elapsed() + Duration::from_millis(250)
                    < MODEL_WEB_EVIDENCE_DEADLINE
            {
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            self.set_web_failure(Some(failure))?;
            return Ok(failed_web_tool_call(
                failure,
                attempt_count,
                call_started.elapsed(),
                remaining_model_web_budget_ms(budget_started.elapsed()),
            ));
        };
        let packets = crate::ai_runtime::web_evidence_broker::web_evidence_items_to_packets(
            query,
            &output.items,
        );
        let evidence_ids = register_model_web_evidence(
            &self.state.db,
            self.accepted,
            self.context,
            self.sink,
            state_version,
            &output.items,
            remaining,
        )?;
        if evidence_ids.is_empty() {
            let failure = classify_web_evidence_output_failure(&output);
            self.set_web_failure(Some(failure))?;
            return Ok(failed_web_tool_call(
                failure,
                self.web_attempt_count(),
                call_started.elapsed(),
                remaining_model_web_budget_ms(budget_started.elapsed()),
            ));
        }
        self.set_web_failure(None)?;
        self.evidence_ids
            .lock()
            .map_err(|_| AppError::msg("agent_run_evidence_lock_failed"))?
            .extend(evidence_ids.iter().copied());
        Ok(ToolCallResult {
            tool_name: WEB_TOOL_NAME.to_string(),
            success: true,
            output: serde_json::json!({
                "results": packets,
                "evidenceIds": evidence_ids,
                "count": evidence_ids.len(),
                "resultBudget": { "format": "context_packets_only", "rawEvidenceOmitted": true },
                "remainingBudgetMs": remaining_model_web_budget_ms(budget_started.elapsed()),
                "webUsage": output.usage,
            }),
            duration_ms: bounded_duration_ms(call_started.elapsed()),
            tokens_used: None,
            error: None,
        })
    }

    fn request_change_confirmation(
        &self,
        call: &crate::ai_runtime::ToolCall,
        entry: &crate::ai_runtime::tool_catalog::ToolCatalogEntry,
        args: &serde_json::Value,
        gate: &ToolExecutionGate<'_>,
        decision: &crate::ai_runtime::permission_decision::PermissionDecisionOutcome,
        state_version: u64,
    ) -> AppResult<()> {
        let plan = self.freeze_change_plan(call, entry, args)?;
        let summary = format!(
            "等待确认：{} 将修改 {} 个目标",
            entry.name,
            plan.relative_paths().len()
        );
        let event = AgentRunRepository::request_frozen_confirmation(
            &self.state.db,
            &plan,
            state_version,
            &summary,
        )?;
        // The state transition is authoritative. The audit uses only the catalog
        // capability and preflight metadata, never the frozen arguments.
        audit_tool_confirmation_requested(&self.state.db, gate, decision)?;
        self.sink.emit(&event)
    }

    fn freeze_change_plan(
        &self,
        call: &crate::ai_runtime::ToolCall,
        entry: &crate::ai_runtime::tool_catalog::ToolCatalogEntry,
        args: &serde_json::Value,
    ) -> AppResult<crate::ai_runtime::frozen_change_plan::FrozenChangePlan> {
        let relative_paths = frozen_relative_paths(entry.name, args, self.context);
        let base_content_hashes = frozen_base_content_hashes(args, self.context, &relative_paths);
        let vault_id = self
            .state
            .vault_path()
            .map(|vault| crate::cas::hash::content_hash_str(&vault.to_string_lossy()))
            .unwrap_or_else(|_| format!("normal-session:{}", self.context.session_id));
        crate::ai_runtime::frozen_change_plan::FrozenChangePlan::freeze(
            crate::ai_runtime::frozen_change_plan::FrozenChangePlanInput {
                confirmation_id: uuid::Uuid::new_v4().to_string(),
                run_id: self.accepted.run_id.clone(),
                session_id: self.context.session_id,
                request_id: self.accepted.run_id.clone(),
                tool_call_id: call.id.clone(),
                vault_id,
                affected_file_count: relative_paths.len(),
                relative_paths,
                operation: entry.name.to_string(),
                base_content_hashes,
                change: args.clone(),
                rollback_summary: rollback_summary(entry.name),
                expires_at_unix_ms: chrono::Utc::now().timestamp_millis()
                    + CHANGE_CONFIRMATION_TTL_MS,
            },
        )
    }

    /// Dispatch one previously approved, hash-bound plan without contacting the model.
    pub(crate) async fn execute_confirmed_frozen_change(
        &self,
        plan: &crate::ai_runtime::frozen_change_plan::FrozenChangePlan,
    ) -> AppResult<ToolCallResult> {
        if plan.run_id() != self.accepted.run_id || plan.session_id() != self.context.session_id {
            return Err(AppError::msg("agent_run_confirmation_expired"));
        }
        plan.validate_approval(
            plan.confirmation_id(),
            plan.plan_hash(),
            chrono::Utc::now().timestamp_millis(),
        )?;
        let entry = catalog_find(plan.operation())
            .filter(|entry| {
                entry.requires_confirmation
                    && entry.implementation
                        == crate::ai_runtime::tool_catalog::ToolImplementationStatus::Dispatchable
            })
            .ok_or_else(|| AppError::msg("agent_run_confirmation_expired"))?;
        let args = plan.change();
        let actual_paths = frozen_relative_paths(entry.name, args, self.context);
        if actual_paths != plan.relative_paths() {
            return Err(AppError::msg("agent_run_confirmation_expired"));
        }
        revalidate_frozen_base_hashes(self.state.as_ref(), plan)?;
        let snapshot = AgentRunRepository::get_for_session(
            &self.state.db,
            &self.accepted.session.session_key,
            &self.accepted.run_id,
        )?
        .ok_or_else(|| AppError::msg("agent_run_not_found"))?;
        if snapshot.run.state != crate::ai_runtime::run_contract::RunState::Running {
            return Err(AppError::msg("agent_run_illegal_transition"));
        }
        let gate = ToolExecutionGate {
            run_id: &self.accepted.run_id,
            session_id: Some(self.context.session_id),
            run_step: 1,
            entry,
            args,
            policy_ctx: &self.policy_ctx,
            skill_id: None,
            subagent_depth: 0,
        };
        let gate_outcome = evaluate_tool_execution(&self.state.db, gate)?;
        let result = if let Some(result) = gate_outcome.tool_result {
            result
        } else {
            self.dispatch_non_web_tool(entry.name, args).await
        };
        audit_dispatched_tool(&self.state.db, &gate, &gate_outcome.decision, &result)?;
        append_model_tool_completed(
            &self.state.db,
            self.accepted,
            snapshot.run.state_version,
            self.sink,
            entry.name,
            plan.tool_call_id(),
            if result.success {
                "已执行已确认的变更"
            } else {
                "已确认的变更未执行"
            },
        )?;
        Ok(result)
    }

    async fn dispatch_non_web_tool(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> ToolCallResult {
        let dispatch_context = ToolDispatchContext {
            note_path: None,
            file_id: None,
            web_search_enabled: self.policy_ctx.web_search_enabled,
            max_web_fetches: 5,
            cold_start_packets: &self.cold_start_packets,
            retrieval_scope: &self.retrieval_scope,
            runtime_documents: &self.runtime_documents,
            app_handle: Some(self.app_handle.clone()),
            attachment_count: 0,
            skill_activation_plan: None,
        };
        dispatch_tool_with_retry(self.state.as_ref(), &dispatch_context, tool_name, args).await
    }
}

impl ToolLoopExecutor for NormalRunToolExecutor<'_> {
    fn execute<'a>(
        &'a self,
        run_id: &'a str,
        call: &'a crate::ai_runtime::ToolCall,
        step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        Box::pin(async move {
            let Some(entry) = catalog_find(&call.function.name) else {
                return Ok(failed_tool_call(
                    &call.function.name,
                    "tool_not_in_run_surface",
                ));
            };
            let args = match serde_json::from_str::<serde_json::Value>(&call.function.arguments) {
                Ok(value) if value.is_object() => value,
                _ => {
                    return Ok(failed_tool_call(
                        &call.function.name,
                        "tool_arguments_invalid",
                    ))
                }
            };
            let gate = ToolExecutionGate {
                run_id,
                session_id: Some(self.context.session_id),
                run_step: step,
                entry,
                args: &args,
                policy_ctx: &self.policy_ctx,
                skill_id: None,
                subagent_depth: 0,
            };
            let gate_outcome = match evaluate_tool_execution(&self.state.db, gate) {
                Ok(outcome) => outcome,
                Err(_) => return Err(AppError::msg("tool_permission_check_failed")),
            };
            let state_version = match append_model_tool_started(
                &self.state.db,
                self.accepted,
                self.sink,
                &call.function.name,
                &call.id,
            ) {
                Ok(version) => version,
                Err(_) => return Err(AppError::msg("tool_event_persistence_failed")),
            };
            let result = if let Some(result) = gate_outcome.tool_result {
                result
            } else if entry.requires_confirmation {
                self.request_change_confirmation(
                    call,
                    entry,
                    &args,
                    &gate,
                    &gate_outcome.decision,
                    state_version,
                )?;
                return Err(AppError::msg(CONFIRMATION_PENDING_ERROR));
            } else if !gate_outcome.decision.can_execute_now() {
                failed_tool_call(&call.function.name, "tool_confirmation_required")
            } else if call.function.name == WEB_TOOL_NAME {
                self.execute_web_search(&args, state_version).await?
            } else {
                self.dispatch_non_web_tool(&call.function.name, &args).await
            };
            audit_dispatched_tool(&self.state.db, &gate, &gate_outcome.decision, &result)?;
            let summary = if result.success {
                "工具调用完成"
            } else {
                "工具调用未完成"
            };
            append_model_tool_completed(
                &self.state.db,
                self.accepted,
                state_version,
                self.sink,
                &call.function.name,
                &call.id,
                summary,
            )?;
            if call.function.name == WEB_TOOL_NAME {
                let failure = self.web_failure();
                tracing::info!(
                    run_id,
                    web_mode = ?self.context.envelope.freshness,
                    web_reason = ?self.context.envelope.web_reason,
                    web_status = if result.success { "succeeded" } else { "degraded" },
                    web_failure_code = failure.map(|value| value.code.as_str()),
                    web_retryable = failure.is_some_and(|value| value.retryable),
                    web_attempt_count = self.web_attempt_count(),
                    web_duration_bucket = web_duration_bucket(Duration::from_millis(result.duration_ms)),
                    "Run model-decided Web capability outcome"
                );
            }
            Ok(result)
        })
    }

    fn evidence_ids(&self) -> Vec<i64> {
        self.evidence_ids
            .lock()
            .map(|ids| ids.clone())
            .unwrap_or_default()
    }

    fn has_web_evidence(&self) -> bool {
        !self.evidence_ids().is_empty()
    }

    fn emit_deferred_web_degradation_if_needed(
        &self,
        db: &Database,
        sink: &dyn RunEventSink,
    ) -> AppResult<()> {
        NormalRunToolExecutor::emit_deferred_web_degradation_if_needed(self, db, sink)
    }
}

impl NormalRunToolExecutor<'_> {
    /// Emit `capability_degraded` once after a successful tool loop when Web attempts
    /// failed and no usable Web evidence was registered for this Run.
    pub(crate) fn emit_deferred_web_degradation_if_needed(
        &self,
        db: &Database,
        sink: &dyn RunEventSink,
    ) -> AppResult<()> {
        emit_deferred_web_degradation(
            DeferredWebDegradationInput {
                db,
                accepted: self.accepted,
                sink,
                web_failure: self.web_failure(),
                has_web_evidence: self.has_web_evidence(),
                attempt_count: self.web_attempt_count(),
            },
            &mut || self.mark_web_degradation_emitted(),
        )
    }
    fn set_web_failure(&self, failure: Option<WebFailure>) -> AppResult<()> {
        *self
            .web_failure
            .lock()
            .map_err(|_| AppError::msg("agent_run_web_failure_lock_failed"))? = failure;
        Ok(())
    }

    fn web_failure(&self) -> Option<WebFailure> {
        self.web_failure.lock().ok().and_then(|failure| *failure)
    }

    fn reserve_web_attempt(&self) -> AppResult<Option<u32>> {
        let mut attempts = self
            .web_attempt_count
            .lock()
            .map_err(|_| AppError::msg("agent_run_web_attempt_lock_failed"))?;
        if *attempts >= 2 {
            return Ok(None);
        }
        *attempts = attempts.saturating_add(1);
        Ok(Some(*attempts))
    }

    fn web_attempt_count(&self) -> u32 {
        self.web_attempt_count
            .lock()
            .map(|attempts| *attempts)
            .unwrap_or(0)
    }

    fn mark_web_degradation_emitted(&self) -> AppResult<bool> {
        let mut emitted = self
            .web_degradation_emitted
            .lock()
            .map_err(|_| AppError::msg("agent_run_web_degradation_lock_failed"))?;
        if *emitted {
            return Ok(false);
        }
        *emitted = true;
        Ok(true)
    }
}

fn frozen_relative_paths(
    tool_name: &str,
    args: &serde_json::Value,
    context: &RunContext,
) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for key in ["target_path", "path", "new_path", "note_path"] {
        if let Some(path) = args
            .get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|path| !path.trim().is_empty())
        {
            paths.insert(path.trim().replace('\\', "/"));
        }
    }
    if paths.is_empty() && matches!(tool_name, "insert_text_at_cursor" | "replace_selection") {
        if let Some(material) = context.materials.first() {
            paths.insert(material.source_path.clone());
        }
    }
    if paths.is_empty() {
        let target = match tool_name {
            "memory_write" => args
                .get("key")
                .and_then(serde_json::Value::as_str)
                .map(|key| format!("application://memory/{key}")),
            "scheduled_task_create" => Some("application://scheduled-tasks/new".to_string()),
            "scheduled_task_delete" => args
                .get("id")
                .and_then(serde_json::Value::as_i64)
                .map(|id| format!("application://scheduled-tasks/{id}")),
            _ => Some(format!("application://tool/{tool_name}")),
        };
        if let Some(target) = target {
            paths.insert(target);
        }
    }
    paths.into_iter().collect()
}

fn frozen_base_content_hashes(
    args: &serde_json::Value,
    context: &RunContext,
    relative_paths: &[String],
) -> Vec<(String, String)> {
    let mut hashes = BTreeSet::new();
    if let Some(base_hash) = args
        .get("base_content_hash")
        .and_then(serde_json::Value::as_str)
        .filter(|hash| !hash.trim().is_empty())
    {
        if let Some(path) = relative_paths.first() {
            hashes.insert((path.clone(), base_hash.to_string()));
        }
    }
    for material in &context.materials {
        if relative_paths.contains(&material.source_path) {
            hashes.insert((material.source_path.clone(), material.content_hash.clone()));
        }
    }
    hashes.into_iter().collect()
}

fn rollback_summary(tool_name: &str) -> String {
    match tool_name {
        "vault_delete_to_trash" => "可从回收站恢复".to_string(),
        "vault_rename_move" => "可重命名或移动回原位置".to_string(),
        "memory_write" | "scheduled_task_create" | "scheduled_task_delete" => {
            "可通过应用设置撤销或更新".to_string()
        }
        _ => "可通过版本历史或后续编辑撤销".to_string(),
    }
}

fn revalidate_frozen_base_hashes(
    state: &AppState,
    plan: &crate::ai_runtime::frozen_change_plan::FrozenChangePlan,
) -> AppResult<()> {
    if plan.base_content_hashes().is_empty() {
        return Ok(());
    }
    let vault = state
        .vault_path()
        .map_err(|_| AppError::msg("agent_run_confirmation_expired"))?;
    for (path, expected_hash) in plan.base_content_hashes() {
        if path.starts_with("application://") {
            continue;
        }
        let resolved = crate::storage::paths::resolve_vault_path(&vault, path)
            .map_err(|_| AppError::msg("agent_run_confirmation_expired"))?;
        let current = std::fs::read_to_string(resolved)
            .map_err(|_| AppError::msg("agent_run_confirmation_expired"))?;
        if crate::cas::hash::content_hash_str(&current) != *expected_hash {
            return Err(AppError::msg("agent_run_confirmation_expired"));
        }
    }
    Ok(())
}

fn failed_tool_call(tool_name: &str, code: &str) -> ToolCallResult {
    failed_tool_call_with_duration(tool_name, code, Duration::ZERO)
}

fn failed_tool_call_with_duration(
    tool_name: &str,
    code: &str,
    duration: Duration,
) -> ToolCallResult {
    ToolCallResult {
        tool_name: tool_name.to_string(),
        success: false,
        output: serde_json::json!({ "error": code }),
        duration_ms: bounded_duration_ms(duration),
        tokens_used: None,
        error: Some(code.to_string()),
    }
}

fn failed_web_tool_call(
    failure: WebFailure,
    attempt_count: u32,
    duration: Duration,
    remaining_budget_ms: u64,
) -> ToolCallResult {
    ToolCallResult {
        tool_name: WEB_TOOL_NAME.to_string(),
        success: false,
        output: serde_json::json!({
            "capability": "web.search",
            "error": failure.code.as_str(),
            "retryable": failure.retryable,
            "attemptCount": attempt_count,
            "budgetExhausted": remaining_budget_ms == 0,
            "remainingBudgetMs": remaining_budget_ms,
        }),
        duration_ms: bounded_duration_ms(duration),
        tokens_used: None,
        error: Some(failure.code.as_str().to_string()),
    }
}

fn append_model_tool_started(
    db: &Database,
    accepted: &AssistantRunAccepted,
    sink: &dyn RunEventSink,
    capability: &str,
    tool_call_id: &str,
) -> AppResult<u64> {
    let snapshot =
        AgentRunRepository::get_for_session(db, &accepted.session.session_key, &accepted.run_id)?
            .ok_or_else(|| AppError::msg("agent_run_not_found"))?;
    let event = AgentRunRepository::append_event(
        db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: snapshot.run.state_version,
            event_type: RunEventType::ToolStarted,
            payload: RunEventPayload::ToolStarted {
                capability: capability.to_string(),
                tool_call_id: tool_call_id.to_string(),
            },
        },
    )?;
    sink.emit(&event)?;
    Ok(snapshot.run.state_version)
}

#[allow(clippy::too_many_arguments)]
fn append_model_tool_completed(
    db: &Database,
    accepted: &AssistantRunAccepted,
    state_version: u64,
    sink: &dyn RunEventSink,
    capability: &str,
    tool_call_id: &str,
    summary: &str,
) -> AppResult<()> {
    let event = AgentRunRepository::append_event(
        db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version,
            event_type: RunEventType::ToolCompleted,
            payload: RunEventPayload::ToolCompleted {
                capability: capability.to_string(),
                tool_call_id: tool_call_id.to_string(),
                summary: summary.to_string(),
            },
        },
    )?;
    sink.emit(&event)
}

fn register_model_web_evidence(
    db: &Database,
    accepted: &AssistantRunAccepted,
    context: &RunContext,
    sink: &dyn RunEventSink,
    state_version: u64,
    items: &[crate::ai_runtime::web_evidence_broker::WebEvidenceItem],
    limit: usize,
) -> AppResult<Vec<i64>> {
    let mut evidence_ids = Vec::new();
    for item in items
        .iter()
        .filter(|item| item.failure_reason.is_none())
        .filter(|item| {
            item.url.starts_with("https://") && item.canonical_url.starts_with("https://")
        })
        .filter_map(bounded_page_evidence)
        .take(limit)
    {
        let registered = AgentEvidenceRepository::register_web(
            db,
            WebEvidenceInput {
                session_id: context.session_id,
                run_id: accepted.run_id.clone(),
                message_seq_first: context.message_seq_first,
                material_role: MaterialRole::Lookup,
                title: item.title,
                url: item.url,
                normalized_url: item.canonical_url,
                domain: item.domain,
                retrieved_at: chrono::Utc::now().to_rfc3339(),
                provider_id: item.provider_id,
                provider_kind: item.provider_kind,
                raw_result_hash: item.raw_result_hash,
                extraction_method: item.extraction_method,
                bounded_excerpt: item.excerpt,
                retrieval_reason: Some(WEB_TOOL_NAME.to_string()),
                score: None,
                source_rank: None,
                conflict_group: item.conflict_group,
                failure_reason: None,
            },
        )?;
        let event = AgentRunRepository::append_event(
            db,
            AppendRunEventInput {
                run_id: accepted.run_id.clone(),
                state_version,
                event_type: RunEventType::EvidenceRegistered,
                payload: RunEventPayload::EvidenceRegistered {
                    evidence_id: registered.evidence_id.to_string(),
                },
            },
        )?;
        sink.emit(&event)?;
        evidence_ids.push(registered.evidence_id);
    }
    Ok(evidence_ids)
}

fn bounded_duration_ms(duration: Duration) -> u64 {
    if duration.is_zero() {
        0
    } else {
        duration.as_millis().max(1).min(u64::MAX as u128) as u64
    }
}

fn remaining_model_web_budget_ms(elapsed: Duration) -> u64 {
    bounded_duration_ms(MODEL_WEB_EVIDENCE_DEADLINE.saturating_sub(elapsed))
}

fn web_duration_bucket(duration: Duration) -> &'static str {
    if duration.is_zero() {
        "not_started"
    } else if duration < Duration::from_secs(1) {
        "under_1s"
    } else if duration < Duration::from_secs(3) {
        "1s_to_3s"
    } else if duration < MODEL_WEB_EVIDENCE_DEADLINE {
        "3s_to_10s"
    } else {
        "budget_exhausted"
    }
}

struct DeferredWebDegradationInput<'a> {
    db: &'a Database,
    accepted: &'a AssistantRunAccepted,
    sink: &'a dyn RunEventSink,
    web_failure: Option<WebFailure>,
    has_web_evidence: bool,
    attempt_count: u32,
}

fn emit_deferred_web_degradation(
    input: DeferredWebDegradationInput<'_>,
    mark_emitted: &mut dyn FnMut() -> AppResult<bool>,
) -> AppResult<()> {
    let Some(failure) = input.web_failure else {
        return Ok(());
    };
    if input.has_web_evidence {
        return Ok(());
    }
    if mark_emitted()? {
        append_capability_degraded(
            input.db,
            input.accepted,
            input.sink,
            failure,
            input.attempt_count,
        )?;
    }
    Ok(())
}

fn append_capability_degraded(
    db: &Database,
    accepted: &AssistantRunAccepted,
    sink: &dyn RunEventSink,
    failure: WebFailure,
    attempt_count: u32,
) -> AppResult<()> {
    let snapshot =
        AgentRunRepository::get_for_session(db, &accepted.session.session_key, &accepted.run_id)?
            .ok_or_else(|| AppError::msg("agent_run_not_found"))?;
    let event = AgentRunRepository::append_event(
        db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: snapshot.run.state_version,
            event_type: RunEventType::CapabilityDegraded,
            payload: RunEventPayload::CapabilityDegraded {
                capability: "web.search".to_string(),
                code: failure.code,
                retryable: failure.retryable,
                attempt_count,
                message: if failure.code == SafeRunErrorCode::WebProviderAuthFailed {
                    "联网 API Key 无效，请重新输入原始 Key；已继续生成不依赖联网证据的受约束答复。"
                        .to_string()
                } else {
                    "联网核实暂不可用，已继续生成受约束答复。".to_string()
                },
            },
        },
    )?;
    sink.emit(&event)
}

#[derive(Debug, Clone)]
struct BoundedWebItem {
    title: String,
    url: String,
    canonical_url: String,
    domain: String,
    provider_id: String,
    provider_kind: String,
    raw_result_hash: String,
    extraction_method: String,
    excerpt: String,
    conflict_group: Option<String>,
}

fn bounded_page_evidence(
    item: &crate::ai_runtime::web_evidence_broker::WebEvidenceItem,
) -> Option<BoundedWebItem> {
    let excerpt = item
        .fetched_excerpt
        .as_deref()
        .filter(|excerpt| !excerpt.trim().is_empty())
        .unwrap_or(item.snippet.as_str())
        .trim();
    if excerpt.is_empty() {
        return None;
    }
    Some(BoundedWebItem {
        title: item.title.clone(),
        url: item.url.clone(),
        canonical_url: item.canonical_url.clone(),
        domain: item.domain.clone(),
        provider_id: item.provider_id.clone(),
        provider_kind: item.provider_kind.clone(),
        raw_result_hash: item.raw_result_hash.clone(),
        extraction_method: item.extraction_method.clone(),
        excerpt: excerpt.chars().take(MAX_WEB_EXCERPT_CHARS).collect(),
        conflict_group: item.conflict_group.clone(),
    })
}

/// Map the shared Broker/MCP boundary into the small safe vocabulary persisted by a Run.
/// Raw MCP diagnostics can include transport and provider details, so they never cross this
/// boundary directly.
#[cfg(test)]
pub(crate) fn classify_web_evidence_failure(error: &AppError) -> SafeRunErrorCode {
    classify_web_failure(error).code
}

/// Report whether the sanitized Web failure is a known transient condition.
#[cfg(test)]
pub(crate) fn web_evidence_failure_is_retryable(error: &AppError) -> bool {
    classify_web_failure(error).retryable
}

/// Return the structured safe reason selected for a Broker/MCP failure.
#[cfg(test)]
pub(crate) fn web_evidence_failure_reason(error: &AppError) -> WebEvidenceFailureReason {
    classify_web_failure(error).reason
}

fn classify_web_failure(error: &AppError) -> WebFailure {
    let message = error.to_string().to_ascii_lowercase();
    match message.as_str() {
        "agent_run_mcp_unavailable" => {
            WebFailure::new(SafeRunErrorCode::WebProviderUnavailable, false)
        }
        "agent_run_web_provider_timeout" => {
            WebFailure::new(SafeRunErrorCode::WebProviderTimeout, true)
        }
        "agent_run_web_provider_auth_failed" => {
            WebFailure::new(SafeRunErrorCode::WebProviderAuthFailed, false)
        }
        "agent_run_web_provider_failed" => {
            WebFailure::new(SafeRunErrorCode::WebProviderFailed, false)
        }
        "agent_run_web_evidence_invalid" => {
            WebFailure::new(SafeRunErrorCode::WebEvidenceInvalid, false)
        }
        _ if message.contains("timeout")
            || message.contains("timed out")
            || message.contains("deadline") =>
        {
            WebFailure::new(SafeRunErrorCode::WebProviderTimeout, true)
        }
        _ if message.contains("output_too_large") || message.contains("output too large") => {
            WebFailure::with_reason(
                SafeRunErrorCode::WebEvidenceInvalid,
                false,
                WebEvidenceFailureReason::ProviderOutputTooLarge,
            )
        }
        _ if message.contains("mcp_search_parse_empty")
            || message.contains("unrecognized_schema")
            || message.contains("text_without_url") =>
        {
            WebFailure::with_reason(
                SafeRunErrorCode::WebEvidenceInvalid,
                false,
                WebEvidenceFailureReason::SearchResultUnparseable,
            )
        }
        _ if message.contains("mcp_search_no_usable_https_results")
            || message.contains("non_https_rejected") =>
        {
            WebFailure::with_reason(
                SafeRunErrorCode::WebEvidenceInvalid,
                false,
                WebEvidenceFailureReason::SearchResultNoUsableHttps,
            )
        }
        _ if message.contains("web_evidence_unavailable") => WebFailure::with_reason(
            SafeRunErrorCode::WebEvidenceInvalid,
            false,
            WebEvidenceFailureReason::EvidenceContentEmpty,
        ),
        _ if message.contains("mcp_provider_rate_limited") => WebFailure::with_reason(
            SafeRunErrorCode::WebProviderFailed,
            true,
            WebEvidenceFailureReason::ProviderRateLimited,
        ),
        _ if message.contains("mcp_provider_quota_exhausted") => WebFailure::with_reason(
            SafeRunErrorCode::WebProviderFailed,
            false,
            WebEvidenceFailureReason::ProviderQuotaExhausted,
        ),
        _ if message.contains("mcp_provider_invalid_arguments") => WebFailure::with_reason(
            SafeRunErrorCode::WebProviderFailed,
            false,
            WebEvidenceFailureReason::ProviderInvalidArguments,
        ),
        _ if message.contains("web_search_provider_missing")
            || message.contains("web_search_provider_unavailable")
            || message.contains("agent_run_web_tool_missing")
            || message.contains("circuit_open") =>
        {
            WebFailure::new(SafeRunErrorCode::WebProviderUnavailable, false)
        }
        _ if message.contains("connection reset")
            || message.contains("connection refused")
            || message.contains("connection aborted")
            || message.contains("broken pipe")
            || message.contains("temporarily unavailable")
            || message.contains("service unavailable")
            || message.contains("transport interrupted")
            || message.contains("network unreachable")
            || message.contains("mcp_provider_transport_error") =>
        {
            WebFailure::with_reason(
                SafeRunErrorCode::WebProviderFailed,
                true,
                WebEvidenceFailureReason::ProviderTransport,
            )
        }
        _ => WebFailure::new(SafeRunErrorCode::WebProviderFailed, false),
    }
}

fn classify_web_evidence_output_failure(
    output: &crate::ai_runtime::web_evidence_broker::WebEvidenceBrokerOutput,
) -> WebFailure {
    let reasons = output
        .items
        .iter()
        .filter_map(|item| item.failure_reason.as_deref())
        .collect::<Vec<_>>();
    if reasons.is_empty() {
        return WebFailure::new(SafeRunErrorCode::WebEvidenceInvalid, false);
    }
    classify_web_failure(&AppError::msg(reasons.join("; ")))
}

fn web_output_has_usable_evidence(
    output: &crate::ai_runtime::web_evidence_broker::WebEvidenceBrokerOutput,
) -> bool {
    output.items.iter().any(|item| {
        item.failure_reason.is_none()
            && item.url.starts_with("https://")
            && item.canonical_url.starts_with("https://")
            && bounded_page_evidence(item).is_some()
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::{emit_deferred_web_degradation, DeferredWebDegradationInput, RunWebBudget};
    use crate::ai_runtime::run_contract::{
        AssistantRunEvent, AssistantRunStartRequest, AssistantTurnDraft, SafeRunErrorCode,
        SecurityDomain,
    };
    use crate::ai_runtime::run_engine::RunEventSink;
    use crate::ai_runtime::run_intake::RunIntake;
    use crate::error::AppResult;
    use crate::storage::db::Database;

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<serde_json::Value>>,
    }

    impl RunEventSink for RecordingSink {
        fn emit(&self, event: &AssistantRunEvent) -> AppResult<()> {
            self.events
                .lock()
                .expect("recording sink lock")
                .push(serde_json::to_value(event)?);
            Ok(())
        }
    }

    fn request() -> AssistantRunStartRequest {
        AssistantRunStartRequest {
            client_request_id: "deferred-web-client".to_string(),
            session: None,
            turn: AssistantTurnDraft {
                message: "请联网核实".to_string(),
                content_parts: None,
                explicit_references: vec![],
                retrieval_scope: Default::default(),
                display_mentions: vec![],
            },
            explicit_action: None,
            web_enabled: true,
            model_override: None,
            security_domain: SecurityDomain::Normal,
            classified_context_ref: None,
        }
    }

    fn web_failure() -> super::WebFailure {
        super::WebFailure::new(SafeRunErrorCode::WebProviderTimeout, true)
    }

    fn capability_degraded_count(events: &[serde_json::Value]) -> usize {
        events
            .iter()
            .filter(|event| event["type"] == "capability_degraded")
            .count()
    }

    #[test]
    fn model_web_calls_share_one_run_budget_start() {
        let budget = RunWebBudget::default();
        let first = budget.started().expect("first budget start");
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = budget.started().expect("second budget start");

        assert_eq!(first, second);
    }

    #[test]
    fn deferred_web_degradation_emits_once_when_failed_without_evidence() {
        let db = Database::open_in_memory().expect("database");
        let accepted = RunIntake::start(&db, request()).expect("accepted");
        let sink = RecordingSink::default();
        let mut emitted = false;

        emit_deferred_web_degradation(
            DeferredWebDegradationInput {
                db: &db,
                accepted: &accepted,
                sink: &sink,
                web_failure: Some(web_failure()),
                has_web_evidence: false,
                attempt_count: 2,
            },
            &mut || {
                if emitted {
                    return Ok(false);
                }
                emitted = true;
                Ok(true)
            },
        )
        .expect("emit deferred degradation");

        let events = sink.events.lock().expect("events");
        assert_eq!(capability_degraded_count(&events), 1);
        assert_eq!(events[0]["type"], "capability_degraded");
        assert_eq!(
            events[0]["payload"]["code"],
            SafeRunErrorCode::WebProviderTimeout.as_str()
        );

        emit_deferred_web_degradation(
            DeferredWebDegradationInput {
                db: &db,
                accepted: &accepted,
                sink: &sink,
                web_failure: Some(web_failure()),
                has_web_evidence: false,
                attempt_count: 2,
            },
            &mut || Ok(false),
        )
        .expect("second emit is idempotent");
        assert_eq!(capability_degraded_count(&events), 1);
    }

    #[test]
    fn deferred_web_degradation_skips_after_successful_web_evidence() {
        let db = Database::open_in_memory().expect("database");
        let accepted = RunIntake::start(&db, request()).expect("accepted");
        let sink = RecordingSink::default();

        emit_deferred_web_degradation(
            DeferredWebDegradationInput {
                db: &db,
                accepted: &accepted,
                sink: &sink,
                web_failure: None,
                has_web_evidence: true,
                attempt_count: 2,
            },
            &mut || Ok(true),
        )
        .expect("success path should not emit");

        let events = sink.events.lock().expect("events");
        assert_eq!(capability_degraded_count(&events), 0);
    }

    #[test]
    fn deferred_web_degradation_skips_when_failure_cleared_after_retry_success() {
        let db = Database::open_in_memory().expect("database");
        let accepted = RunIntake::start(&db, request()).expect("accepted");
        let sink = RecordingSink::default();

        emit_deferred_web_degradation(
            DeferredWebDegradationInput {
                db: &db,
                accepted: &accepted,
                sink: &sink,
                web_failure: None,
                has_web_evidence: true,
                attempt_count: 2,
            },
            &mut || Ok(true),
        )
        .expect("cleared failure with evidence should not emit");

        let events = sink.events.lock().expect("events");
        assert_eq!(capability_degraded_count(&events), 0);
    }
}
