use super::*;

const MAX_FINAL_OUTPUT_CHARS: usize = 32_000;

#[derive(Debug, Clone, Copy)]
pub(super) enum RunFinalizationStage {
    StreamFlush,
    WebDegradation,
    EvidenceValidation,
    FinalOutputValidation,
    SqliteFinalize,
    EventDelivery,
}

impl RunFinalizationStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::StreamFlush => "stream_flush",
            Self::WebDegradation => "web_degradation",
            Self::EvidenceValidation => "evidence_validation",
            Self::FinalOutputValidation => "final_output_validation",
            Self::SqliteFinalize => "sqlite_finalize",
            Self::EventDelivery => "event_delivery",
        }
    }
}

/// Prepend an inline disclosure blockquote when Web search degraded without usable evidence.
/// The `capability_degraded` event is still emitted separately for diagnostics/eval; this
/// function only rewrites the persisted answer body so the user sees the notice inline
/// instead of a separate banner.
pub(super) fn apply_required_web_degradation_notice(
    _db: &Database,
    _session: &AssistantSessionRef,
    _run_id: &str,
    content: &mut String,
    web_degraded: bool,
) -> AppResult<()> {
    if !web_degraded || content.trim().is_empty() {
        return Ok(());
    }
    *content = format!("> 联网搜索未取得结果，以下为离线回答。\n\n{content}");
    Ok(())
}

pub(super) fn linkify_final_web_citations(
    db: &Database,
    evidence_ids: &[i64],
    content: String,
) -> String {
    match AgentEvidenceRepository::list_web_citation_links(db, evidence_ids) {
        Ok(cites) if !cites.is_empty() => linkify_web_citations(&content, &cites),
        Ok(_) => content,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "web citation linkify skipped after evidence lookup failure"
            );
            content
        }
    }
}

/// Reject model-authored Web links that are not registered by this exact Run.
///
/// The UI citation map is derived from the ledger, but a model can still emit
/// arbitrary Markdown links in its prose. A strict factual Run must not finish
/// with a URL that was not supplied by its own `web_search` evidence.
pub(super) fn validate_current_run_citation_links(
    db: &Database,
    evidence_ids: &[i64],
    content: &str,
) -> AppResult<()> {
    let allowed_urls = AgentEvidenceRepository::list_web_citation_links(db, evidence_ids)?
        .into_iter()
        .map(|citation| citation.url)
        .collect::<HashSet<_>>();
    if allowed_urls.is_empty() {
        return Err(AppError::run(SafeRunErrorCode::WebEvidenceRequired));
    }
    validate_web_urls_against_allowed(content, &allowed_urls)
}

pub(super) fn validate_web_urls_against_allowed(
    content: &str,
    allowed_urls: &HashSet<String>,
) -> AppResult<()> {
    if content.contains("http://") {
        return Err(AppError::run(SafeRunErrorCode::UnverifiedWebCitation));
    }
    let mut remainder = content;
    while let Some(offset) = remainder.find("https://") {
        let candidate = &remainder[offset..];
        let end = candidate
            .find(|character: char| {
                character.is_whitespace() || matches!(character, ')' | ']' | '>')
            })
            .unwrap_or(candidate.len());
        let url = candidate[..end].trim_end_matches(['.', ',', ';', ':']);
        if !allowed_urls.contains(url) {
            return Err(AppError::run(SafeRunErrorCode::UnverifiedWebCitation));
        }
        remainder = &candidate[end..];
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn direct_user_message(content: &str) -> crate::ai_runtime::LlmMessage {
    crate::ai_runtime::LlmMessage {
        role: crate::ai_runtime::MessageRole::User,
        content: crate::ai_types::MessageContent::Text(content.to_string()),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }
}

pub(super) fn validated_final_model_answer(
    content: &str,
) -> Result<String, RunFinalizationFailure> {
    let normalized = crate::ai_runtime::text_support::normalize_model_visible_text(
        &crate::ai_runtime::text_support::sanitize_meta_analysis_prefix(content),
    );
    if normalized.trim().is_empty() {
        return Err(RunFinalizationFailure::new(
            RunFinalizationStage::FinalOutputValidation,
            SafeRunErrorCode::EmptyOutput,
            "empty visible model output",
        ));
    }
    if normalized.chars().count() > MAX_FINAL_OUTPUT_CHARS {
        return Err(RunFinalizationFailure::new(
            RunFinalizationStage::FinalOutputValidation,
            SafeRunErrorCode::OutputTooLong,
            "final model output exceeded bounded character limit",
        ));
    }
    Ok(normalized)
}

pub(super) fn validated_final_model_answer_with_telemetry(
    content: &str,
    finish_reason: Option<&str>,
    requires_factual_completion: bool,
    telemetry: Option<&crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap>,
) -> Result<String, RunFinalizationFailure> {
    let result = match finish_reason {
        Some(reason) if !crate::ai_runtime::final_answer_integrity::FinalAnswerIntegrity::has_normal_finish_reason(reason) => Err(RunFinalizationFailure::new(
            RunFinalizationStage::FinalOutputValidation,
            SafeRunErrorCode::IncompleteOutput,
            "provider did not report a normal final finish reason",
        )),
        _ if !crate::ai_runtime::final_answer_integrity::FinalAnswerIntegrity::has_complete_visible_answer(
            content,
            requires_factual_completion,
        ) => Err(RunFinalizationFailure::new(
            RunFinalizationStage::FinalOutputValidation,
            SafeRunErrorCode::IncompleteOutput,
            "visible model output was only a title",
        )),
        _ => validated_final_model_answer(content),
    };
    if let Some(telemetry) = telemetry {
        match &result {
            Ok(_) => telemetry.record_final_output_validation(true, false),
            Err(failure) => telemetry.record_final_output_validation(
                false,
                failure.code == SafeRunErrorCode::OutputTooLong,
            ),
        }
    }
    result
}

pub(super) fn log_finalization_failure(
    run_id: &str,
    stage: RunFinalizationStage,
    code: SafeRunErrorCode,
) {
    tracing::warn!(
        run_id = %run_id,
        stage = stage.as_str(),
        safe_code = code.as_str(),
        "Agent Run finalization stage failed"
    );
}

pub(super) fn fail_finalization_with_sink(
    db: &Database,
    run_id: &str,
    running_state_version: u64,
    sink: &impl RunEventSink,
    failure: RunFinalizationFailure,
) -> AppResult<()> {
    log_finalization_failure(run_id, failure.stage, failure.code);
    let _internal_reason = &failure.internal_reason;
    let append = AgentRunRepository::append_event(
        db,
        AppendRunEventInput {
            run_id: run_id.to_string(),
            state_version: running_state_version,
            event_type: RunEventType::Failed,
            payload: RunEventPayload::Failed {
                code: failure.code,
                message: safe_failure_message(failure.code).to_string(),
            },
        },
    );
    match append {
        Ok(failed) => {
            emit_durable_event_best_effort(sink, &failed);
            Err(AppError::run(failure.code))
        }
        Err(_) => {
            let code = SafeRunErrorCode::PersistenceFailed;
            log_finalization_failure(run_id, RunFinalizationStage::SqliteFinalize, code);
            let seq = AgentRunRepository::get(db, run_id)
                .ok()
                .flatten()
                .map_or(1, |response| response.events.len() as u64 + 1);
            if let Ok(event) = crate::ai_runtime::run_contract::AssistantRunEvent::new(
                run_id,
                seq,
                running_state_version.saturating_add(1),
                RunEventType::Failed,
                chrono::Utc::now().to_rfc3339(),
                RunEventPayload::Failed {
                    code,
                    message: safe_failure_message(code).to_string(),
                },
            ) {
                let _ = sink.emit_ephemeral_failure(&event);
            }
            Err(AppError::run(code))
        }
    }
}

pub(super) fn validate_final_evidence_or_fail(
    db: &Database,
    run_id: &str,
    state_version: u64,
    evidence_ids: &[i64],
    sink: &impl RunEventSink,
) -> AppResult<()> {
    AgentRunRepository::validate_final_evidence(db, run_id, evidence_ids).map_err(|error| {
        fail_finalization_with_sink(
            db,
            run_id,
            state_version,
            sink,
            RunFinalizationFailure::new(
                RunFinalizationStage::EvidenceValidation,
                SafeRunErrorCode::EvidenceInvalid,
                error.to_string(),
            ),
        )
        .expect_err("finalization failure helper always returns an error")
    })
}

/// Validate a structured terminal submission before it becomes visible or
/// durable. Invalid attribution remains a safe terminal failure.
pub(super) fn validated_current_run_final_submission(
    db: &Database,
    run_id: &str,
    submission: &crate::ai_runtime::final_answer_submission::FinalAnswerSubmission,
    strict_web: bool,
) -> Result<crate::ai_runtime::provenance::ValidatedFinalAnswerSubmission, RunFinalizationFailure> {
    let policy =
        AgentEvidenceRepository::provenance_policy(db, run_id, strict_web).map_err(|error| {
            RunFinalizationFailure::new(
                RunFinalizationStage::EvidenceValidation,
                SafeRunErrorCode::EvidenceInvalid,
                error.to_string(),
            )
        })?;
    crate::ai_runtime::provenance::validate_final_answer_submission(submission, &policy).map_err(
        |error| {
            RunFinalizationFailure::new(
                RunFinalizationStage::EvidenceValidation,
                SafeRunErrorCode::FinalizationProtocolInvalid,
                error.to_string(),
            )
        },
    )
}

pub(super) fn flush_validated_stream_or_fail(
    db: &Database,
    run_id: &str,
    state_version: u64,
    observer: &mut AgentRunStreamObserver<'_>,
    sink: &impl RunEventSink,
) -> AppResult<()> {
    observer.flush_without_terminal().map_err(|error| {
        let code = if error.to_string().contains("delivery") || error.to_string().contains("emit") {
            SafeRunErrorCode::EventDeliveryFailed
        } else {
            SafeRunErrorCode::PersistenceFailed
        };
        fail_finalization_with_sink(
            db,
            run_id,
            state_version,
            sink,
            RunFinalizationFailure::new(RunFinalizationStage::StreamFlush, code, error.to_string()),
        )
        .expect_err("finalization failure helper always returns an error")
    })
}

/// Shared Direct/ToolLoop terminal contract:
/// validated deltas → durable message/`completed` → AnswerComplete → clear abort handle.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_run_terminal(
    db: &Database,
    session: &AssistantSessionRef,
    run_id: &str,
    state_version: u64,
    content: String,
    evidence_ids: Vec<i64>,
    citation_binding: Option<CitationBinding>,
    source_summary: Option<&crate::ai_runtime::provenance::SourceSummary>,
    attribution: Option<&[crate::ai_runtime::provenance::BlockAttribution]>,
    sink: &impl RunEventSink,
) -> AppResult<()> {
    let effective_source_summary = match source_summary {
        Some(summary) => Some(summary.clone()),
        None => {
            match AgentEvidenceRepository::source_summary_for_current_run(db, run_id, &evidence_ids)
            {
                Ok(summary) if !summary.is_empty() => Some(summary),
                Ok(_) => None,
                Err(error) => {
                    tracing::warn!(
                        run_id = %run_id,
                        error = %error,
                        "current Run source summary skipped after evidence lookup failure"
                    );
                    None
                }
            }
        }
    };
    let citation_map =
        match AgentEvidenceRepository::list_current_run_web_citation_links(db, run_id) {
            Ok(cites) if !cites.is_empty() => {
                crate::ai_runtime::citation_linkify::web_citation_map_json(
                    &cites,
                    citation_binding.as_ref(),
                    effective_source_summary.as_ref(),
                    attribution,
                )
            }
            Ok(_) => match AgentEvidenceRepository::list_web_citation_links(db, &evidence_ids) {
                Ok(cites) => crate::ai_runtime::citation_linkify::web_citation_map_json(
                    &cites,
                    citation_binding.as_ref(),
                    effective_source_summary.as_ref(),
                    attribution,
                ),
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "web citation map skipped after evidence lookup failure"
                    );
                    crate::ai_runtime::citation_linkify::web_citation_map_json(
                        &[],
                        citation_binding.as_ref(),
                        effective_source_summary.as_ref(),
                        attribution,
                    )
                }
            },
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "current Run citation map skipped after evidence lookup failure"
                );
                crate::ai_runtime::citation_linkify::web_citation_map_json(
                    &[],
                    citation_binding.as_ref(),
                    effective_source_summary.as_ref(),
                    attribution,
                )
            }
        };
    if let Err(error) = AgentRunRepository::finalize(
        db,
        FinalizeRunInput {
            run_id: run_id.to_string(),
            state_version,
            content,
            evidence_ids,
            citation_map,
            source_summary: effective_source_summary
                .as_ref()
                .map(crate::ai_runtime::provenance::SourceSummary::entries)
                .unwrap_or_default(),
        },
    ) {
        return fail_finalization_with_sink(
            db,
            run_id,
            state_version,
            sink,
            RunFinalizationFailure::new(
                RunFinalizationStage::SqliteFinalize,
                SafeRunErrorCode::PersistenceFailed,
                error.to_string(),
            ),
        );
    }
    match NormalSessionRepository::get(db, &session.session_key) {
        Ok(Some(normal_session)) => {
            if ConversationMemory::refresh_for_session(
                db,
                normal_session.session_id,
                Default::default(),
            )
            .is_err()
            {
                tracing::warn!(
                    run_id = %run_id,
                    reason = "conversation_memory_refresh_failed",
                    "conversation memory refresh skipped after completed Run"
                );
            }
        }
        Ok(None) => tracing::warn!(
            run_id = %run_id,
            reason = "conversation_memory_session_missing",
            "conversation memory refresh skipped after completed Run"
        ),
        Err(_) => tracing::warn!(
            run_id = %run_id,
            reason = "conversation_memory_session_lookup_failed",
            "conversation memory refresh skipped after completed Run"
        ),
    }
    let completed = AgentRunRepository::get_for_session(db, &session.session_key, run_id)
        .map_err(|_| AppError::run(SafeRunErrorCode::PersistenceFailed))?
        .and_then(|response| response.events.last().cloned())
        .ok_or_else(|| AppError::run(SafeRunErrorCode::PersistenceFailed))?;
    emit_durable_event_best_effort(sink, &completed);
    // Terminal presentation delivery is best-effort: it is a live UI
    // projection of an already-durable Completed fact, so a failed emit must
    // never turn a successfully persisted Run into an error.
    let _ = sink.emit_terminal_presentation(run_id);
    crate::ai_runtime::model_gateway::clear_abort(run_id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_and_emit_with_sink(
    db: &Database,
    session: &AssistantSessionRef,
    run_id: &str,
    state_version: u64,
    content: String,
    evidence_ids: Vec<i64>,
    citation_binding: Option<CitationBinding>,
    source_summary: Option<&crate::ai_runtime::provenance::SourceSummary>,
    attribution: Option<&[crate::ai_runtime::provenance::BlockAttribution]>,
    sink: &impl RunEventSink,
) -> AppResult<()> {
    emit_run_terminal(
        db,
        session,
        run_id,
        state_version,
        content,
        evidence_ids,
        citation_binding,
        source_summary,
        attribution,
        sink,
    )
}

pub(super) fn safe_failure_message(code: SafeRunErrorCode) -> &'static str {
    match code {
        SafeRunErrorCode::ProviderUnavailable => "模型服务暂时不可用，请稍后重试",
        SafeRunErrorCode::ProviderTimeout => "模型服务响应超时，请稍后重试",
        SafeRunErrorCode::NoCapableModel => {
            "没有已启用模型满足当前任务所需能力，请在模型设置中启用兼容模型"
        }
        SafeRunErrorCode::WebProviderUnavailable => {
            "未配置可用的联网证据提供方，请在联网与证据中完成配置"
        }
        SafeRunErrorCode::WebProviderTimeout => "联网证据服务响应超时，请稍后重试",
        SafeRunErrorCode::WebProviderAuthFailed => {
            "联网 API Key 无效，请在联网配置中重新输入原始 Key"
        }
        SafeRunErrorCode::WebProviderFailed => "联网证据服务暂时不可用，请稍后重试",
        SafeRunErrorCode::WebEvidenceInvalid => {
            "联网证据未返回可核验结果，不能给出事实结论，请稍后重试"
        }
        SafeRunErrorCode::WebVerificationRequired => {
            "该请求需要联网核验；当前未开启联网，不能给出事实结论"
        }
        SafeRunErrorCode::InvalidRequest => "请求无法按当前运行能力处理",
        SafeRunErrorCode::ToolLoopLimit => "模型调用工具次数过多，请基于已附资料缩小问题后重试",
        SafeRunErrorCode::EmptyOutput => "模型未生成可用回答，请重试",
        SafeRunErrorCode::OutputTooLong => "模型回答超过本次运行上限，请缩小问题范围后重试",
        SafeRunErrorCode::IncompleteOutput => "回答未完整生成，请重试",
        SafeRunErrorCode::EvidenceInvalid => "回答与所附证据无法安全关联，请重新附带资料后重试",
        SafeRunErrorCode::FinalizationProtocolInvalid => "模型未完成本次答案的来源归因协议，请重试",
        SafeRunErrorCode::EventDeliveryFailed => "回答状态未能送达界面，请重新打开会话查看结果",
        SafeRunErrorCode::InvalidExplicitReference => "引用材料无效，请重新附带后重试",
        SafeRunErrorCode::ExplicitReferenceChanged => "引用材料已发生变化，请重新附带后重试",
        SafeRunErrorCode::InvalidRetrievalScope => "资料范围无效，请重新选择后重试",
        SafeRunErrorCode::LocalReferenceIndexUnavailable => {
            "本地资料索引暂不可用，请完成索引后重试"
        }
        SafeRunErrorCode::PermissionDenied => "当前请求不具备执行权限",
        SafeRunErrorCode::Cancelled => "运行已取消",
        SafeRunErrorCode::ClassifiedContextRequired => "请先明确附带当前打开的涉密文档",
        SafeRunErrorCode::ClassifiedContextExpired => "当前涉密文档上下文已失效，请重新附带",
        SafeRunErrorCode::ClassifiedVaultLocked => "涉密保险库已锁定，请解锁后重试",
        SafeRunErrorCode::SessionNotFound
        | SafeRunErrorCode::RunNotFound
        | SafeRunErrorCode::IllegalTransition
        | SafeRunErrorCode::StateVersionConflict
        | SafeRunErrorCode::ConfirmationExpired
        | SafeRunErrorCode::PersistenceFailed
        | SafeRunErrorCode::InvalidChangePlan
        | SafeRunErrorCode::ContinuationLockFailed
        | SafeRunErrorCode::ControlNotAvailable
        | SafeRunErrorCode::TerminalState
        | SafeRunErrorCode::ClassifiedDomainNotSupported
        | SafeRunErrorCode::EvidenceLockFailed
        | SafeRunErrorCode::InvalidBudgetPolicy
        | SafeRunErrorCode::InvalidEvent
        | SafeRunErrorCode::LocalEvidenceInvalid
        | SafeRunErrorCode::InvalidExplicitAction
        | SafeRunErrorCode::ClassifiedHistoryDisabled
        | SafeRunErrorCode::CheckpointStageConflict
        | SafeRunErrorCode::AcceptedEventMissing
        | SafeRunErrorCode::FinalSubmissionInvalid
        | SafeRunErrorCode::WriteTargetViolation
        | SafeRunErrorCode::InvalidDocumentPolicy
        | SafeRunErrorCode::CheckpointInvalidSchema
        | SafeRunErrorCode::InvalidFinalOutput
        | SafeRunErrorCode::ConfirmationPending
        | SafeRunErrorCode::ConfirmationMissing
        | SafeRunErrorCode::InvalidSubagentLifecycle
        | SafeRunErrorCode::InvalidSubagentBatchReport
        | SafeRunErrorCode::RetryNotAvailable
        | SafeRunErrorCode::IdempotencyConflict
        | SafeRunErrorCode::ActiveRunExists
        | SafeRunErrorCode::UnknownToolCallId
        | SafeRunErrorCode::UnverifiedWebCitation
        | SafeRunErrorCode::WebEvidenceRequired
        | SafeRunErrorCode::GroundedFinalizationUnavailable
        | SafeRunErrorCode::FreshEvidenceInsufficient
        | SafeRunErrorCode::LocationRequired => "运行暂时无法完成，请稍后重试",
    }
}

/// Map transport diagnostics to a small safe public vocabulary. The raw provider
/// error is deliberately neither persisted into the Run event nor shown to the user.
pub(super) fn classify_provider_failure(error: &AppError) -> SafeRunErrorCode {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("agent_run_event_delivery_failed") {
        SafeRunErrorCode::EventDeliveryFailed
    } else if message.contains("first_response_timeout")
        || message.contains("stream_idle_timeout")
        || message.contains("timed out")
        || message.contains("timeout")
        || message.contains("deadline")
    {
        SafeRunErrorCode::ProviderTimeout
    } else {
        SafeRunErrorCode::ProviderUnavailable
    }
}

/// When the user cancelled the live stream, keep any safe visible partial for the
/// next turn and exit without rewriting Cancelled as Failed.
pub(super) fn settle_cancelled_run_with_partial(
    db: &Database,
    session: &AssistantSessionRef,
    run_id: &str,
    observer: &AgentRunStreamObserver<'_>,
    _sink: &impl RunEventSink,
    fallback_content: Option<&str>,
) -> AppResult<bool> {
    let snapshot = AgentRunRepository::get_for_session(db, &session.session_key, run_id)?
        .ok_or_else(|| AppError::run(SafeRunErrorCode::RunNotFound))?;
    if snapshot.run.state != RunState::Cancelled {
        return Ok(false);
    }
    let mut partial = observer.interrupt_visible_content();
    if partial.trim().is_empty() {
        if let Some(fallback) = fallback_content {
            partial = fallback.to_string();
        }
    }
    let _ = AgentRunRepository::persist_interrupted_assistant_message(db, run_id, &partial)?;
    crate::ai_runtime::model_gateway::clear_abort(run_id);
    Ok(true)
}

pub(crate) fn classify_tool_loop_failure(error: &AppError) -> SafeRunErrorCode {
    match error.to_string().as_str() {
        "agent_run_mcp_unavailable" => SafeRunErrorCode::WebProviderUnavailable,
        "agent_run_web_provider_timeout" => SafeRunErrorCode::WebProviderTimeout,
        "agent_run_web_provider_auth_failed" => SafeRunErrorCode::WebProviderAuthFailed,
        "agent_run_web_provider_failed" => SafeRunErrorCode::WebProviderFailed,
        "agent_run_web_evidence_invalid" | "agent_run_web_evidence_required" => {
            SafeRunErrorCode::WebEvidenceInvalid
        }
        "agent_run_tool_loop_limit" => SafeRunErrorCode::ToolLoopLimit,
        "agent_run_output_too_long" => SafeRunErrorCode::OutputTooLong,
        "agent_run_incomplete_output" => SafeRunErrorCode::IncompleteOutput,
        "agent_run_invalid_model_response" => SafeRunErrorCode::InvalidRequest,
        "agent_run_final_submission_required" | "agent_run_final_submission_invalid" => {
            SafeRunErrorCode::FinalizationProtocolInvalid
        }
        error if error.starts_with("agent_run_provenance_") => {
            SafeRunErrorCode::FinalizationProtocolInvalid
        }
        _ => classify_provider_failure(error),
    }
}

pub(super) struct RunFinalizationFailure {
    stage: RunFinalizationStage,
    pub(super) code: SafeRunErrorCode,
    internal_reason: String,
}

impl RunFinalizationFailure {
    pub(super) fn new(
        stage: RunFinalizationStage,
        code: SafeRunErrorCode,
        internal_reason: impl Into<String>,
    ) -> Self {
        Self {
            stage,
            code,
            internal_reason: internal_reason.into(),
        }
    }
}

#[cfg(test)]
mod apply_notice_tests {
    use std::collections::HashSet;

    use super::{
        apply_required_web_degradation_notice, classify_tool_loop_failure,
        validate_web_urls_against_allowed, validated_final_model_answer,
        validated_final_model_answer_with_telemetry,
    };
    use crate::ai_runtime::run_contract::AssistantSessionRef;
    use crate::ai_runtime::run_contract::SafeRunErrorCode;
    use crate::error::AppError;
    use crate::storage::db::Database;

    fn dummy_session() -> AssistantSessionRef {
        use crate::ai_runtime::run_contract::SecurityDomain;
        AssistantSessionRef {
            domain: SecurityDomain::Normal,
            session_key: "test".to_string(),
        }
    }

    #[test]
    fn prepends_notice_when_web_degraded_and_content_nonempty() {
        let db = Database::open_in_memory().expect("database");
        let session = dummy_session();
        let mut content = "这是模型回答。".to_string();
        apply_required_web_degradation_notice(&db, &session, "run-1", &mut content, true)
            .expect("notice apply");
        assert!(
            content.starts_with("> 联网搜索未取得结果，以下为离线回答。"),
            "content should start with notice blockquote, got: {content}"
        );
        assert!(content.contains("这是模型回答。"));
    }

    #[test]
    fn does_not_inject_when_not_degraded() {
        let db = Database::open_in_memory().expect("database");
        let session = dummy_session();
        let mut content = "这是模型回答。".to_string();
        apply_required_web_degradation_notice(&db, &session, "run-1", &mut content, false)
            .expect("notice apply");
        assert_eq!(content, "这是模型回答。");
    }

    #[test]
    fn does_not_inject_when_content_empty() {
        let db = Database::open_in_memory().expect("database");
        let session = dummy_session();
        let mut content = "   ".to_string();
        apply_required_web_degradation_notice(&db, &session, "run-1", &mut content, true)
            .expect("notice apply");
        assert_eq!(content, "   ");
    }

    #[test]
    fn strict_web_answer_rejects_a_model_invented_source_url() {
        let allowed = HashSet::from(["https://official.example/result".to_string()]);
        let error = validate_web_urls_against_allowed(
            "See [invented source](https://invented.example/result).",
            &allowed,
        )
        .expect_err("invented source must fail finalization");
        assert_eq!(error.to_string(), "agent_run_unverified_web_citation");
    }

    #[test]
    fn strict_web_answer_accepts_only_its_registered_source_url() {
        let allowed = HashSet::from(["https://official.example/result".to_string()]);
        validate_web_urls_against_allowed(
            "See [official source](https://official.example/result).",
            &allowed,
        )
        .expect("registered source must be accepted");
    }

    #[test]
    fn provenance_protocol_errors_use_the_model_protocol_safe_error() {
        let code =
            classify_tool_loop_failure(&AppError::msg("agent_run_provenance_reference_invalid"));

        assert_eq!(code, SafeRunErrorCode::FinalizationProtocolInvalid);
    }

    #[test]
    fn rejects_a_title_only_final_answer() {
        assert!(
            validated_final_model_answer_with_telemetry(
                "特朗普 最新新闻 2026年8月",
                Some("stop"),
                true,
                None,
            )
            .is_err(),
            "a title without an answer must not complete a Run"
        );
    }

    #[test]
    fn shared_terminal_validation_strips_source_appendices_before_strict_binding() {
        let content = match validated_final_model_answer(
            "结论已经给出。\n\n## Sources\n1. https://example.test/one\n2. [Two](https://example.test/two)",
        ) {
            Ok(content) => content,
            Err(error) => panic!("unexpected finalization failure: {}", error.code.as_str()),
        };

        assert_eq!(content, "结论已经给出。");
    }

    #[test]
    fn shared_terminal_validation_rejects_a_source_appendix_without_an_answer() {
        let error = validated_final_model_answer("## References\n- https://example.test/one")
            .expect_err("a source appendix is not an answer");

        assert_eq!(error.code, SafeRunErrorCode::EmptyOutput);
    }
}
