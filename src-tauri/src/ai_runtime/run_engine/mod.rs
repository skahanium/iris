//! Minimal scene-free direct-answer Run Engine.

mod finalization;
mod observer;
mod providers;
mod recovery;

pub(crate) use finalization::classify_tool_loop_failure;
use finalization::*;
#[cfg(test)]
use observer::NoopRunEventSink;
pub(crate) use observer::*;
pub(crate) use providers::*;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use rusqlite::OptionalExtension;
use tauri::{AppHandle, Emitter, Runtime};

use crate::ai_runtime::agent_evidence_repository::AgentEvidenceRepository;
use crate::ai_runtime::agent_run_repository::{
    AgentRunRepository, AppendRunCheckpointInput, AppendRunEventInput, DurableApplyCheckpoint,
    DurableApplyCheckpointStage, FinalizeRunInput,
};
use crate::ai_runtime::agent_tool_loop::{
    is_evidence_limited_response, resolved_turn_usage, AgentModelTurnBudget, AgentToolLoop,
    ToolLoopExecutor, ToolLoopProvider, EVIDENCE_LIMITED_RESPONSE,
};
use crate::ai_runtime::citation_linkify::{
    bind_strict_current_run_citations, linkify_web_citations,
};
use crate::ai_runtime::conversation_memory::ConversationMemory;
use crate::ai_runtime::direct_provider_route::DirectProviderRoute;
use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
use crate::ai_runtime::run_contract::{
    AssistantSessionRef, Effect, Effort, PresentationProcessKind, PresentationProcessStatus,
    RunEventPayload, RunEventType, RunPresentationEvent, RunPresentationPayload, RunRecoveryKind,
    RunStageCode, RunState, SafeRunErrorCode,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

/// Owns the normal-domain Run lifecycle without legacy Harness state.
pub(crate) struct RunEngine;

impl RunEngine {
    /// Persist a policy denial before any Provider, credential, Web, or tool dispatch.
    ///
    /// A denied Run remains fully replayable: the policy event records the safe
    /// reason and the existing pre-dispatch failure path supplies a terminal state.
    pub(crate) fn enforce_policy_before_dispatch_with_sink(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        decision: &crate::ai_runtime::policy_decision_engine::RunPolicyDecision,
        sink: &impl RunEventSink,
    ) -> AppResult<bool> {
        let Some(code) = decision.denial_code else {
            return Ok(true);
        };
        let snapshot = AgentRunRepository::get_for_session(db, &session.session_key, run_id)?
            .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        if snapshot.run.state != RunState::Accepted {
            return Err(AppError::run(SafeRunErrorCode::IllegalTransition));
        }
        let denied = AgentRunRepository::append_event(
            db,
            AppendRunEventInput {
                run_id: run_id.to_string(),
                state_version: snapshot.run.state_version,
                event_type: RunEventType::PermissionDenied,
                payload: RunEventPayload::PermissionDenied {
                    code,
                    message: "当前请求不具备执行权限".into(),
                },
            },
        )?;
        sink.emit(&denied)?;
        Self::fail_before_dispatch_with_sink(db, session, run_id, code, sink)?;
        Ok(false)
    }
    /// Persist a safe terminal failure after acceptance but before provider dispatch.
    ///
    /// Model routing and credential hydration occur after the accepted event so the
    /// UI can observe slow preparation. If either step cannot proceed, this keeps
    /// the Run from being stranded in `Accepted`/`Preparing` without exposing
    /// implementation details or credential errors.
    pub(crate) fn fail_before_dispatch_with_sink(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        code: SafeRunErrorCode,
        sink: &impl RunEventSink,
    ) -> AppResult<()> {
        let snapshot = AgentRunRepository::get_for_session(db, &session.session_key, run_id)?
            .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        let preparing_version = match snapshot.run.state {
            RunState::Preparing => snapshot.run.state_version,
            RunState::Accepted => {
                let preparing = AgentRunRepository::append_event(
                    db,
                    AppendRunEventInput {
                        run_id: run_id.to_string(),
                        state_version: snapshot.run.state_version,
                        event_type: RunEventType::StageChanged,
                        payload: RunEventPayload::StageChanged {
                            state: RunState::Preparing,
                            stage: "正在准备".to_string(),
                            stage_code: Some(RunStageCode::Preparing),
                        },
                    },
                )?;
                sink.emit(&preparing)?;
                preparing.state_version()
            }
            _ => return Err(AppError::run(SafeRunErrorCode::IllegalTransition)),
        };
        let failed = AgentRunRepository::append_event(
            db,
            AppendRunEventInput {
                run_id: run_id.to_string(),
                state_version: preparing_version,
                event_type: RunEventType::Failed,
                payload: RunEventPayload::Failed {
                    code,
                    message: safe_failure_message(code).to_string(),
                },
            },
        )?;
        emit_durable_event_best_effort(sink, &failed);
        Ok(())
    }

    /// Persist the structured strict-Web failure before terminalizing the Run.
    /// This is used when deterministic evidence acquisition completed its own
    /// bounded attempts but could not produce evidence safe for a factual
    /// answer. The UI can therefore offer a real retry instead of presenting a
    /// generic model failure.
    pub(crate) fn mark_preparing_with_sink(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        sink: &impl RunEventSink,
    ) -> AppResult<u64> {
        let snapshot = AgentRunRepository::get_for_session(db, &session.session_key, run_id)?
            .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        if snapshot.run.state == RunState::Preparing {
            return Ok(snapshot.run.state_version);
        }
        if snapshot.run.state != RunState::Accepted {
            return Err(AppError::run(SafeRunErrorCode::IllegalTransition));
        }
        let preparing = AgentRunRepository::append_event(
            db,
            AppendRunEventInput {
                run_id: run_id.to_string(),
                state_version: snapshot.run.state_version,
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state: RunState::Preparing,
                    stage: "正在准备".to_string(),
                    stage_code: Some(RunStageCode::Preparing),
                },
            },
        )?;
        sink.emit(&preparing)?;
        Ok(preparing.state_version())
    }

    /// Ensure a background execution error cannot leave a non-terminal Run behind.
    ///
    /// Provider and policy errors normally terminalize themselves. This guard is
    /// deliberately idempotent and only covers unexpected orchestration exits.
    /// It records a safe persistence failure instead of exposing the underlying
    /// error, which may include provider or user-derived data.
    pub(crate) fn fail_active_with_sink(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        sink: &impl RunEventSink,
    ) -> AppResult<bool> {
        let snapshot = AgentRunRepository::get_for_session(db, &session.session_key, run_id)?
            .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        if snapshot.run.state.is_terminal()
            || matches!(
                snapshot.run.state,
                RunState::AwaitingConfirmation | RunState::AwaitingInput | RunState::Paused
            )
        {
            return Ok(false);
        }
        if snapshot.run.state == RunState::Accepted {
            Self::fail_before_dispatch_with_sink(
                db,
                session,
                run_id,
                SafeRunErrorCode::PersistenceFailed,
                sink,
            )?;
            return Ok(true);
        }
        let failed = AgentRunRepository::append_event(
            db,
            AppendRunEventInput {
                run_id: run_id.to_string(),
                state_version: snapshot.run.state_version,
                event_type: RunEventType::Failed,
                payload: RunEventPayload::Failed {
                    code: SafeRunErrorCode::PersistenceFailed,
                    message: safe_failure_message(SafeRunErrorCode::PersistenceFailed).to_string(),
                },
            },
        )?;
        emit_durable_event_best_effort(sink, &failed);
        Ok(true)
    }

    /// Finish a confirmed change with an exact Host-derived execution report.
    /// A partial set is terminally reported without manufacturing a completed
    /// checkpoint for operations that were deliberately not dispatched.
    pub(crate) fn finalize_confirmed_change_report_with_sink(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        content: &str,
        completed_all_operations: bool,
        sink: &impl RunEventSink,
    ) -> AppResult<()> {
        let snapshot = AgentRunRepository::get_for_session(db, &session.session_key, run_id)?
            .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        if snapshot.run.state != RunState::Running {
            return Err(AppError::run(SafeRunErrorCode::IllegalTransition));
        }
        let checkpoint = AgentRunRepository::latest_durable_apply_checkpoint(db, run_id)?
            .ok_or_else(|| AppError::run(SafeRunErrorCode::CheckpointStageConflict))?;
        if completed_all_operations {
            AgentRunRepository::append_checkpoint_step(
                db,
                crate::ai_runtime::agent_run_repository::AppendRunCheckpointInput {
                    run_id: run_id.to_string(),
                    state_version: snapshot.run.state_version,
                    checkpoint: crate::ai_runtime::agent_run_repository::DurableApplyCheckpoint::new_change_set(
                        checkpoint.confirmation_id(),
                        checkpoint.plan_hash(),
                        crate::ai_runtime::agent_run_repository::DurableApplyCheckpointStage::Completed,
                        checkpoint.base_content_hashes().to_vec(),
                        checkpoint.expected_post_content_hashes().to_vec(),
                        checkpoint.operation_count(),
                        checkpoint.operation_count(),
                        Vec::new(),
                    )?,
                },
            )?;
        }
        AgentRunRepository::finalize(
            db,
            FinalizeRunInput {
                run_id: run_id.to_string(),
                state_version: snapshot.run.state_version,
                content: content.to_string(),
                evidence_ids: Vec::new(),
                citation_map: serde_json::json!({}),
                source_summary: Vec::new(),
            },
        )?;
        let completed = AgentRunRepository::get_for_session(db, &session.session_key, run_id)?
            .and_then(|response| response.events.last().cloned())
            .ok_or_else(|| AppError::msg("agent_run_completed_event_missing"))?;
        emit_durable_event_best_effort(sink, &completed);
        crate::ai_runtime::model_gateway::clear_abort(run_id);
        Ok(())
    }

    /// Drive accepted → preparing → running → completed for one direct answer.
    #[cfg(test)]
    pub(crate) fn execute_direct(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        provider: &impl DirectAnswerProvider,
    ) -> AppResult<()> {
        Self::execute_direct_with_sink(db, session, run_id, provider, &NoopRunEventSink)
    }

    /// Drive a direct Run and emit each event only after its durable write succeeds.
    #[cfg(test)]
    pub(crate) fn execute_direct_with_sink(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        provider: &impl DirectAnswerProvider,
        sink: &impl RunEventSink,
    ) -> AppResult<()> {
        let snapshot = AgentRunRepository::get_for_session(db, &session.session_key, run_id)?
            .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        if snapshot.run.state.is_terminal() {
            if snapshot.run.state == RunState::Cancelled {
                crate::ai_runtime::model_gateway::clear_abort(run_id);
            }
            return Err(AppError::run(SafeRunErrorCode::TerminalState));
        }
        if snapshot.run.state != RunState::Accepted {
            return Err(AppError::run(SafeRunErrorCode::IllegalTransition));
        }
        let message = user_message_for_run(db, &session.session_key, run_id)?;
        let preparing = AgentRunRepository::append_event(
            db,
            AppendRunEventInput {
                run_id: run_id.to_string(),
                state_version: snapshot.run.state_version,
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state: RunState::Preparing,
                    stage: "正在准备".to_string(),
                    stage_code: Some(RunStageCode::Preparing),
                },
            },
        )?;
        sink.emit(&preparing)?;
        let running = AgentRunRepository::append_event(
            db,
            AppendRunEventInput {
                run_id: run_id.to_string(),
                state_version: preparing.state_version(),
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state: RunState::Running,
                    stage: "正在生成答复".to_string(),
                    stage_code: Some(RunStageCode::GeneratingAnswer),
                },
            },
        )?;
        sink.emit(&running)?;
        let answer = match provider.answer(run_id, &message) {
            Ok(answer) => answer,
            Err(_) => {
                let failed = AgentRunRepository::append_event(
                    db,
                    AppendRunEventInput {
                        run_id: run_id.to_string(),
                        state_version: running.state_version(),
                        event_type: RunEventType::Failed,
                        payload: RunEventPayload::Failed {
                            code: SafeRunErrorCode::ProviderUnavailable,
                            message: "模型服务暂时不可用，请稍后重试".to_string(),
                        },
                    },
                )?;
                emit_durable_event_best_effort(sink, &failed);
                return Err(AppError::run(SafeRunErrorCode::ProviderUnavailable));
            }
        };
        let answer = match validated_final_model_answer(&answer) {
            Ok(answer) => answer,
            Err(failure) => {
                return fail_finalization_with_sink(
                    db,
                    run_id,
                    running.state_version(),
                    sink,
                    failure,
                );
            }
        };
        finalize_and_emit_with_sink(
            db,
            session,
            run_id,
            running.state_version(),
            answer,
            Vec::new(),
            None,
            None,
            None,
            sink,
        )
    }

    /// Drive a streaming direct answer using the persisted user message only.
    #[cfg(test)]
    pub(crate) async fn execute_direct_streaming_with_sink(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        provider: &impl ToolLoopProvider,
        sink: &impl RunEventSink,
    ) -> AppResult<()> {
        let message = user_message_for_run(db, &session.session_key, run_id)?;
        let messages = [direct_user_message(&message)];
        Self::execute_direct_streaming_with_messages_and_sink(
            db,
            session,
            run_id,
            &messages,
            &[],
            None,
            provider,
            sink,
            None,
        )
        .await
    }

    /// Evaluation-only entry that records the real direct Gateway/stream/finalization path.
    #[cfg(test)]
    pub(crate) async fn execute_direct_streaming_with_eval_telemetry(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        provider: &impl ToolLoopProvider,
        sink: &impl RunEventSink,
        telemetry: &crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap,
    ) -> AppResult<()> {
        let message = user_message_for_run(db, &session.session_key, run_id)?;
        let messages = [direct_user_message(&message)];
        Self::execute_direct_streaming_with_messages_and_sink(
            db,
            session,
            run_id,
            &messages,
            &[],
            None,
            provider,
            sink,
            Some(telemetry),
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn execute_direct_streaming_with_prompt_and_evidence_with_sink(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        prompt: &str,
        evidence_ids: &[i64],
        provider: &impl ToolLoopProvider,
        sink: &impl RunEventSink,
    ) -> AppResult<()> {
        let messages = [direct_user_message(prompt)];
        Self::execute_direct_streaming_with_messages_and_sink(
            db,
            session,
            run_id,
            &messages,
            evidence_ids,
            None,
            provider,
            sink,
            None,
        )
        .await
    }

    /// Drive a streaming Run with multimodal messages and authorized material.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_direct_streaming_with_messages_evidence_and_context_material_plan_with_sink(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        messages: &[crate::ai_runtime::LlmMessage],
        evidence_ids: &[i64],
        material_plan: &crate::ai_runtime::context_materials::ContextMaterialPlan,
        provider: &impl ToolLoopProvider,
        sink: &impl RunEventSink,
    ) -> AppResult<()> {
        Self::execute_direct_streaming_with_messages_and_sink(
            db,
            session,
            run_id,
            messages,
            evidence_ids,
            Some(material_plan),
            provider,
            sink,
            None,
        )
        .await
    }

    /// Evaluation-only direct path with the same messages, evidence, verifier,
    /// Gateway and finalization stages as production.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_direct_streaming_with_messages_evidence_and_context_material_plan_with_eval_telemetry(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        messages: &[crate::ai_runtime::LlmMessage],
        evidence_ids: &[i64],
        material_plan: &crate::ai_runtime::context_materials::ContextMaterialPlan,
        provider: &impl ToolLoopProvider,
        sink: &impl RunEventSink,
        telemetry: &crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap,
    ) -> AppResult<()> {
        Self::execute_direct_streaming_with_messages_and_sink(
            db,
            session,
            run_id,
            messages,
            evidence_ids,
            Some(material_plan),
            provider,
            sink,
            Some(telemetry),
        )
        .await
    }

    /// Drive a bounded model/tool loop through the same persisted Run lifecycle
    /// used by direct answers. Tool dispatch itself is injected so policy,
    /// permission, confirmation and audit ownership remain at the command layer.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_tool_loop_with_sink(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        messages: Vec<crate::ai_runtime::LlmMessage>,
        tools: Vec<crate::ai_runtime::ToolSpec>,
        evidence_ids: &[i64],
        material_plan: Option<&crate::ai_runtime::context_materials::ContextMaterialPlan>,
        provider: &impl ToolLoopProvider,
        executor: &impl ToolLoopExecutor,
        sink: &impl RunEventSink,
    ) -> AppResult<()> {
        Self::execute_tool_loop_with_sink_internal(
            db,
            session,
            run_id,
            messages,
            tools,
            evidence_ids,
            material_plan,
            provider,
            executor,
            sink,
            None,
            None,
            false,
            true,
        )
        .await
    }

    /// Run the single post-confirmation verification loop while preserving the
    /// already-running Durable Apply lifecycle. Callers must expose only the
    /// target-bounded local read surface.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_post_confirmation_verification_with_sink(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        messages: Vec<crate::ai_runtime::LlmMessage>,
        tools: Vec<crate::ai_runtime::ToolSpec>,
        provider: &impl ToolLoopProvider,
        executor: &impl ToolLoopExecutor,
        sink: &impl RunEventSink,
    ) -> AppResult<()> {
        let policy =
            AgentRunRepository::budget_policy_for_session(db, &session.session_key, run_id)?
                .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        Self::execute_tool_loop_with_sink_internal(
            db,
            session,
            run_id,
            messages,
            tools,
            &[],
            None,
            provider,
            executor,
            sink,
            None,
            Some(AgentToolLoop::from_post_confirmation_policy(&policy)),
            true,
            false,
        )
        .await
    }

    /// Evaluation-only tool-loop entry; only observation is added.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_tool_loop_with_eval_telemetry(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        messages: Vec<crate::ai_runtime::LlmMessage>,
        tools: Vec<crate::ai_runtime::ToolSpec>,
        evidence_ids: &[i64],
        material_plan: Option<&crate::ai_runtime::context_materials::ContextMaterialPlan>,
        provider: &impl ToolLoopProvider,
        executor: &impl ToolLoopExecutor,
        sink: &impl RunEventSink,
        telemetry: &crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap,
    ) -> AppResult<()> {
        Self::execute_tool_loop_with_sink_internal(
            db,
            session,
            run_id,
            messages,
            tools,
            evidence_ids,
            material_plan,
            provider,
            executor,
            sink,
            Some(telemetry),
            None,
            false,
            true,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_tool_loop_with_sink_internal(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        messages: Vec<crate::ai_runtime::LlmMessage>,
        tools: Vec<crate::ai_runtime::ToolSpec>,
        evidence_ids: &[i64],
        _material_plan: Option<&crate::ai_runtime::context_materials::ContextMaterialPlan>,
        provider: &impl ToolLoopProvider,
        executor: &impl ToolLoopExecutor,
        sink: &impl RunEventSink,
        telemetry: Option<&crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap>,
        tool_loop_override: Option<AgentToolLoop>,
        resume_running: bool,
        fail_on_error: bool,
    ) -> AppResult<()> {
        let snapshot = AgentRunRepository::get_for_session(db, &session.session_key, run_id)?
            .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        if snapshot.run.state.is_terminal() {
            if snapshot.run.state == RunState::Cancelled {
                crate::ai_runtime::model_gateway::clear_abort(run_id);
            }
            return Err(AppError::run(SafeRunErrorCode::TerminalState));
        }
        let budget_policy =
            AgentRunRepository::budget_policy_for_session(db, &session.session_key, run_id)?
                .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        let preparing_version = match snapshot.run.state {
            RunState::Running if resume_running => snapshot.run.state_version,
            RunState::Preparing => snapshot.run.state_version,
            RunState::Accepted => {
                let preparing = AgentRunRepository::append_event(
                    db,
                    AppendRunEventInput {
                        run_id: run_id.to_string(),
                        state_version: snapshot.run.state_version,
                        event_type: RunEventType::StageChanged,
                        payload: RunEventPayload::StageChanged {
                            state: RunState::Preparing,
                            stage: "正在准备工具执行".to_string(),
                            stage_code: Some(RunStageCode::PreparingTools),
                        },
                    },
                )?;
                sink.emit(&preparing)?;
                preparing.state_version()
            }
            _ => return Err(AppError::run(SafeRunErrorCode::IllegalTransition)),
        };
        let running = if resume_running && snapshot.run.state == RunState::Running {
            None
        } else {
            Some(AgentRunRepository::append_event(
                db,
                AppendRunEventInput {
                    run_id: run_id.to_string(),
                    state_version: preparing_version,
                    event_type: RunEventType::StageChanged,
                    payload: RunEventPayload::StageChanged {
                        state: RunState::Running,
                        stage: "正在调用模型和工具".to_string(),
                        stage_code: Some(RunStageCode::ModelAndTools),
                    },
                },
            )?)
        };
        if let Some(running) = &running {
            sink.emit(running)?;
        }
        let running_state_version = running
            .as_ref()
            .map_or(preparing_version, |event| event.state_version());
        // Tool-call turns may stream provisional text. Keep it private until
        // the loop reaches a final assistant answer so it cannot be duplicated.
        let mut observer = if let Some(telemetry) = telemetry {
            AgentRunStreamObserver::new_with_eval_telemetry(
                db,
                run_id,
                running_state_version,
                sink,
                true,
                telemetry.clone(),
            )
        } else {
            AgentRunStreamObserver::new_with_deferred_deltas(
                db,
                run_id,
                running_state_version,
                sink,
                true,
            )
        };
        let finalization_required = tools.iter().any(|tool| {
            tool.name == crate::ai_runtime::final_answer_submission::FINAL_ANSWER_TOOL_NAME
        });
        if executor.requires_web_evidence()
            || executor.requires_external_evidence()
            || finalization_required
        {
            observer.seal_visible_deltas_until_validated();
        }
        let tool_loop =
            tool_loop_override.unwrap_or_else(|| AgentToolLoop::from_policy(&budget_policy));
        let outcome = if let Some(telemetry) = telemetry {
            tool_loop
                .execute_with_eval_telemetry(
                    provider,
                    executor,
                    run_id,
                    messages,
                    tools,
                    &mut observer,
                    telemetry,
                )
                .await
        } else {
            tool_loop
                .execute(provider, executor, run_id, messages, tools, &mut observer)
                .await
        };
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                if !fail_on_error {
                    return Err(error);
                }
                if settle_cancelled_run_with_partial(db, session, run_id, &observer, sink, None)? {
                    return Ok(());
                }
                if error.to_string() == crate::ai_runtime::run_tool_loop::CONFIRMATION_PENDING_ERROR
                {
                    let current =
                        AgentRunRepository::get_for_session(db, &session.session_key, run_id)?
                            .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
                    if current.run.state == RunState::AwaitingConfirmation {
                        // The executor already committed the immutable plan and its
                        // ConfirmationRequired transition. Do not emit a terminal
                        // failure or make another model turn while user approval is
                        // outstanding.
                        return Ok(());
                    }
                }
                let code = classify_tool_loop_failure(&error);
                let failed = AgentRunRepository::append_event(
                    db,
                    AppendRunEventInput {
                        run_id: run_id.to_string(),
                        state_version: running_state_version,
                        event_type: RunEventType::Failed,
                        payload: RunEventPayload::Failed {
                            code,
                            message: safe_failure_message(code).to_string(),
                        },
                    },
                )?;
                emit_durable_event_best_effort(sink, &failed);
                return Err(AppError::run(code));
            }
        };
        let natural_clarification = outcome.final_submission.is_none()
            && crate::ai_runtime::agent_tool_loop::is_natural_clarification(&outcome.content);
        let structured_final_submission = outcome.final_submission.is_some();
        if settle_cancelled_run_with_partial(
            db,
            session,
            run_id,
            &observer,
            sink,
            Some(outcome.content.as_str()),
        )? {
            return Ok(());
        }
        let web_degraded = executor.emit_deferred_web_degradation_if_needed(db, sink)?;
        if !observer.emitted_generating_answer_stage() {
            if let Err(error) = observer.emit_generating_answer_stage_if_needed() {
                if settle_cancelled_run_with_partial(
                    db,
                    session,
                    run_id,
                    &observer,
                    sink,
                    Some(outcome.content.as_str()),
                )? {
                    return Ok(());
                }
                return Err(error);
            }
        }
        observer.clear_deferred_visible_deltas();
        // `EVIDENCE_LIMITED_RESPONSE` is created by the Host after it has
        // withheld an unsupported model draft. It is a complete, safe normal
        // answer, not provider output that should be subjected to the model
        // finish-reason/integrity recovery path a second time.
        let evidence_limited = is_evidence_limited_response(&outcome.content);
        let mut content = if evidence_limited {
            outcome.content.clone()
        } else {
            match validated_final_model_answer_with_telemetry(
                &outcome.content,
                outcome
                    .final_submission
                    .is_none()
                    .then_some(outcome.finish_reason.as_str()),
                executor.requires_web_evidence() || executor.requires_external_evidence(),
                telemetry,
            ) {
                Ok(content) => content,
                Err(failure) => {
                    return fail_finalization_with_sink(
                        db,
                        run_id,
                        running_state_version,
                        sink,
                        failure,
                    );
                }
            }
        };
        if let Err(error) =
            apply_required_web_degradation_notice(db, session, run_id, &mut content, web_degraded)
        {
            return fail_finalization_with_sink(
                db,
                run_id,
                running_state_version,
                sink,
                RunFinalizationFailure::new(
                    RunFinalizationStage::WebDegradation,
                    SafeRunErrorCode::PersistenceFailed,
                    error.to_string(),
                ),
            );
        }
        let mut final_evidence_ids = evidence_ids.to_vec();
        final_evidence_ids.extend(executor.evidence_ids());
        final_evidence_ids.sort_unstable();
        final_evidence_ids.dedup();
        if !natural_clarification && !is_evidence_limited_response(&content) {
            validate_final_evidence_or_fail(
                db,
                run_id,
                running_state_version,
                &final_evidence_ids,
                sink,
            )?;
        }
        if !natural_clarification
            && !is_evidence_limited_response(&content)
            && executor.requires_external_evidence()
            && !AgentEvidenceRepository::has_current_run_external_evidence(
                db,
                run_id,
                &executor.evidence_ids(),
            )?
        {
            return fail_finalization_with_sink(
                db,
                run_id,
                running_state_version,
                sink,
                RunFinalizationFailure::new(
                    RunFinalizationStage::EvidenceValidation,
                    SafeRunErrorCode::EvidenceInvalid,
                    "agent_run_external_evidence_required",
                ),
            );
        }
        if !is_evidence_limited_response(&content) {
            content = match validated_final_model_answer_with_telemetry(
                &content,
                None,
                executor.requires_web_evidence() || executor.requires_external_evidence(),
                telemetry,
            ) {
                Ok(content) => content,
                Err(failure) => {
                    return fail_finalization_with_sink(
                        db,
                        run_id,
                        running_state_version,
                        sink,
                        failure,
                    );
                }
            };
        }
        let mut citation_binding = None;
        let mut source_summary = None;
        let mut attribution = None;
        let structured_evidence_ids = if !is_evidence_limited_response(&content) {
            if let Some(submission) = outcome.final_submission.as_ref() {
                let provenance = match validated_current_run_final_submission(
                    db,
                    run_id,
                    submission,
                    executor.requires_web_evidence() || executor.requires_external_evidence(),
                ) {
                    Ok(provenance) => provenance,
                    Err(failure) => {
                        return fail_finalization_with_sink(
                            db,
                            run_id,
                            running_state_version,
                            sink,
                            failure,
                        );
                    }
                };
                let selected = match AgentEvidenceRepository::evidence_ids_for_validated_references(
                    db,
                    run_id,
                    &provenance.accepted_references,
                ) {
                    Ok(selected) => selected,
                    Err(error) => {
                        return fail_finalization_with_sink(
                            db,
                            run_id,
                            running_state_version,
                            sink,
                            RunFinalizationFailure::new(
                                RunFinalizationStage::EvidenceValidation,
                                SafeRunErrorCode::EvidenceInvalid,
                                error.to_string(),
                            ),
                        );
                    }
                };
                content = provenance.visible_content;
                source_summary = Some(provenance.source_summary);
                attribution = Some(provenance.attribution);
                Some(selected)
            } else if finalization_required {
                return fail_finalization_with_sink(
                    db,
                    run_id,
                    running_state_version,
                    sink,
                    RunFinalizationFailure::new(
                        RunFinalizationStage::EvidenceValidation,
                        SafeRunErrorCode::GroundedFinalizationUnavailable,
                        "current-evidence run required a grounded final submission",
                    ),
                );
            } else {
                None
            }
        } else {
            None
        };
        let citation_evidence_ids = structured_evidence_ids.clone().unwrap_or_else(|| {
            if executor.requires_web_evidence() {
                executor.evidence_ids()
            } else {
                final_evidence_ids.clone()
            }
        });
        if executor.requires_web_evidence()
            && !natural_clarification
            && !is_evidence_limited_response(&content)
        {
            if !AgentEvidenceRepository::has_current_run_web_evidence(
                db,
                run_id,
                &citation_evidence_ids,
            )? {
                return fail_finalization_with_sink(
                    db,
                    run_id,
                    running_state_version,
                    sink,
                    RunFinalizationFailure::new(
                        RunFinalizationStage::EvidenceValidation,
                        SafeRunErrorCode::EvidenceInvalid,
                        "agent_run_web_evidence_required",
                    ),
                );
            }
            let citations =
                match AgentEvidenceRepository::list_current_run_web_citation_links(db, run_id) {
                    Ok(citations) => citations,
                    Err(_) => {
                        return fail_finalization_with_sink(
                            db,
                            run_id,
                            running_state_version,
                            sink,
                            RunFinalizationFailure::new(
                                RunFinalizationStage::EvidenceValidation,
                                SafeRunErrorCode::PersistenceFailed,
                                "current_run_citation_load_failed",
                            ),
                        );
                    }
                };
            let outcome = if structured_evidence_ids.is_some() {
                match bind_strict_current_run_citations(&content, &citations) {
                    Ok(outcome) => Some(outcome),
                    Err(error) => {
                        return fail_finalization_with_sink(
                            db,
                            run_id,
                            running_state_version,
                            sink,
                            RunFinalizationFailure::new(
                                RunFinalizationStage::EvidenceValidation,
                                SafeRunErrorCode::EvidenceInvalid,
                                error.to_string(),
                            ),
                        );
                    }
                }
            } else if is_evidence_limited_response(&content) {
                // The ToolLoop used its single repair slot and deliberately
                // withheld an unsupported draft.  This is a normal assistant
                // limitation, not a red internal failure and it must not
                // invent a source-group binding.
                None
            } else {
                // Natural factual answers are admitted only with exact
                // Run-local source markers. Production reaches this branch
                // after the ToolLoop's repair turn; this fallback also keeps
                // direct callers from persisting an unsupported draft.
                match bind_strict_current_run_citations(&content, &citations) {
                    Ok(outcome) => Some(outcome),
                    Err(_) => {
                        content = EVIDENCE_LIMITED_RESPONSE.to_string();
                        None
                    }
                }
            };
            if let Some(outcome) = outcome {
                tracing::info!(
                    run_id = %run_id,
                    binding_mode = ?outcome.binding.mode,
                    fallback_reason = ?outcome.binding.fallback_reason,
                    referenced_count = outcome.binding.referenced_indices.len(),
                    "current Run citation binding resolved"
                );
                content = outcome.content;
                citation_binding = Some(outcome.binding);
            }
        } else {
            content = linkify_final_web_citations(db, &citation_evidence_ids, content);
        }
        if executor.requires_web_evidence()
            && !natural_clarification
            && !is_evidence_limited_response(&content)
        {
            if let Err(error) =
                validate_current_run_citation_links(db, &citation_evidence_ids, &content)
            {
                return fail_finalization_with_sink(
                    db,
                    run_id,
                    running_state_version,
                    sink,
                    RunFinalizationFailure::new(
                        RunFinalizationStage::EvidenceValidation,
                        SafeRunErrorCode::EvidenceInvalid,
                        error.to_string(),
                    ),
                );
            }
        }
        if settle_cancelled_run_with_partial(
            db,
            session,
            run_id,
            &observer,
            sink,
            Some(content.as_str()),
        )? {
            return Ok(());
        }
        observer.bind_validated_content(&content);
        flush_validated_stream_or_fail(db, run_id, running_state_version, &mut observer, sink)?;
        let terminal_evidence_ids = if is_evidence_limited_response(&content) {
            Vec::new()
        } else if let Some(structured_evidence_ids) = structured_evidence_ids {
            structured_evidence_ids
        } else if executor.requires_web_evidence() && !structured_final_submission {
            match citation_binding.as_ref() {
                Some(binding) => AgentEvidenceRepository::current_run_web_evidence_ids_for_indices(
                    db,
                    run_id,
                    &binding.referenced_indices,
                )?,
                None => final_evidence_ids,
            }
        } else {
            final_evidence_ids
        };
        finalize_and_emit_with_sink(
            db,
            session,
            run_id,
            running_state_version,
            content,
            terminal_evidence_ids,
            citation_binding,
            source_summary.as_ref(),
            attribution.as_deref(),
            sink,
        )
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_direct_streaming_with_messages_and_sink(
        db: &Database,
        session: &AssistantSessionRef,
        run_id: &str,
        messages: &[crate::ai_runtime::LlmMessage],
        evidence_ids: &[i64],
        material_plan: Option<&crate::ai_runtime::context_materials::ContextMaterialPlan>,
        provider: &impl ToolLoopProvider,
        sink: &impl RunEventSink,
        telemetry: Option<&crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap>,
    ) -> AppResult<()> {
        let snapshot = AgentRunRepository::get_for_session(db, &session.session_key, run_id)?
            .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        if snapshot.run.state.is_terminal() {
            if snapshot.run.state == RunState::Cancelled {
                crate::ai_runtime::model_gateway::clear_abort(run_id);
            }
            return Err(AppError::run(SafeRunErrorCode::TerminalState));
        }
        let budget_policy =
            AgentRunRepository::budget_policy_for_session(db, &session.session_key, run_id)?
                .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
        let turn_budget = AgentModelTurnBudget {
            max_prompt_tokens: Some(budget_policy.max_prompt_tokens),
            max_completion_tokens: Some(budget_policy.max_completion_tokens),
            max_turn_output_tokens: Some(budget_policy.max_turn_output_tokens),
        };
        let preparing_version = match snapshot.run.state {
            RunState::Preparing => snapshot.run.state_version,
            RunState::Accepted => {
                let analyzing_materials = material_plan.is_some_and(|plan| {
                    !plan.rendered_authorized_material.trim().is_empty()
                        || !plan.rendered_local_retrieval.trim().is_empty()
                });
                let preparing = AgentRunRepository::append_event(
                    db,
                    AppendRunEventInput {
                        run_id: run_id.to_string(),
                        state_version: snapshot.run.state_version,
                        event_type: RunEventType::StageChanged,
                        payload: RunEventPayload::StageChanged {
                            state: RunState::Preparing,
                            stage: if analyzing_materials {
                                "正在分析材料".to_string()
                            } else {
                                "正在准备".to_string()
                            },
                            stage_code: Some(RunStageCode::Preparing),
                        },
                    },
                )?;
                sink.emit(&preparing)?;
                preparing.state_version()
            }
            _ => return Err(AppError::run(SafeRunErrorCode::IllegalTransition)),
        };
        let running = AgentRunRepository::append_event(
            db,
            AppendRunEventInput {
                run_id: run_id.to_string(),
                state_version: preparing_version,
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state: RunState::Running,
                    stage: "正在生成答复".to_string(),
                    stage_code: Some(RunStageCode::GeneratingAnswer),
                },
            },
        )?;
        sink.emit(&running)?;
        let running_state_version = running.state_version();
        // Generic material provenance is a prompt boundary, not a brittle
        // string-based semantic verifier. Stream normal answers normally.
        let defer_visible_deltas = false;
        let mut observer = if let Some(telemetry) = telemetry {
            AgentRunStreamObserver::new_with_eval_telemetry(
                db,
                run_id,
                running_state_version,
                sink,
                defer_visible_deltas,
                telemetry.clone(),
            )
        } else {
            AgentRunStreamObserver::new_with_deferred_deltas(
                db,
                run_id,
                running_state_version,
                sink,
                defer_visible_deltas,
            )
        };
        let model_started_at = Instant::now();
        let response = provider
            .answer_turn(run_id, messages, &[], turn_budget, &mut observer)
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                if settle_cancelled_run_with_partial(db, session, run_id, &observer, sink, None)? {
                    return Ok(());
                }
                let code = classify_provider_failure(&error);
                let failed = AgentRunRepository::append_event(
                    db,
                    AppendRunEventInput {
                        run_id: run_id.to_string(),
                        state_version: running_state_version,
                        event_type: RunEventType::Failed,
                        payload: RunEventPayload::Failed {
                            code,
                            message: safe_failure_message(code).to_string(),
                        },
                    },
                )?;
                emit_durable_event_best_effort(sink, &failed);
                return Err(AppError::run(code));
            }
        };
        if let Some(telemetry) = telemetry {
            telemetry.record_model_turn(&response, model_started_at);
        }
        if settle_cancelled_run_with_partial(
            db,
            session,
            run_id,
            &observer,
            sink,
            response.content.as_deref(),
        )? {
            return Ok(());
        }
        let (prompt_tokens, completion_tokens, _) = resolved_turn_usage(&response, messages, &[]);
        if turn_budget
            .max_prompt_tokens
            .is_some_and(|limit| prompt_tokens > limit)
        {
            return fail_finalization_with_sink(
                db,
                run_id,
                running_state_version,
                sink,
                RunFinalizationFailure::new(
                    RunFinalizationStage::FinalOutputValidation,
                    SafeRunErrorCode::InvalidRequest,
                    "direct model turn exceeded frozen prompt budget",
                ),
            );
        }
        if turn_budget
            .max_turn_output_tokens
            .is_some_and(|limit| completion_tokens > limit)
            || turn_budget
                .max_completion_tokens
                .is_some_and(|limit| completion_tokens > limit)
        {
            if let Some(telemetry) = telemetry {
                telemetry.record_final_output_validation(false, true);
            }
            return fail_finalization_with_sink(
                db,
                run_id,
                running_state_version,
                sink,
                RunFinalizationFailure::new(
                    RunFinalizationStage::FinalOutputValidation,
                    SafeRunErrorCode::OutputTooLong,
                    "direct model turn exceeded frozen completion budget",
                ),
            );
        }
        if let Err(error) = observer.flush_transient() {
            return fail_finalization_with_sink(
                db,
                run_id,
                running_state_version,
                sink,
                RunFinalizationFailure::new(
                    RunFinalizationStage::EventDelivery,
                    SafeRunErrorCode::EventDeliveryFailed,
                    error.to_string(),
                ),
            );
        }
        if !response.tool_calls.is_empty() {
            let failed = AgentRunRepository::append_event(
                db,
                AppendRunEventInput {
                    run_id: run_id.to_string(),
                    state_version: running_state_version,
                    event_type: RunEventType::Failed,
                    payload: RunEventPayload::Failed {
                        code: SafeRunErrorCode::InvalidRequest,
                        message: "当前直答运行不支持工具调用".to_string(),
                    },
                },
            )?;
            emit_durable_event_best_effort(sink, &failed);
            return Err(AppError::msg("agent_run_direct_response_invalid"));
        }
        let mut content = match validated_final_model_answer_with_telemetry(
            response.content.as_deref().unwrap_or_default(),
            Some(response.finish_reason.as_str()),
            false,
            telemetry,
        ) {
            Ok(content) => content,
            Err(failure) => {
                return fail_finalization_with_sink(
                    db,
                    run_id,
                    running_state_version,
                    sink,
                    failure,
                );
            }
        };
        if let Err(error) =
            apply_required_web_degradation_notice(db, session, run_id, &mut content, false)
        {
            return fail_finalization_with_sink(
                db,
                run_id,
                running_state_version,
                sink,
                RunFinalizationFailure::new(
                    RunFinalizationStage::WebDegradation,
                    SafeRunErrorCode::PersistenceFailed,
                    error.to_string(),
                ),
            );
        }
        validate_final_evidence_or_fail(db, run_id, running_state_version, evidence_ids, sink)?;
        content =
            match validated_final_model_answer_with_telemetry(&content, None, false, telemetry) {
                Ok(content) => content,
                Err(failure) => {
                    return fail_finalization_with_sink(
                        db,
                        run_id,
                        running_state_version,
                        sink,
                        failure,
                    );
                }
            };
        let citation_binding =
            match AgentEvidenceRepository::list_current_run_web_citation_links(db, run_id) {
                Ok(cites) if !cites.is_empty() => {
                    let outcome = crate::ai_runtime::citation_linkify::bind_current_run_citations(
                        &content, &cites,
                    );
                    content = outcome.content;
                    Some(outcome.binding)
                }
                _ => {
                    content = linkify_final_web_citations(db, evidence_ids, content);
                    None
                }
            };
        if settle_cancelled_run_with_partial(
            db,
            session,
            run_id,
            &observer,
            sink,
            Some(content.as_str()),
        )? {
            return Ok(());
        }
        observer.bind_validated_content(&content);
        flush_validated_stream_or_fail(db, run_id, running_state_version, &mut observer, sink)?;
        finalize_and_emit_with_sink(
            db,
            session,
            run_id,
            running_state_version,
            content,
            evidence_ids.to_vec(),
            citation_binding,
            None,
            None,
            sink,
        )
    }
}
