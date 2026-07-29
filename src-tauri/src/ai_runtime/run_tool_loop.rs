//! Bounded model-driven Web evidence for normal-domain Runs.
//!
//! This module owns the `web_search` tool path used by `NormalRunToolExecutor`: policy/audit
//! gates, bounded evidence registration, and deferred `CapabilityDegraded` emission when an
//! authorized Web search fails without usable evidence. Runs without `web.search` never enable
//! the tool.

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::ai_runtime::agent_evidence_repository::{
    AgentEvidenceRepository, MaterialRole, WebEvidenceInput,
};
use crate::ai_runtime::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
use crate::ai_runtime::agent_tool_loop::{
    AgentToolLoop, ToolLoopExecutor, ToolLoopProvider, MAX_WEB_TOOL_RESULT_CHARS,
};
use crate::ai_runtime::model_gateway::{StreamEvent, StreamEventObserver};
use crate::ai_runtime::run_context::RunContext;
use crate::ai_runtime::run_contract::{
    AssistantRunAccepted, CapabilityId, RunEventPayload, RunEventType, SafeRunErrorCode,
    WebDecisionReason, WebEvidenceFailureReason,
};
use crate::ai_runtime::run_engine::RunEventSink;
use crate::ai_runtime::tool_audit::record_web_query_taint_witness;
use crate::ai_runtime::tool_catalog::catalog_find;
use crate::ai_runtime::tool_dispatch::{dispatch_tool_with_retry, ToolDispatchContext};
use crate::ai_runtime::tool_execution_pipeline::{
    audit_dispatched_tool, audit_tool_confirmation_requested, evaluate_tool_execution,
    ToolExecutionGate,
};
use crate::ai_runtime::tool_executor::ToolRegistry;
use crate::ai_runtime::{LlmMessage, MessageRole, ToolCallResult};
use crate::ai_types::WebSourceRank;
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
const MODEL_WEB_EVIDENCE_DEADLINE: Duration = Duration::from_secs(20);
/// Minimum remaining budget required before retrying a failed web search attempt.
/// Spawning a fresh MCP stdio process commonly takes 3-5s; retrying with less
/// than this budget just burns time before the outer timeout fires.
const MIN_RETRY_BUDGET: Duration = Duration::from_secs(5);
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct McpFailoverEvent {
    from_provider_id: String,
    provider_id: String,
    model_id: String,
    reason_code: String,
    attempt: u32,
}

fn mcp_failover_events(
    snapshots: &[crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary],
    winner_provider_id: &str,
) -> Vec<McpFailoverEvent> {
    let Some(winner_index) = snapshots
        .iter()
        .position(|snapshot| snapshot.id == winner_provider_id)
    else {
        return Vec::new();
    };
    snapshots
        .windows(2)
        .take(winner_index)
        .enumerate()
        .map(|(index, pair)| McpFailoverEvent {
            from_provider_id: pair[0].id.clone(),
            provider_id: pair[1].id.clone(),
            model_id: mcp_mapping_tool_name(pair[1].web_search_mapping_json.as_deref()),
            reason_code: "mcp_provider_failed".into(),
            attempt: (index + 2) as u32,
        })
        .collect()
}

fn mcp_mapping_tool_name(mapping_json: Option<&str>) -> String {
    mapping_json
        .and_then(|mapping| serde_json::from_str::<serde_json::Value>(mapping).ok())
        .and_then(|mapping| {
            mapping
                .get("tool")
                .or_else(|| mapping.get("tool_name"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|tool| !tool.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_default()
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
    app_handle: Option<tauri::AppHandle>,
    accepted: &'a AssistantRunAccepted,
    context: &'a RunContext,
    authorized_capabilities: Vec<CapabilityId>,
    /// The exact cached Skill plan selected before this Run entered the model.
    /// It is inherited by ChildRuns only as a scope restriction; Skill bodies
    /// never become tools or a second authorization path.
    skill_activation_plan: Option<crate::ai_types::SkillActivationPlanSummary>,
    sink: &'a dyn RunEventSink,
    retrieval_scope: crate::ai_runtime::retrieval_scope::RetrievalScope,
    cold_start_packets: Vec<crate::ai_runtime::ContextPacket>,
    runtime_documents: Vec<crate::ai_runtime::RuntimeDocumentSnapshot>,
    evidence_ids: Mutex<Vec<i64>>,
    web_evidence_domains: Mutex<BTreeSet<String>>,
    web_evidence_has_official_source: Mutex<bool>,
    web_failure: Mutex<Option<WebFailure>>,
    web_attempt_count: Mutex<u32>,
    web_budget: RunWebBudget,
    web_degradation_emitted: Mutex<bool>,
    required_web_provider_snapshots:
        Vec<crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary>,
    web_preferred_provider_id: Mutex<Option<String>>,
    /// The parent Run's provider, used only for a bounded depth-one ChildRun.
    /// The ChildRun retains the parent Run identity and persistence boundary.
    child_run_provider: Option<&'a dyn ToolLoopProvider>,
    /// Audit depth of this executor. Only `0` may launch a ChildRun.
    subagent_depth: u32,
}

impl<'a> NormalRunToolExecutor<'a> {
    /// Create a Run-bound executor for the already-authorized normal domain.
    pub(crate) fn new(
        state: &'a Arc<AppState>,
        app_handle: Option<tauri::AppHandle>,
        accepted: &'a AssistantRunAccepted,
        context: &'a RunContext,
        authorized_capabilities: Vec<CapabilityId>,
        sink: &'a dyn RunEventSink,
        required_web_provider_snapshots: Vec<
            crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary,
        >,
    ) -> Self {
        Self {
            state,
            app_handle,
            accepted,
            context,
            authorized_capabilities,
            skill_activation_plan: None,
            sink,
            retrieval_scope: context.retrieval_scope.clone(),
            cold_start_packets: context.local_retrieval_packets.clone(),
            runtime_documents: Vec::new(),
            evidence_ids: Mutex::new(Vec::new()),
            web_evidence_domains: Mutex::new(BTreeSet::new()),
            web_evidence_has_official_source: Mutex::new(false),
            web_failure: Mutex::new(None),
            web_attempt_count: Mutex::new(0),
            web_budget: RunWebBudget::default(),
            web_degradation_emitted: Mutex::new(false),
            required_web_provider_snapshots,
            web_preferred_provider_id: Mutex::new(None),
            child_run_provider: None,
            subagent_depth: 0,
        }
    }

    /// Enable real ChildRun execution with the same provider route selected for
    /// the parent Run. Keeping this as a builder leaves confirmation replay and
    /// other non-model executor callers without an accidental child capability.
    pub(crate) fn with_child_run_provider(mut self, provider: &'a dyn ToolLoopProvider) -> Self {
        self.child_run_provider = Some(provider);
        self
    }

    /// Bind the prompt-only Skill plan selected from the in-memory vault cache.
    /// The executor passes it only to the normal dispatch context so existing
    /// retrieval-scope checks can enforce confirmed Skill scope rules.
    pub(crate) fn with_skill_activation_plan(
        mut self,
        plan: Option<crate::ai_types::SkillActivationPlanSummary>,
    ) -> Self {
        self.skill_activation_plan = plan;
        self
    }

    fn at_subagent_depth(mut self, depth: u32) -> Self {
        self.subagent_depth = depth;
        self
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
        record_web_query_taint_witness(
            &self.state.db,
            &self.accepted.run_id,
            u32::try_from(state_version).unwrap_or(u32::MAX),
            query,
            self.context
                .materials
                .iter()
                .map(|material| material.content.clone()),
        )?;
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
        // Model web calls share MODEL_WEB_EVIDENCE_DEADLINE (20s). MCP search alone
        // commonly takes ~4s; scheduling deep page fetches (WEB_FETCH_TURN_BUDGET=8s)
        // after that exceeds the outer timeout and discards already-usable search
        // snippets. Prefer registering search snippets first.
        let provider_snapshots = self.ordered_web_provider_snapshots();
        let broker_input = crate::ai_runtime::web_evidence_broker::WebEvidenceBrokerInput {
            query: query.to_owned(),
            urls,
            enabled: self.has_capability("web.search"),
            max_search_results: web_search_result_limit(remaining, 1),
            max_fetches: 0,
            provider_snapshots: provider_snapshots.clone(),
            provider_selection_frozen: true,
        };
        let budget_started = self.web_budget.started()?;
        let call_started = Instant::now();
        let output =
            loop {
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
                let mut attempt_input = broker_input.clone();
                attempt_input.max_search_results =
                    web_search_result_limit(remaining, attempt_count);
                let failure = match tokio::time::timeout(
                remaining_time,
                crate::ai_runtime::web_evidence_broker::collect_initial_run_web_evidence_with_usage(
                    &self.state.db,
                    attempt_input,
                ),
            )
            .await
            {
                Ok(Ok(output)) if output.items.iter().any(|item| item.conflict_group.is_some()) => {
                    WebFailure::new(SafeRunErrorCode::WebEvidenceInvalid, false)
                }
                Ok(Ok(output)) if web_output_has_usable_evidence(&output) => break output,
                Ok(Ok(output)) => classify_web_evidence_output_failure(&output),
                Ok(Err(error)) => classify_web_failure(&error),
                Err(_) => WebFailure::new(SafeRunErrorCode::WebProviderTimeout, true),
            };
                let adaptive_oversize_retry =
                    failure.reason == WebEvidenceFailureReason::ProviderOutputTooLarge;
                if attempt_count < 2
                    && (failure.retryable || adaptive_oversize_retry)
                    && budget_started.elapsed() + Duration::from_millis(250) + MIN_RETRY_BUDGET
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
        self.remember_web_provider_winner(&output.usage)?;
        self.emit_mcp_failover_events(&provider_snapshots, &output.usage)?;
        let packed_items =
            match pack_web_evidence_for_model(query, &output.items, remaining, &output.usage) {
                Ok(items) if !items.is_empty() => items,
                Ok(_) | Err(_) => {
                    let failure = WebFailure::new(SafeRunErrorCode::WebEvidenceInvalid, false);
                    self.set_web_failure(Some(failure))?;
                    return Ok(failed_web_tool_call(
                        failure,
                        self.web_attempt_count(),
                        call_started.elapsed(),
                        remaining_model_web_budget_ms(budget_started.elapsed()),
                    ));
                }
            };
        let evidence_ids = register_model_web_evidence(
            &self.state.db,
            self.accepted,
            self.context,
            self.sink,
            state_version,
            &packed_items,
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
        self.record_web_evidence_quality(&packed_items)?;
        let packets = crate::ai_runtime::web_evidence_broker::web_evidence_items_to_packets_with_excerpt_limit(
            query,
            &packed_items,
            MAX_WEB_EXCERPT_CHARS,
        );
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
            authorized_capabilities: &self.authorized_capabilities,
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
            result.duration_ms,
            result.success,
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
            run_id: Some(&self.accepted.run_id),
            write_target_path: self.context.write_target_path.as_deref(),
            document_policy: Some(&self.context.document_policy),
            web_search_enabled: self.has_capability("web.search"),
            max_web_fetches: 5,
            cold_start_packets: &self.cold_start_packets,
            retrieval_scope: &self.retrieval_scope,
            runtime_documents: &self.runtime_documents,
            app_handle: self.app_handle.clone(),
            attachment_count: 0,
            skill_activation_plan: self.skill_activation_plan.as_ref(),
        };
        dispatch_tool_with_retry(self.state.as_ref(), &dispatch_context, tool_name, args).await
    }

    /// Return already-authorized note text without a second vault read when the
    /// requested path was injected as a Run material or exact-path fallback chunk.
    fn cached_authorized_note(&self, args: &serde_json::Value) -> Option<ToolCallResult> {
        let path = args.get("path").and_then(serde_json::Value::as_str)?;
        let normalized = crate::ai_runtime::retrieval_scope::normalize_note_path(path).ok()?;
        let max_chars = args
            .get("max_chars")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(12_000) as usize;

        if let Some(material) = self
            .context
            .materials
            .iter()
            .find(|material| material.source_path == normalized)
        {
            let truncated = material.content.chars().count() > max_chars;
            let body: String = material.content.chars().take(max_chars).collect();
            return Some(ToolCallResult {
                tool_name: "read_note".to_string(),
                success: true,
                output: serde_json::json!({
                    "path": normalized,
                    "content": body,
                    "truncated": truncated,
                    "cached": true,
                }),
                duration_ms: 0,
                tokens_used: None,
                error: None,
            });
        }

        let packet = self.cold_start_packets.iter().find(|packet| {
            packet
                .source_path
                .as_deref()
                .is_some_and(|source| source == normalized)
        })?;
        let truncated = packet.excerpt.chars().count() > max_chars;
        let body: String = packet.excerpt.chars().take(max_chars).collect();
        Some(ToolCallResult {
            tool_name: "read_note".to_string(),
            success: true,
            output: serde_json::json!({
                "path": normalized,
                "content": body,
                "truncated": truncated,
                "cached": true,
            }),
            duration_ms: 0,
            tokens_used: None,
            error: None,
        })
    }
}

/// Select the provider-facing result count for one bounded Web attempt.
///
/// The Run may ultimately register up to eight evidence rows, but an MCP
/// provider must never be asked for that many raw search bodies in one strict
/// prefetch. A response that exceeds the host cap gets exactly one smaller
/// retry; this preserves the cap rather than hiding an unbounded payload.
fn web_search_result_limit(remaining: usize, attempt_count: u32) -> usize {
    let first_attempt = remaining.clamp(1, INITIAL_WEB_SEARCH_RESULTS);
    if attempt_count >= 2 {
        first_attempt.min(2)
    } else {
        first_attempt
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
                authorized_capabilities: &self.authorized_capabilities,
                skill_id: None,
                subagent_depth: self.subagent_depth,
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
            } else if call.function.name == "spawn_subagent" {
                if self.subagent_depth != 0 {
                    failed_tool_call(&call.function.name, "subagent_depth_exceeded")
                } else {
                    self.execute_child_run(run_id, call, &args).await?
                }
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
            } else if call.function.name == "read_note" {
                if let Some(cached) = self.cached_authorized_note(&args) {
                    cached
                } else {
                    self.dispatch_non_web_tool(&call.function.name, &args).await
                }
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
                result.duration_ms,
                result.success,
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
        // This vector is populated only after this executor's successful
        // `web_search` call has persisted a Run-level evidence association.
        // It deliberately cannot be satisfied by session history or a prior
        // Run's citations.
        if self.evidence_ids().is_empty() {
            return false;
        }
        if !self.requires_corroborated_web_evidence() {
            return true;
        }
        let has_official = self
            .web_evidence_has_official_source
            .lock()
            .map(|value| *value)
            .unwrap_or(false);
        let independent_domains = self
            .web_evidence_domains
            .lock()
            .map(|domains| domains.len())
            .unwrap_or(0);
        corroborated_source_threshold_met(has_official, independent_domains)
    }

    fn requires_web_evidence(&self) -> bool {
        // A ChildRun produces an internal tool result, never the parent Run's
        // final factual answer. Requiring the parent's Web proof here would
        // reject trusted runtime operations (for example system_time_now)
        // before the parent can continue. The depth-zero parent still owns the
        // terminal gate and cannot complete an external factual response
        // without current-run Web evidence.
        self.subagent_depth == 0
            && self.context.envelope.verification_requirement
                == crate::ai_runtime::run_contract::VerificationRequirement::CurrentRunWeb
    }

    fn emit_deferred_web_degradation_if_needed(
        &self,
        db: &Database,
        sink: &dyn RunEventSink,
    ) -> AppResult<bool> {
        NormalRunToolExecutor::emit_deferred_web_degradation_if_needed(self, db, sink)
    }
}

impl NormalRunToolExecutor<'_> {
    fn requires_corroborated_web_evidence(&self) -> bool {
        matches!(
            self.context.envelope.web_reason,
            WebDecisionReason::VolatileExternalFact | WebDecisionReason::HighStakesCurrentFact
        )
    }

    fn record_web_evidence_quality(
        &self,
        items: &[crate::ai_runtime::web_evidence_broker::WebEvidenceItem],
    ) -> AppResult<()> {
        let mut domains = self
            .web_evidence_domains
            .lock()
            .map_err(|_| AppError::msg("agent_run_web_evidence_lock_failed"))?;
        let mut has_official = self
            .web_evidence_has_official_source
            .lock()
            .map_err(|_| AppError::msg("agent_run_web_evidence_lock_failed"))?;
        for item in items {
            if item.failure_reason.is_none()
                && item.url.starts_with("https://")
                && item.canonical_url.starts_with("https://")
                && bounded_page_evidence(item).is_some()
            {
                if !item.domain.trim().is_empty() {
                    domains.insert(item.domain.to_ascii_lowercase());
                }
                *has_official |= item.source_rank == WebSourceRank::Official;
            }
        }
        Ok(())
    }

    /// Execute one bounded ChildRun inside the parent Run's provider, policy,
    /// evidence and audit boundary. There is intentionally no child task table
    /// or child lifecycle: only the parent Run may persist an effect or ask the
    /// user for confirmation.
    async fn execute_child_run(
        &self,
        run_id: &str,
        call: &crate::ai_runtime::ToolCall,
        args: &serde_json::Value,
    ) -> AppResult<ToolCallResult> {
        let started = Instant::now();
        let Some(provider) = self.child_run_provider else {
            return Ok(failed_tool_call(
                &call.function.name,
                "child_run_provider_unavailable",
            ));
        };

        let registry = ToolRegistry::new();
        let parent_surface = ToolRegistry::constrain_for_explicit_references(
            registry.tools_for_authorized_capabilities(&self.authorized_capabilities, true),
            self.context.envelope.context,
            &self.context.retrieval_scope,
        );
        let inherited_tool_names = parent_surface
            .iter()
            .map(|tool| tool.name.clone())
            .collect::<Vec<_>>();
        let inherited_tool_names =
            crate::ai_runtime::subagent_coordinator::SubAgentCoordinator::child_tool_surface(
                &inherited_tool_names,
            );
        let spec = crate::ai_runtime::subagent_coordinator::SubAgentTaskSpec::from_tool_call(
            run_id,
            call,
            self.context
                .materials
                .first()
                .map(|material| material.source_path.as_str()),
            self.evidence_ids()
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
            inherited_tool_names,
            Some(2_000),
        );
        if spec.resource_locks.iter().any(|lock| {
            lock.access == crate::ai_runtime::subagent_coordinator::ResourceAccess::Write
        }) {
            let report = crate::ai_runtime::subagent_coordinator::SubAgentCoordinator::report_error(
                &spec,
                "child_run_write_lock_forbidden",
            );
            return Ok(child_report_tool_result(
                &call.function.name,
                report,
                0,
                started.elapsed(),
            ));
        }
        if spec.allowed_tools.is_empty() {
            let report = crate::ai_runtime::subagent_coordinator::SubAgentCoordinator::report_error(
                &spec,
                "child_run_no_safe_authorized_tools",
            );
            return Ok(child_report_tool_result(
                &call.function.name,
                report,
                0,
                started.elapsed(),
            ));
        }
        let tools = parent_surface
            .into_iter()
            .filter(|tool| spec.allowed_tools.contains(&tool.name))
            .collect::<Vec<_>>();
        let max_rounds = args
            .get("max_rounds")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(2)
            .clamp(1, 2);
        let messages = vec![
            LlmMessage {
                role: MessageRole::System,
                content: format!(
                    "你是父级 Agent 的受限子任务执行者，角色为 {}。只可使用列出的只读或联网工具；禁止修改数据、创建确认、调用 spawn_subagent，最后给出可供父级核验的简明结论。",
                    spec.role
                )
                .into(),
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            },
            LlmMessage {
                role: MessageRole::User,
                content: spec.task.clone().into(),
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            },
        ];
        let child_executor = NormalRunToolExecutor::new(
            self.state,
            self.app_handle.clone(),
            self.accepted,
            self.context,
            self.authorized_capabilities.clone(),
            self.sink,
            self.required_web_provider_snapshots.clone(),
        )
        .with_skill_activation_plan(self.skill_activation_plan.clone())
        .at_subagent_depth(1);
        let mut observer = ChildRunStreamObserver;
        let outcome = AgentToolLoop::with_limits(max_rounds, 6)
            .execute(
                provider,
                &child_executor,
                run_id,
                messages,
                tools,
                &mut observer,
            )
            .await;
        let result = match outcome {
            Ok(outcome) => {
                let citation_valid = !child_executor.evidence_ids().is_empty()
                    || !spec.input_evidence_ids.is_empty();
                let report =
                    crate::ai_runtime::subagent_coordinator::SubAgentCoordinator::report_success(
                        &spec,
                        outcome.content,
                        citation_valid,
                        outcome.model_turns,
                    );
                child_report_tool_result(
                    &call.function.name,
                    report,
                    outcome.model_turns,
                    started.elapsed(),
                )
            }
            Err(error) => {
                let report =
                    crate::ai_runtime::subagent_coordinator::SubAgentCoordinator::report_error(
                        &spec,
                        sanitize_child_run_error(&error),
                    );
                child_report_tool_result(&call.function.name, report, 0, started.elapsed())
            }
        };
        Ok(result)
    }

    fn has_capability(&self, required: &str) -> bool {
        self.authorized_capabilities
            .iter()
            .any(|capability| capability.as_str() == required)
    }

    fn ordered_web_provider_snapshots(
        &self,
    ) -> Vec<crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary> {
        let preferred = self
            .web_preferred_provider_id
            .lock()
            .ok()
            .and_then(|value| value.clone());
        let mut snapshots = self.required_web_provider_snapshots.clone();
        if let Some(preferred) = preferred {
            if let Some(index) = snapshots
                .iter()
                .position(|snapshot| snapshot.id == preferred)
            {
                let winner = snapshots.remove(index);
                snapshots.insert(0, winner);
            }
        }
        snapshots
    }

    fn remember_web_provider_winner(
        &self,
        usage: &crate::ai_runtime::web_evidence_broker::WebEvidenceUsage,
    ) -> AppResult<()> {
        let Some(winner) = usage.providers.iter().find_map(|provider| {
            (provider.successful_search_requests > 0).then(|| provider.provider_id.clone())
        }) else {
            return Ok(());
        };
        *self
            .web_preferred_provider_id
            .lock()
            .map_err(|_| AppError::msg("agent_run_web_provider_lock_failed"))? = Some(winner);
        Ok(())
    }

    fn emit_mcp_failover_events(
        &self,
        provider_snapshots: &[crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary],
        usage: &crate::ai_runtime::web_evidence_broker::WebEvidenceUsage,
    ) -> AppResult<()> {
        let Some(winner) = usage.providers.iter().find_map(|provider| {
            (provider.successful_search_requests > 0).then_some(provider.provider_id.as_str())
        }) else {
            return Ok(());
        };
        for event in mcp_failover_events(provider_snapshots, winner) {
            let snapshot = AgentRunRepository::get_for_session(
                &self.state.db,
                &self.accepted.session.session_key,
                &self.accepted.run_id,
            )?
            .ok_or_else(|| AppError::msg("agent_run_not_found"))?;
            let persisted = AgentRunRepository::append_event(
                &self.state.db,
                AppendRunEventInput {
                    run_id: self.accepted.run_id.clone(),
                    state_version: snapshot.run.state_version,
                    event_type: RunEventType::ProviderSwitched,
                    payload: RunEventPayload::ProviderSwitched {
                        capability: "web.search".into(),
                        from_provider_id: event.from_provider_id,
                        provider_id: event.provider_id,
                        // MCP web mappings name tools rather than models.
                        model_id: event.model_id,
                        reason_code: event.reason_code,
                        attempt: event.attempt,
                    },
                },
            )?;
            self.sink.emit(&persisted)?;
        }
        Ok(())
    }

    /// Emit `capability_degraded` once after a successful tool loop when Web attempts
    /// failed and no usable Web evidence was registered for this Run.
    /// Returns `true` when the event was emitted on this call.
    pub(crate) fn emit_deferred_web_degradation_if_needed(
        &self,
        db: &Database,
        sink: &dyn RunEventSink,
    ) -> AppResult<bool> {
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

    /// Return the sanitized state of the deterministic evidence stage. This
    /// crosses the service boundary only as stable UI/event fields; raw search
    /// errors remain inside the executor.
    pub(crate) fn web_verification_failure_details(
        &self,
    ) -> (SafeRunErrorCode, WebEvidenceFailureReason, bool, u32) {
        let failure = self
            .web_failure()
            .unwrap_or_else(|| WebFailure::new(SafeRunErrorCode::WebEvidenceInvalid, false));
        (
            failure.code,
            failure.reason,
            failure.retryable,
            self.web_attempt_count().max(1),
        )
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

/// Child streaming is intentionally not forwarded as parent visible output.
/// Only the normalized report returns to the parent model transcript.
struct ChildRunStreamObserver;

impl StreamEventObserver for ChildRunStreamObserver {
    fn observe(&mut self, _event: &StreamEvent, _token_index: u32) -> AppResult<()> {
        Ok(())
    }
}

fn child_report_tool_result(
    tool_name: &str,
    report: crate::ai_runtime::subagent_coordinator::SubagentReport,
    harness_rounds: u32,
    duration: Duration,
) -> ToolCallResult {
    let success = report.errors.is_empty();
    let error = (!success).then(|| "child_run_failed".to_string());
    ToolCallResult {
        tool_name: tool_name.to_string(),
        success,
        output: serde_json::json!({
            "content": report.summary,
            "citation_valid": report.errors.is_empty(),
            "harness_rounds": harness_rounds,
            "subagent_report": report,
        }),
        duration_ms: bounded_duration_ms(duration),
        tokens_used: None,
        error,
    }
}

fn sanitize_child_run_error(error: &AppError) -> &'static str {
    match error.to_string().as_str() {
        "agent_run_cancelled" => "child_run_cancelled",
        "agent_run_tool_loop_limit" => "child_run_limit_exceeded",
        "agent_run_web_evidence_required" => "child_run_web_evidence_required",
        _ => "child_run_failed",
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
    duration_ms: u64,
    success: bool,
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
                duration_ms: Some(duration_ms),
                success: Some(success),
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
) -> AppResult<bool> {
    let Some(failure) = input.web_failure else {
        return Ok(false);
    };
    if input.has_web_evidence {
        return Ok(false);
    }
    if mark_emitted()? {
        append_capability_degraded(
            input.db,
            input.accepted,
            input.sink,
            failure,
            input.attempt_count,
        )?;
        Ok(true)
    } else {
        Ok(false)
    }
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

/// Select and normalize the exact Web evidence that may reach a model. The
/// same normalized values are later written to the evidence ledger, which
/// prevents a session record from claiming support that the model never saw.
fn pack_web_evidence_for_model(
    query: &str,
    items: &[crate::ai_runtime::web_evidence_broker::WebEvidenceItem],
    limit: usize,
    usage: &crate::ai_runtime::web_evidence_broker::WebEvidenceUsage,
) -> AppResult<Vec<crate::ai_runtime::web_evidence_broker::WebEvidenceItem>> {
    let mut packed = items
        .iter()
        .filter(|item| item.failure_reason.is_none())
        .filter(|item| {
            item.url.starts_with("https://") && item.canonical_url.starts_with("https://")
        })
        .filter_map(|item| {
            let bounded = bounded_page_evidence(item)?;
            let mut item = item.clone();
            item.title = truncate_web_field(&bounded.title, 256);
            item.url = truncate_web_field(&bounded.url, 512);
            item.canonical_url = truncate_web_field(&bounded.canonical_url, 512);
            item.domain = truncate_web_field(&bounded.domain, 255);
            item.provider_id = truncate_web_field(&bounded.provider_id, 128);
            item.provider_kind = truncate_web_field(&bounded.provider_kind, 128);
            item.raw_result_hash = truncate_web_field(&bounded.raw_result_hash, 128);
            item.extraction_method = truncate_web_field(&bounded.extraction_method, 128);
            item.snippet = bounded.excerpt.clone();
            item.fetched_excerpt = Some(bounded.excerpt);
            Some(item)
        })
        .take(limit)
        .collect::<Vec<_>>();

    // The final evidence ids are decimal SQLite identifiers. Reserve their
    // worst-case serialized footprint before persistence, then compact the
    // longest excerpts until the *actual JSON shape* fits. Never leave a
    // later generic string truncation to corrupt the JSON packet.
    let placeholder_ids = vec!["9223372036854775807"; packed.len()];
    while !packed.is_empty()
        && serialized_web_tool_payload_chars(query, &packed, &placeholder_ids, usage)
            > MAX_WEB_TOOL_RESULT_CHARS
    {
        let Some((index, length)) = packed
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                item.fetched_excerpt
                    .as_ref()
                    .map(|excerpt| (index, excerpt.chars().count()))
            })
            .max_by_key(|(_, length)| *length)
        else {
            break;
        };
        if length <= 64 {
            return Err(AppError::msg("agent_run_web_evidence_pack_overflow"));
        }
        let next_length = length.saturating_sub(256).max(64);
        let excerpt = packed[index].fetched_excerpt.as_deref().unwrap_or_default();
        let compact = truncate_web_field(excerpt, next_length);
        packed[index].snippet = compact.clone();
        packed[index].fetched_excerpt = Some(compact);
    }
    if packed.is_empty()
        || serialized_web_tool_payload_chars(query, &packed, &placeholder_ids, usage)
            > MAX_WEB_TOOL_RESULT_CHARS
    {
        return Err(AppError::msg("agent_run_web_evidence_pack_overflow"));
    }
    Ok(packed)
}

fn serialized_web_tool_payload_chars(
    query: &str,
    items: &[crate::ai_runtime::web_evidence_broker::WebEvidenceItem],
    evidence_ids: &[&str],
    usage: &crate::ai_runtime::web_evidence_broker::WebEvidenceUsage,
) -> usize {
    let packets =
        crate::ai_runtime::web_evidence_broker::web_evidence_items_to_packets_with_excerpt_limit(
            query,
            items,
            MAX_WEB_EXCERPT_CHARS,
        );
    serde_json::to_string(&serde_json::json!({
        "success": true,
        "output": {
            "results": packets,
            "evidenceIds": evidence_ids,
            "count": evidence_ids.len(),
            "resultBudget": { "format": "context_packets_only", "rawEvidenceOmitted": true },
            // Reserve the largest possible decimal representation because the
            // remaining budget is produced after the packer has run.
            "remainingBudgetMs": u64::MAX,
            "webUsage": usage,
        },
        "error": serde_json::Value::Null,
    }))
    .map(|value| value.chars().count())
    .unwrap_or(usize::MAX)
}

fn truncate_web_field(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
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

fn corroborated_source_threshold_met(has_official: bool, independent_domains: usize) -> bool {
    has_official || independent_domains >= 2
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    use super::{
        corroborated_source_threshold_met, emit_deferred_web_degradation, mcp_failover_events,
        web_search_result_limit, DeferredWebDegradationInput, McpFailoverEvent,
        NormalRunToolExecutor, RunWebBudget,
    };
    use crate::ai_runtime::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
    use crate::ai_runtime::agent_tool_loop::{ToolLoopExecutor, ToolLoopProvider};
    use crate::ai_runtime::model_gateway::{GatewayResponse, StreamEventObserver};
    use crate::ai_runtime::run_context::RunContextAssembler;
    use crate::ai_runtime::run_contract::{
        AssistantRunEvent, AssistantRunStartRequest, AssistantTurnDraft, CapabilityId,
        RunEventPayload, RunEventType, RunState, SafeRunErrorCode, SecurityDomain,
    };
    use crate::ai_runtime::run_engine::{RunEngine, RunEventSink};
    use crate::ai_runtime::run_intake::RunIntake;
    use crate::ai_runtime::skills::SkillScopeRule;
    use crate::ai_runtime::{FunctionCall, LlmMessage, ToolCall, ToolSpec};
    use crate::ai_types::{SkillActivationItemSummary, SkillActivationPlanSummary};
    use crate::app::AppState;
    use crate::error::AppResult;
    use crate::storage::db::Database;
    use std::sync::Arc;

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

    struct ScriptedChildProvider {
        responses: Mutex<VecDeque<GatewayResponse>>,
        tool_surfaces: Mutex<Vec<Vec<String>>>,
    }

    impl ToolLoopProvider for ScriptedChildProvider {
        fn answer_turn<'a>(
            &'a self,
            _run_id: &'a str,
            _messages: &'a [LlmMessage],
            tools: &'a [ToolSpec],
            _observer: &'a mut dyn StreamEventObserver,
        ) -> Pin<Box<dyn Future<Output = AppResult<GatewayResponse>> + Send + 'a>> {
            self.tool_surfaces
                .lock()
                .expect("tool surfaces")
                .push(tools.iter().map(|tool| tool.name.clone()).collect());
            Box::pin(async move {
                self.responses
                    .lock()
                    .expect("responses")
                    .pop_front()
                    .ok_or_else(|| crate::error::AppError::msg("missing child response"))
            })
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

    #[tokio::test]
    async fn child_run_executes_a_real_bounded_model_tool_loop_with_no_write_or_recursion() {
        let directory = tempfile::tempdir().expect("temporary app directory");
        let state = Arc::new(AppState::new(directory.path().join("data")).expect("state"));
        let mut start = request();
        start.web_enabled = false;
        start.turn.message = "请委派一个子任务读取时间。".to_string();
        let accepted = RunIntake::start(&state.db, start).expect("accepted");
        let context = RunContextAssembler::assemble(
            &state.db,
            None,
            &accepted.session.session_key,
            &accepted.run_id,
        )
        .expect("context");
        let sink = RecordingSink::default();
        let preparing = RunEngine::mark_preparing_with_sink(
            &state.db,
            &accepted.session,
            &accepted.run_id,
            &sink,
        )
        .expect("preparing");
        AgentRunRepository::append_event(
            &state.db,
            AppendRunEventInput {
                run_id: accepted.run_id.clone(),
                state_version: preparing,
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state: RunState::Running,
                    stage: "测试 ChildRun".to_string(),
                },
            },
        )
        .expect("running");
        let provider = ScriptedChildProvider {
            responses: Mutex::new(VecDeque::from([
                GatewayResponse {
                    content: None,
                    tool_calls: vec![ToolCall {
                        id: "child-time".to_string(),
                        call_type: "function".to_string(),
                        function: FunctionCall {
                            name: "system_time_now".to_string(),
                            arguments: "{}".to_string(),
                        },
                    }],
                    usage: Default::default(),
                    finish_reason: "tool_calls".to_string(),
                    reasoning_content: None,
                    continuation: None,
                },
                GatewayResponse {
                    content: Some("子任务已读取当前时间。".to_string()),
                    tool_calls: Vec::new(),
                    usage: Default::default(),
                    finish_reason: "stop".to_string(),
                    reasoning_content: None,
                    continuation: None,
                },
            ])),
            tool_surfaces: Mutex::new(Vec::new()),
        };
        let executor = NormalRunToolExecutor::new(
            &state,
            None,
            &accepted,
            &context,
            vec![
                CapabilityId::new("runtime.read"),
                CapabilityId::new("harness.child_run"),
                CapabilityId::new("memory.write"),
            ],
            &sink,
            Vec::new(),
        )
        .with_child_run_provider(&provider);
        let result = executor
            .execute(
                &accepted.run_id,
                &ToolCall::new(
                    "parent-spawn",
                    "spawn_subagent",
                    r#"{"task":"请联网读取当前时间","allowed_tools":["system_time_now","memory_write","spawn_subagent"]}"#,
                ),
                1,
            )
            .await
            .expect("child execution result");

        assert!(
            result.success,
            "error: {:?}, report: {}",
            result.error, result.output
        );
        assert_eq!(
            result.output["subagent_report"]["summary"],
            "子任务已读取当前时间。"
        );
        let surfaces = provider.tool_surfaces.lock().expect("surfaces");
        assert_eq!(
            surfaces.len(),
            2,
            "child must make a real continuation turn"
        );
        assert!(surfaces[0].contains(&"system_time_now".to_string()));
        assert!(!surfaces[0].contains(&"web_search".to_string()));
        assert!(!surfaces[0].contains(&"memory_write".to_string()));
        assert!(!surfaces[0].contains(&"spawn_subagent".to_string()));
        let child_depth = state
            .db
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT subagent_depth FROM tool_audit WHERE run_id = ?1 AND tool_name = 'system_time_now'",
                    [&accepted.run_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(Into::into)
            })
            .expect("child audit");
        assert_eq!(child_depth, 1);
    }

    #[tokio::test]
    async fn child_run_rejects_a_declared_write_lock_before_calling_the_model() {
        let directory = tempfile::tempdir().expect("temporary app directory");
        let state = Arc::new(AppState::new(directory.path().join("data")).expect("state"));
        let mut start = request();
        start.web_enabled = false;
        start.turn.message = "请委派子任务。".to_string();
        let accepted = RunIntake::start(&state.db, start).expect("accepted");
        let context = RunContextAssembler::assemble(
            &state.db,
            None,
            &accepted.session.session_key,
            &accepted.run_id,
        )
        .expect("context");
        let sink = RecordingSink::default();
        let preparing = RunEngine::mark_preparing_with_sink(
            &state.db,
            &accepted.session,
            &accepted.run_id,
            &sink,
        )
        .expect("preparing");
        AgentRunRepository::append_event(
            &state.db,
            AppendRunEventInput {
                run_id: accepted.run_id.clone(),
                state_version: preparing,
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state: RunState::Running,
                    stage: "测试 ChildRun".to_string(),
                },
            },
        )
        .expect("running");
        let provider = ScriptedChildProvider {
            responses: Mutex::new(VecDeque::new()),
            tool_surfaces: Mutex::new(Vec::new()),
        };
        let executor = NormalRunToolExecutor::new(
            &state,
            None,
            &accepted,
            &context,
            vec![
                CapabilityId::new("runtime.read"),
                CapabilityId::new("harness.child_run"),
            ],
            &sink,
            Vec::new(),
        )
        .with_child_run_provider(&provider);
        let result = executor
            .execute(
                &accepted.run_id,
                &ToolCall::new(
                    "parent-spawn-write-lock",
                    "spawn_subagent",
                    r#"{"task":"写入","resource_locks":[{"resource_id":"note.md","access":"write"}]}"#,
                ),
                1,
            )
            .await
            .expect("bounded child result");

        assert!(!result.success);
        assert_eq!(
            result.output["subagent_report"]["errors"][0],
            "child_run_write_lock_forbidden"
        );
        assert!(provider.tool_surfaces.lock().expect("surfaces").is_empty());
    }

    fn capability_degraded_count(events: &[serde_json::Value]) -> usize {
        events
            .iter()
            .filter(|event| event["type"] == "capability_degraded")
            .count()
    }

    #[test]
    fn mcp_failover_events_describe_only_provider_route_metadata() {
        let snapshots = vec![
            crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary {
                id: "primary".into(),
                kind: "mcp".into(),
                transport_kind: "https".into(),
                provider_config_hash: "config-a".into(),
                web_search_mapping_json: Some(r#"{"tool":"primary_search"}"#.into()),
                web_fetch_mapping_json: None,
            },
            crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary {
                id: "backup".into(),
                kind: "mcp".into(),
                transport_kind: "https".into(),
                provider_config_hash: "config-b".into(),
                web_search_mapping_json: Some(r#"{"tool":"backup_search"}"#.into()),
                web_fetch_mapping_json: None,
            },
        ];

        let events = mcp_failover_events(&snapshots, "backup");

        assert!(matches!(
            events.as_slice(),
            [McpFailoverEvent { from_provider_id, provider_id, model_id, reason_code, attempt }]
                if from_provider_id == "primary"
                    && provider_id == "backup"
                    && model_id == "backup_search"
                    && reason_code == "mcp_provider_failed"
                    && *attempt == 2
        ));
        assert!(mcp_failover_events(&snapshots, "primary").is_empty());
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
    fn strict_web_search_retries_an_oversize_provider_with_two_results() {
        assert_eq!(web_search_result_limit(8, 1), 5);
        assert_eq!(web_search_result_limit(8, 2), 2);
        assert_eq!(web_search_result_limit(1, 1), 1);
        assert_eq!(web_search_result_limit(1, 2), 1);
    }

    #[test]
    fn high_risk_web_facts_require_official_or_two_independent_domains() {
        assert!(corroborated_source_threshold_met(true, 1));
        assert!(corroborated_source_threshold_met(false, 2));
        assert!(!corroborated_source_threshold_met(false, 1));
    }

    #[test]
    fn deferred_web_degradation_emits_once_when_failed_without_evidence() {
        let db = Database::open_in_memory().expect("database");
        let accepted = RunIntake::start(&db, request()).expect("accepted");
        let sink = RecordingSink::default();
        let mut emitted = false;

        let emitted_now = emit_deferred_web_degradation(
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
        assert!(emitted_now);

        let events = sink.events.lock().expect("events");
        assert_eq!(capability_degraded_count(&events), 1);
        assert_eq!(events[0]["type"], "capability_degraded");
        assert_eq!(
            events[0]["payload"]["code"],
            SafeRunErrorCode::WebProviderTimeout.as_str()
        );

        let second = emit_deferred_web_degradation(
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
        assert!(!second);
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

    #[tokio::test]
    async fn cached_skill_plan_reaches_dispatch_scope_guard() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let vault = directory.path().join("vault");
        std::fs::create_dir_all(&vault).expect("vault directory");
        std::fs::write(vault.join("allowed.md"), "allowed").expect("allowed note");
        std::fs::write(vault.join("blocked.md"), "blocked").expect("blocked note");
        let state = AppState::new(directory.path().join("data")).expect("application state");
        state.set_vault(vault.clone()).expect("activate vault");
        let accepted = RunIntake::start(&state.db, request()).expect("accepted run");
        let context = RunContextAssembler::assemble(
            &state.db,
            Some(&vault),
            &accepted.session.session_key,
            &accepted.run_id,
        )
        .expect("run context");
        let plan = SkillActivationPlanSummary {
            activated_skills: vec![SkillActivationItemSummary {
                name: "bounded-skill".into(),
                scope: "Vault".into(),
                scope_rules: vec![SkillScopeRule {
                    kind: "path".into(),
                    pattern: "allowed.md".into(),
                }],
                score: 1.0,
                match_reason: "test".into(),
                injected_sections: vec!["skill_overlay".into()],
                degraded_reasons: vec![],
                requested_tools: vec![],
                confirmation_required_tools: vec![],
                blocked_capabilities: vec![],
            }],
            requested_tools: vec![],
            confirmation_required_tools: vec![],
            blocked_capabilities: vec![],
            skill_overlay_summary: "one skill".into(),
            degraded: false,
        };
        let sink = RecordingSink::default();
        let executor = NormalRunToolExecutor::new(
            &state,
            None,
            &accepted,
            &context,
            vec![CapabilityId::new("note.read")],
            &sink,
            Vec::new(),
        )
        .with_skill_activation_plan(Some(plan));

        let result = executor
            .dispatch_non_web_tool("read_note", &serde_json::json!({"path": "blocked.md"}))
            .await;
        assert!(!result.success);
        assert!(result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("outside the confirmed Skill scope"));
    }
}
