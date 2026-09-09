use super::*;

/// Single channel for persisted, replayable Run events.
pub(crate) trait RunEventSink: Send + Sync {
    /// Emit only an event that has already been committed to the Repository.
    fn emit(&self, event: &crate::ai_runtime::run_contract::AssistantRunEvent) -> AppResult<()>;

    /// Emit one strictly ordered, non-persisted visual event. Delivery failure
    /// must never invalidate the durable Run result.
    fn emit_presentation(&self, _run_id: &str, _payload: RunPresentationPayload) -> AppResult<()> {
        Ok(())
    }

    /// Emit the terminal presentation only after the durable Completed event.
    /// Tauri derives this projection from that durable event itself.
    fn emit_terminal_presentation(&self, run_id: &str) -> AppResult<()> {
        self.emit_presentation(run_id, RunPresentationPayload::AnswerComplete)
    }

    /// Emit a safe terminal event when SQLite itself cannot record that event.
    fn emit_ephemeral_failure(
        &self,
        event: &crate::ai_runtime::run_contract::AssistantRunEvent,
    ) -> AppResult<()> {
        self.emit(event)
    }
}

/// Project an already committed Run event without letting transient UI delivery
/// rewrite or interrupt the durable lifecycle fact.
pub(crate) fn emit_durable_event_best_effort(
    sink: &(impl RunEventSink + ?Sized),
    event: &crate::ai_runtime::run_contract::AssistantRunEvent,
) {
    if sink.emit(event).is_err() {
        tracing::warn!(
            run_id = %event.run_id(),
            reason = "durable_event_delivery_failed",
            "durable Agent Run event will be recovered from SQLite"
        );
    }
}

#[cfg(test)]
pub(super) struct NoopRunEventSink;

#[cfg(test)]
impl RunEventSink for NoopRunEventSink {
    fn emit(&self, _event: &crate::ai_runtime::run_contract::AssistantRunEvent) -> AppResult<()> {
        Ok(())
    }
}

/// Tauri adapter for the sole persisted Agent Run event channel.
pub(crate) struct TauriRunEventSink<'a, R: Runtime> {
    app_handle: &'a AppHandle<R>,
}

struct PresentationClock {
    started_at: Instant,
    next_seq: u64,
}

/// Presentation delivery can cross command boundaries (for example after a
/// confirmation resume), so its sequence clock belongs to the desktop process
/// rather than one short-lived IPC sink.
fn presentation_clocks() -> &'static Mutex<HashMap<String, PresentationClock>> {
    static CLOCKS: OnceLock<Mutex<HashMap<String, PresentationClock>>> = OnceLock::new();
    CLOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Internal prep stages stay in the durable log but are not shown in the process timeline.
fn is_internal_preparing_stage(stage: &str) -> bool {
    matches!(
        stage.trim(),
        "正在准备" | "正在准备工具执行" | "正在恢复运行状态"
    )
}

fn next_presentation_event(
    run_id: &str,
    payload: RunPresentationPayload,
) -> AppResult<RunPresentationEvent> {
    let mut clocks = presentation_clocks()
        .lock()
        .map_err(|_| AppError::msg("agent_run_presentation_lock_failed"))?;
    let clock = clocks
        .entry(run_id.to_string())
        .or_insert_with(|| PresentationClock {
            started_at: Instant::now(),
            next_seq: 1,
        });
    let event = RunPresentationEvent::new(
        run_id,
        clock.next_seq,
        clock
            .started_at
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
        payload,
    )
    .map_err(AppError::msg)?;
    clock.next_seq = clock.next_seq.saturating_add(1);
    Ok(event)
}

impl<'a, R: Runtime> TauriRunEventSink<'a, R> {
    pub(crate) fn new(app_handle: &'a AppHandle<R>) -> Self {
        Self { app_handle }
    }
}

impl<R: Runtime> RunEventSink for TauriRunEventSink<'_, R> {
    fn emit(&self, event: &crate::ai_runtime::run_contract::AssistantRunEvent) -> AppResult<()> {
        if self.app_handle.emit("assistant:run_event", event).is_err() {
            tracing::warn!(
                run_id = %event.run_id(),
                reason = "durable_event_delivery_failed",
                "durable Agent Run event will be recovered from SQLite"
            );
            return Ok(());
        }
        if let Some(payload) = presentation_payload_for_durable_event(event) {
            let _ = self.emit_presentation(event.run_id(), payload);
        }
        if matches!(
            event.payload(),
            RunEventPayload::Completed { .. }
                | RunEventPayload::Failed { .. }
                | RunEventPayload::Cancelled { .. }
        ) {
            if let Ok(mut clocks) = presentation_clocks().lock() {
                clocks.remove(event.run_id());
            }
        }
        Ok(())
    }

    fn emit_presentation(&self, run_id: &str, payload: RunPresentationPayload) -> AppResult<()> {
        let is_terminal = matches!(&payload, RunPresentationPayload::AnswerComplete);
        let event = next_presentation_event(run_id, payload)?;
        let result = self
            .app_handle
            .emit("assistant:run_presentation", event)
            .map_err(|_| AppError::msg("agent_run_presentation_delivery_failed"));
        if is_terminal {
            if let Ok(mut clocks) = presentation_clocks().lock() {
                clocks.remove(run_id);
            }
        }
        result
    }

    fn emit_terminal_presentation(&self, _run_id: &str) -> AppResult<()> {
        // `emit` projects the persisted Completed event to AnswerComplete.
        // Do not send a second terminal presentation here.
        Ok(())
    }
}

/// User-visible process copy for a Provider switch. Never include provider or tool ids.
fn provider_switch_process_label(capability: &str) -> &'static str {
    match capability {
        "model.respond" => "已切换到备用模型",
        "web.search" | "web.fetch" => "已改用备用检索服务",
        _ => "服务不可用，已切换到备用服务",
    }
}

/// Map one durable Run event into an optional live presentation payload.
fn presentation_payload_for_durable_event(
    event: &crate::ai_runtime::run_contract::AssistantRunEvent,
) -> Option<RunPresentationPayload> {
    match event.payload() {
        RunEventPayload::StageChanged { stage, .. } if !is_internal_preparing_stage(stage) => {
            Some(RunPresentationPayload::ProcessStarted {
                item_id: format!("stage:{}", event.seq()),
                item_kind: PresentationProcessKind::Stage,
                label: stage.clone(),
            })
        }
        // Reasoning summaries are projected live by AgentRunStreamObserver.
        // Re-projecting the durable event would double-count presentationSeq.
        RunEventPayload::ReasoningSummary { .. } => None,
        RunEventPayload::ToolStarted {
            capability,
            tool_call_id,
        } => Some(RunPresentationPayload::ProcessStarted {
            item_id: format!("tool:{tool_call_id}"),
            item_kind: PresentationProcessKind::Tool,
            label: capability.clone(),
        }),
        RunEventPayload::ToolCompleted {
            tool_call_id,
            duration_ms,
            success,
            ..
        } => Some(RunPresentationPayload::ProcessFinished {
            item_id: format!("tool:{tool_call_id}"),
            status: if *success == Some(false) {
                PresentationProcessStatus::Failed
            } else {
                PresentationProcessStatus::Completed
            },
            duration_ms: *duration_ms,
        }),
        RunEventPayload::ProviderSwitched { capability, .. } => {
            Some(RunPresentationPayload::ProcessStarted {
                item_id: format!("provider-switch:{}", event.seq()),
                item_kind: PresentationProcessKind::Stage,
                label: provider_switch_process_label(capability).to_string(),
            })
        }
        RunEventPayload::Completed { .. } => Some(RunPresentationPayload::AnswerComplete),
        RunEventPayload::Failed { .. } | RunEventPayload::Cancelled { .. } => None,
        _ => None,
    }
}

pub(crate) struct AgentRunStreamObserver<'a> {
    db: &'a Database,
    run_id: &'a str,
    running_state_version: u64,
    sink: &'a dyn RunEventSink,
    pending_delta: String,
    transient_content: String,
    candidate_rounds: u32,
    candidate_active: bool,
    discarded_candidates: u32,
    replacement_requests: u32,
    emitted_generating_answer_stage: bool,
    reasoning_summaries: BTreeMap<String, String>,
    persisted_reasoning_summaries: BTreeMap<String, String>,
    evaluation_telemetry: Option<crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap>,
}

impl<'a> AgentRunStreamObserver<'a> {
    /// Create an observer bound to one already-running normal-domain Run.
    #[cfg(test)]
    pub(crate) fn new(
        db: &'a Database,
        run_id: &'a str,
        running_state_version: u64,
        sink: &'a dyn RunEventSink,
    ) -> Self {
        Self::new_with_deferred_deltas(db, run_id, running_state_version, sink, false)
    }

    /// Receive private candidates for every Agent entry; finalization alone publishes.
    pub(crate) fn new_with_deferred_deltas(
        db: &'a Database,
        run_id: &'a str,
        running_state_version: u64,
        sink: &'a dyn RunEventSink,
        _defer_visible_deltas: bool,
    ) -> Self {
        Self {
            db,
            run_id,
            running_state_version,
            sink,
            pending_delta: String::new(),
            transient_content: String::new(),
            candidate_rounds: 0,
            candidate_active: false,
            discarded_candidates: 0,
            replacement_requests: 0,
            emitted_generating_answer_stage: false,
            reasoning_summaries: BTreeMap::new(),
            persisted_reasoning_summaries: BTreeMap::new(),
            evaluation_telemetry: None,
        }
    }

    /// Evaluation-only observer constructor. Measurements remain in the
    /// supplied memory tap and never enter the Run repository.
    pub(crate) fn new_with_eval_telemetry(
        db: &'a Database,
        run_id: &'a str,
        running_state_version: u64,
        sink: &'a dyn RunEventSink,
        defer_visible_deltas: bool,
        telemetry: crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap,
    ) -> Self {
        let mut observer = Self::new_with_deferred_deltas(
            db,
            run_id,
            running_state_version,
            sink,
            defer_visible_deltas,
        );
        observer.evaluation_telemetry = Some(telemetry);
        observer
    }
}

impl AgentRunStreamObserver<'_> {
    /// Replace provisional provider tokens with the fully validated final body.
    #[cfg(test)]
    pub(crate) fn bind_validated_content(&mut self, content: &str) {
        self.pending_delta.clear();
        // Test fixtures submit through the same atomic finalization as production.
        self.pending_delta.push_str(content);
        self.transient_content.clear();
    }

    /// No candidate is published before the terminal transaction.
    pub(crate) fn interrupt_visible_content(&self) -> String {
        String::new()
    }

    /// Cancellation must never materialize the private candidate as history.
    pub(crate) fn withholds_unvalidated_content(&self) -> bool {
        true
    }

    /// This observer only receives private candidates; publication belongs to finalization.
    pub(crate) fn has_visible_content(&self) -> bool {
        false
    }

    /// Discard internal draft text when tools or model recovery continue.
    pub(crate) fn reset_provisional_answer_if_any(&mut self) {
        if !self.transient_content.is_empty() {
            self.discarded_candidates += 1;
        }
        self.transient_content.clear();
        self.candidate_active = false;
        self.pending_delta.clear();
    }

    /// Whether the live "正在生成答复" stage was already emitted for this Run.
    pub(crate) fn emitted_generating_answer_stage(&self) -> bool {
        self.emitted_generating_answer_stage
    }

    /// Persist the user-visible generating stage once the tool loop will not run again.
    pub(crate) fn emit_generating_answer_stage_if_needed(&mut self) -> AppResult<()> {
        if self.emitted_generating_answer_stage {
            return Ok(());
        }
        let generating = AgentRunRepository::append_event(
            self.db,
            AppendRunEventInput {
                run_id: self.run_id.to_string(),
                state_version: self.running_state_version,
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state: RunState::Running,
                    stage: "正在生成答复".to_string(),
                    stage_code: Some(RunStageCode::GeneratingAnswer),
                },
            },
        )?;
        self.sink.emit(&generating)?;
        self.emitted_generating_answer_stage = true;
        Ok(())
    }

    /// Test harness delegates publication to the same atomic repository path.
    #[cfg(test)]
    pub(crate) fn flush(&mut self) -> AppResult<()> {
        let events = AgentRunRepository::finalize_with_events(
            self.db,
            FinalizeRunInput {
                run_id: self.run_id.to_string(),
                state_version: self.running_state_version,
                content: mem::take(&mut self.pending_delta),
                evidence_ids: Vec::new(),
                citation_map: serde_json::json!({}),
                source_summary: Vec::new(),
            },
        )?;
        for event in &events {
            emit_durable_event_best_effort(self.sink, event);
        }
        self.sink.emit_terminal_presentation(self.run_id)
    }

    fn observe_reasoning_summary(&mut self, summary_id: &str, text: &str) -> AppResult<()> {
        let summary_id = safe_reasoning_summary_id(summary_id);
        let text = safe_reasoning_summary(text);
        if summary_id.is_empty() || text.is_empty() {
            return Ok(());
        }
        let previous = self
            .reasoning_summaries
            .insert(summary_id.clone(), text.clone());
        let payload = if previous.is_some() {
            RunPresentationPayload::ProcessUpdated {
                item_id: format!("reasoning:{summary_id}"),
                label: text,
            }
        } else {
            RunPresentationPayload::ProcessStarted {
                item_id: format!("reasoning:{summary_id}"),
                item_kind: PresentationProcessKind::ReasoningSummary,
                label: text,
            }
        };
        let _ = self.sink.emit_presentation(self.run_id, payload);
        Ok(())
    }

    fn persist_reasoning_summaries(&mut self) -> AppResult<()> {
        for (summary_id, text) in self.reasoning_summaries.clone() {
            if self.persisted_reasoning_summaries.get(&summary_id) == Some(&text) {
                continue;
            }
            let event = AgentRunRepository::append_event(
                self.db,
                AppendRunEventInput {
                    run_id: self.run_id.to_string(),
                    state_version: self.running_state_version,
                    event_type: RunEventType::ReasoningSummary,
                    payload: RunEventPayload::ReasoningSummary {
                        summary_id: summary_id.clone(),
                        text: text.clone(),
                    },
                },
            )?;
            self.sink.emit(&event)?;
            let _ = self.sink.emit_presentation(
                self.run_id,
                RunPresentationPayload::ProcessFinished {
                    item_id: format!("reasoning:{summary_id}"),
                    status: PresentationProcessStatus::Completed,
                    duration_ms: None,
                },
            );
            self.persisted_reasoning_summaries.insert(summary_id, text);
        }
        Ok(())
    }
}

fn safe_reasoning_summary(value: &str) -> String {
    // JSON expands control characters to up to six visible characters. Normalize
    // non-layout controls before the fixed 800-char bound so a transient summary
    // can never render successfully and then fail the durable 2,000-char event
    // budget at turn completion.
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let redacted = crate::ai_runtime::trace::redact_classified_leaks(&normalized);
    let trimmed = redacted.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if looks_like_tool_argument_or_structured_data(trimmed) {
        return "已完成必要的推理准备。".to_string();
    }
    // Keep comfortably below both the per-summary 1,500-char cap and the
    // 2,000-char serialized Run-event budget even when JSON escaping expands
    // every character. The ID has a separate conservative bound below.
    truncate_reasoning_summary(trimmed, 800)
}

fn safe_reasoning_summary_id(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    truncate_reasoning_summary(&normalized, 96)
}

fn truncate_reasoning_summary(value: &str, limit: usize) -> String {
    if limit == 0 {
        return String::new();
    }
    if value.chars().count() <= limit {
        value.to_string()
    } else {
        let truncated = value
            .chars()
            .take(limit.saturating_sub(1))
            .collect::<String>();
        format!("{truncated}…")
    }
}

fn looks_like_tool_argument_or_structured_data(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    value.starts_with('{')
        || value.starts_with('[')
        || [
            "\"query\"",
            "\"url\"",
            "\"arguments\"",
            "tool_call",
            "call_",
            "api_key",
            "authorization",
            "token=",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
}

impl Drop for AgentRunStreamObserver<'_> {
    fn drop(&mut self) {
        // Numeric diagnostics only; a diagnostics write cannot change publication outcome.
        let _ = self.db.with_conn(|conn| {
            conn.execute("UPDATE agent_runs SET provider_route_summary_json = json_set(provider_route_summary_json,
                '$.publication.candidateRounds', ?1, '$.publication.discardedCandidates', ?2,
                '$.publication.rejectedResets', ?3, '$.publication.resetReason', 'private_candidate_only') WHERE run_id = ?4",
                rusqlite::params![self.candidate_rounds, self.discarded_candidates, self.replacement_requests, self.run_id])?;
            Ok(())
        });
    }
}

impl crate::ai_runtime::model_gateway::StreamEventObserver for AgentRunStreamObserver<'_> {
    fn observe(
        &mut self,
        event: &crate::ai_runtime::model_gateway::StreamEvent,
        _token_index: u32,
    ) -> AppResult<()> {
        if let Some(telemetry) = &self.evaluation_telemetry {
            telemetry.record_stream_event(event);
        }
        match &event.data {
            crate::ai_runtime::model_gateway::StreamEventData::Token {
                token,
                replace_visible,
            } => {
                if !event.surface.sanitizes_visible_output() {
                    return Ok(());
                }
                if *replace_visible {
                    self.replacement_requests += 1;
                    self.transient_content.clear();
                }
                if !self.candidate_active && !token.is_empty() {
                    self.candidate_rounds += 1;
                    self.candidate_active = true;
                }
                self.transient_content.push_str(token);
            }
            crate::ai_runtime::model_gateway::StreamEventData::ReasoningSummary {
                summary_id,
                text,
            } => self.observe_reasoning_summary(summary_id, text)?,
            crate::ai_runtime::model_gateway::StreamEventData::Done { .. } => {
                self.candidate_active = false;
                self.persist_reasoning_summaries()?
            }
            crate::ai_runtime::model_gateway::StreamEventData::ToolCall { .. }
            | crate::ai_runtime::model_gateway::StreamEventData::Error { .. } => {}
        }
        Ok(())
    }

    fn on_tools_finished(&mut self) -> AppResult<()> {
        // A tool completion never authorizes publication of the next candidate.
        Ok(())
    }

    fn on_tools_starting(&mut self) -> AppResult<()> {
        self.reset_provisional_answer_if_any();
        Ok(())
    }

    fn reset_visible_answer_for_new_attempt(&mut self) {
        self.reset_provisional_answer_if_any();
    }

    fn has_visible_content(&self) -> bool {
        self.has_visible_content()
    }

    fn visible_content_snapshot(&self) -> Option<String> {
        let content = self.interrupt_visible_content();
        (!content.trim().is_empty()).then_some(content)
    }
}

#[cfg(test)]
mod presentation_clock_tests {
    use super::{
        is_internal_preparing_stage, next_presentation_event, presentation_clocks,
        presentation_payload_for_durable_event,
    };
    use crate::ai_runtime::run_contract::{
        PresentationProcessKind, RunEventPayload, RunEventType, RunPresentationPayload, RunState,
    };

    #[test]
    fn presentation_sequence_survives_a_new_sink_for_the_same_run() {
        let run_id = "presentation-clock-cross-sink";
        let first = next_presentation_event(
            run_id,
            RunPresentationPayload::ProcessStarted {
                item_id: "stage:1".to_string(),
                item_kind: PresentationProcessKind::Stage,
                label: "正在准备".to_string(),
            },
        )
        .expect("first presentation event");
        let second = next_presentation_event(run_id, RunPresentationPayload::AnswerComplete)
            .expect("second presentation event");

        assert_eq!(
            serde_json::to_value(first).expect("serialize")["presentationSeq"],
            1
        );
        assert_eq!(
            serde_json::to_value(second).expect("serialize")["presentationSeq"],
            2
        );
        presentation_clocks()
            .lock()
            .expect("presentation clocks")
            .remove(run_id);
    }

    #[test]
    fn internal_preparing_stages_are_not_projected_to_presentation() {
        assert!(is_internal_preparing_stage("正在准备"));
        assert!(is_internal_preparing_stage("正在准备工具执行"));
        assert!(is_internal_preparing_stage("正在恢复运行状态"));
        assert!(!is_internal_preparing_stage("正在调用模型和工具"));
        assert!(!is_internal_preparing_stage("正在生成答复"));

        let preparing = crate::ai_runtime::run_contract::AssistantRunEvent::new(
            "run-prep",
            2,
            1,
            RunEventType::StageChanged,
            "2026-07-22T08:00:00Z",
            RunEventPayload::StageChanged {
                state: RunState::Preparing,
                stage: "正在准备".to_string(),
                stage_code: None,
            },
        )
        .expect("preparing event");
        assert!(presentation_payload_for_durable_event(&preparing).is_none());

        let running = crate::ai_runtime::run_contract::AssistantRunEvent::new(
            "run-prep",
            3,
            2,
            RunEventType::StageChanged,
            "2026-07-22T08:00:01Z",
            RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "正在调用模型和工具".to_string(),
                stage_code: None,
            },
        )
        .expect("running event");
        let payload = presentation_payload_for_durable_event(&running).expect("projected");
        assert!(matches!(
            payload,
            RunPresentationPayload::ProcessStarted { label, .. } if label == "正在调用模型和工具"
        ));
    }

    #[test]
    fn failed_and_cancelled_runs_do_not_emit_answer_complete_presentation() {
        for (event_type, payload) in [
            (
                RunEventType::Failed,
                RunEventPayload::Failed {
                    code: crate::ai_runtime::run_contract::SafeRunErrorCode::IncompleteOutput,
                    message: "回答未完整生成，请重试".to_string(),
                },
            ),
            (
                RunEventType::Cancelled,
                RunEventPayload::Cancelled {
                    reason: "user_cancelled".to_string(),
                },
            ),
        ] {
            let event = crate::ai_runtime::run_contract::AssistantRunEvent::new(
                "terminal-projection",
                3,
                2,
                event_type,
                "2026-08-06T08:00:00Z",
                payload,
            )
            .expect("terminal event");
            assert!(presentation_payload_for_durable_event(&event).is_none());
        }
    }

    #[test]
    fn provider_switch_copy_distinguishes_model_and_web_tools() {
        for (capability, expected) in [
            ("model.respond", "已切换到备用模型"),
            ("web.search", "已改用备用检索服务"),
            ("web.fetch", "已改用备用检索服务"),
        ] {
            let event = crate::ai_runtime::run_contract::AssistantRunEvent::new(
                "provider-switch-copy",
                3,
                2,
                RunEventType::ProviderSwitched,
                "2026-09-01T08:00:00Z",
                RunEventPayload::ProviderSwitched {
                    capability: capability.into(),
                    from_provider_id: "primary".into(),
                    provider_id: "backup".into(),
                    model_id: "model-or-tool".into(),
                    reason_code: "provider_failed".into(),
                    attempt: 2,
                },
            )
            .expect("provider switch event");
            let payload = presentation_payload_for_durable_event(&event).expect("presentation");
            assert!(matches!(
                payload,
                RunPresentationPayload::ProcessStarted { label, .. } if label == expected
            ));
        }
    }
}

#[cfg(test)]
mod strict_publish_tests {
    use std::sync::Mutex;

    use super::{AgentRunStreamObserver, RunEventSink};
    use crate::ai_runtime::agent_run_repository::{AgentRunRepository, AppendRunEventInput};
    use crate::ai_runtime::model_gateway::{
        StreamEvent, StreamEventData, StreamEventObserver, StreamEventType, StreamSurface,
    };
    use crate::ai_runtime::run_contract::{
        AssistantRunEvent, AssistantRunStartRequest, AssistantTurnDraft, RunEventPayload,
        RunEventType, RunPresentationPayload, RunState, SecurityDomain,
    };
    use crate::ai_runtime::run_intake::RunIntake;
    use crate::error::AppResult;
    use crate::storage::db::Database;

    #[derive(Default)]
    struct PresentationSink {
        events: Mutex<Vec<RunPresentationPayload>>,
    }

    impl RunEventSink for PresentationSink {
        fn emit(&self, _event: &AssistantRunEvent) -> AppResult<()> {
            Ok(())
        }

        fn emit_presentation(
            &self,
            _run_id: &str,
            payload: RunPresentationPayload,
        ) -> AppResult<()> {
            self.events
                .lock()
                .expect("presentation events")
                .push(payload);
            Ok(())
        }
    }

    #[test]
    fn strict_path_publishes_only_the_validated_answer_without_reset() {
        let db = Database::open_in_memory().expect("database");
        let accepted = RunIntake::start(
            &db,
            AssistantRunStartRequest {
                client_request_id: "strict-publish".into(),
                session: None,
                turn: AssistantTurnDraft {
                    message: "请联网核实".into(),
                    content_parts: None,
                    explicit_references: Vec::new(),
                    retrieval_scope: Default::default(),
                    display_mentions: Vec::new(),
                },
                explicit_action: None,
                web_enabled: true,
                model_override: None,
                external_tool_grants: Vec::new(),
                security_domain: SecurityDomain::Normal,
                classified_context_ref: None,
            },
        )
        .expect("accepted");
        let preparing = AgentRunRepository::append_event(
            &db,
            AppendRunEventInput {
                run_id: accepted.run_id.clone(),
                state_version: 0,
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state: RunState::Preparing,
                    stage: "正在准备工具执行".into(),
                    stage_code: None,
                },
            },
        )
        .expect("preparing");
        let running = AgentRunRepository::append_event(
            &db,
            AppendRunEventInput {
                run_id: accepted.run_id.clone(),
                state_version: preparing.state_version(),
                event_type: RunEventType::StageChanged,
                payload: RunEventPayload::StageChanged {
                    state: RunState::Running,
                    stage: "正在调用模型和工具".into(),
                    stage_code: None,
                },
            },
        )
        .expect("running");
        let sink = PresentationSink::default();
        let mut observer = AgentRunStreamObserver::new_with_deferred_deltas(
            &db,
            &accepted.run_id,
            running.state_version(),
            &sink,
            true,
        );
        observer.on_tools_finished().expect("tools finished");
        observer
            .observe(
                &StreamEvent {
                    request_id: accepted.run_id.clone(),
                    event_type: StreamEventType::Token,
                    data: StreamEventData::Token {
                        token: "未经验证的完整草稿。".into(),
                        replace_visible: false,
                    },
                    surface: StreamSurface::VisibleAnswerSanitized,
                    classified: false,
                },
                0,
            )
            .expect("draft token");
        assert!(sink.events.lock().expect("events").is_empty());
        assert!(observer.interrupt_visible_content().is_empty());
        assert!(observer.withholds_unvalidated_content());

        observer.bind_validated_content("验证通过后的唯一答复。");
        observer.flush().expect("validated flush");
        let events = sink.events.lock().expect("events");
        assert!(events
            .iter()
            .all(|event| !matches!(event, RunPresentationPayload::AnswerReset)));
        assert!(events
            .iter()
            .all(|event| !matches!(event, RunPresentationPayload::AnswerDelta { .. })));
        let replay = RunIntake::get(&db, &accepted.session, &accepted.run_id)
            .unwrap()
            .unwrap();
        let body: String = replay
            .events
            .iter()
            .filter_map(|event| match event.payload() {
                RunEventPayload::ContentDelta { delta } => Some(delta.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(body, "验证通过后的唯一答复。");
    }
}
