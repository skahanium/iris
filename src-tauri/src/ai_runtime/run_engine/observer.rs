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

    /// Emit a safe terminal event when SQLite itself cannot record that event.
    fn emit_ephemeral_failure(
        &self,
        event: &crate::ai_runtime::run_contract::AssistantRunEvent,
    ) -> AppResult<()> {
        self.emit(event)
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
        self.app_handle
            .emit("assistant:run_event", event)
            .map_err(|_| AppError::msg("agent_run_event_emit_failed"))?;
        if let Some(payload) = presentation_payload_for_durable_event(event) {
            let _ = self.emit_presentation(event.run_id(), payload);
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
                label: if capability == "model.respond" {
                    "主模型不可用，已切换到备用模型".to_string()
                } else {
                    "服务不可用，已切换到备用服务".to_string()
                },
            })
        }
        RunEventPayload::Failed { .. }
        | RunEventPayload::Cancelled { .. }
        | RunEventPayload::Completed { .. } => Some(RunPresentationPayload::AnswerComplete),
        _ => None,
    }
}

/// Separates live UI stream snapshots from the one validated durable Run delta.
const STREAM_PRESENTATION_FLUSH_INTERVAL: Duration = Duration::from_millis(33);
pub(crate) struct AgentRunStreamObserver<'a> {
    db: &'a Database,
    run_id: &'a str,
    running_state_version: u64,
    sink: &'a dyn RunEventSink,
    pending_delta: String,
    transient_content: String,
    last_presentation_emit_at: Instant,
    presentation_content: String,
    defer_visible_deltas: bool,
    source_group_citation_filter: bool,
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

    /// Create an observer that holds visible deltas until a verifier accepts final output.
    pub(crate) fn new_with_deferred_deltas(
        db: &'a Database,
        run_id: &'a str,
        running_state_version: u64,
        sink: &'a dyn RunEventSink,
        defer_visible_deltas: bool,
    ) -> Self {
        Self {
            db,
            run_id,
            running_state_version,
            sink,
            pending_delta: String::new(),
            transient_content: String::new(),
            last_presentation_emit_at: Instant::now(),
            presentation_content: String::new(),
            defer_visible_deltas,
            source_group_citation_filter: false,
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
    /// Keep model-authored precise citation syntax out of an uncalibrated
    /// source-group stream before any AnswerDelta reaches the UI.
    pub(crate) fn enable_source_group_citation_filter(&mut self) {
        self.source_group_citation_filter = true;
    }

    /// Replace provisional provider tokens with the fully validated final body.
    pub(crate) fn bind_validated_content(&mut self, content: &str) {
        self.pending_delta.clear();
        self.pending_delta.push_str(content);
        if self.presentation_content != content {
            if !self.presentation_content.is_empty() {
                let _ = self
                    .sink
                    .emit_presentation(self.run_id, RunPresentationPayload::AnswerReset);
            }
            self.presentation_content.clear();
        }
        self.transient_content.clear();
    }

    /// Visible answer text captured before cancellation, already buffered for the UI.
    pub(crate) fn interrupt_visible_content(&self) -> String {
        if !self.presentation_content.is_empty() {
            return self.presentation_content.clone();
        }
        self.transient_content.clone()
    }

    /// Whether this model attempt has already produced user-visible tokens.
    /// A fallback after this point would splice two providers into one answer.
    pub(crate) fn has_visible_content(&self) -> bool {
        !self.presentation_content.is_empty()
    }

    /// Allow a later final turn to emit AnswerDelta after tool rounds stayed private.
    pub(crate) fn clear_deferred_visible_deltas(&mut self) {
        self.defer_visible_deltas = false;
    }

    /// Hide provisional tokens again when another tool round begins.
    pub(crate) fn enable_deferred_visible_deltas(&mut self) {
        self.defer_visible_deltas = true;
    }

    /// Drop any already-streamed provisional answer before more tools run.
    pub(crate) fn reset_provisional_answer_if_any(&mut self) {
        if self.presentation_content.is_empty() && self.transient_content.is_empty() {
            return;
        }
        if !self.presentation_content.is_empty() {
            let _ = self
                .sink
                .emit_presentation(self.run_id, RunPresentationPayload::AnswerReset);
        }
        self.presentation_content.clear();
        self.transient_content.clear();
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

    /// Deliver the complete provisional snapshot to the live UI without persistence.
    pub(crate) fn flush_transient(&mut self) -> AppResult<()> {
        if self.defer_visible_deltas || self.transient_content.is_empty() {
            return Ok(());
        }
        let visible = if self.source_group_citation_filter {
            crate::ai_runtime::citation_linkify::strip_model_authored_citation_markers_for_stream(
                &self.transient_content,
            )
        } else {
            self.transient_content.clone()
        };
        if visible == self.presentation_content {
            return Ok(());
        }
        let delta = if let Some(delta) = visible.strip_prefix(&self.presentation_content) {
            delta.to_string()
        } else {
            if !self.presentation_content.is_empty() {
                let _ = self
                    .sink
                    .emit_presentation(self.run_id, RunPresentationPayload::AnswerReset);
            }
            self.presentation_content.clear();
            visible
        };
        if delta.is_empty() {
            return Ok(());
        }
        let _ = self.sink.emit_presentation(
            self.run_id,
            RunPresentationPayload::AnswerDelta {
                delta: delta.clone(),
            },
        );
        self.presentation_content.push_str(&delta);
        self.last_presentation_emit_at = Instant::now();
        Ok(())
    }

    /// Persist and emit bounded, already-validated visible fragments.
    ///
    /// Final answers are bound as one string but must be split before persistence:
    /// Run events reject payloads over the 2_000-char safe-event budget. A single long
    /// web-grounded answer previously failed flush as `agent_run_persistence_failed`
    /// after evidence had already registered.
    ///
    /// Even when `pending_delta` is empty (already streamed or a retry after a partial
    /// flush), AnswerComplete must still be delivered so the UI can leave streaming.
    pub(crate) fn flush(&mut self) -> AppResult<()> {
        let emit_final_presentation = self.presentation_content != self.pending_delta;
        if !self.pending_delta.is_empty() {
            let mut remaining = mem::take(&mut self.pending_delta);
            while !remaining.is_empty() {
                let chunk = take_safe_content_delta_chunk(&mut remaining)?;
                if chunk.is_empty() {
                    break;
                }
                let persisted = AgentRunRepository::append_event(
                    self.db,
                    AppendRunEventInput {
                        run_id: self.run_id.to_string(),
                        state_version: self.running_state_version,
                        event_type: RunEventType::ContentDelta,
                        payload: RunEventPayload::ContentDelta {
                            delta: chunk.clone(),
                        },
                    },
                )?;
                self.sink.emit(&persisted)?;
                if emit_final_presentation {
                    let _ = self.sink.emit_presentation(
                        self.run_id,
                        RunPresentationPayload::AnswerDelta {
                            delta: chunk.clone(),
                        },
                    );
                    self.presentation_content.push_str(&chunk);
                }
            }
        }
        let _ = self
            .sink
            .emit_presentation(self.run_id, RunPresentationPayload::AnswerComplete);
        Ok(())
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

/// Keep each ContentDelta JSON under the Run event safe-text budget (2_000 chars).
fn take_safe_content_delta_chunk(remaining: &mut String) -> AppResult<String> {
    const SAFE_EVENT_BUDGET_CHARS: usize = 2_000;
    const INITIAL_CHUNK_CHARS: usize = 1_500;
    if remaining.is_empty() {
        return Ok(String::new());
    }
    let total = remaining.chars().count();
    let mut end = total.min(INITIAL_CHUNK_CHARS);
    loop {
        let chunk: String = remaining.chars().take(end).collect();
        let payload = RunEventPayload::ContentDelta {
            delta: chunk.clone(),
        };
        let encoded = serde_json::to_string(&payload)?;
        if encoded.chars().count() <= SAFE_EVENT_BUDGET_CHARS || end <= 1 {
            *remaining = remaining.chars().skip(chunk.chars().count()).collect();
            return Ok(chunk);
        }
        end = (end * 3 / 4).max(1);
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
                    self.transient_content.clear();
                    if !self.presentation_content.is_empty() {
                        let _ = self
                            .sink
                            .emit_presentation(self.run_id, RunPresentationPayload::AnswerReset);
                        self.presentation_content.clear();
                    }
                }
                self.transient_content.push_str(token);
                if !self.defer_visible_deltas
                    && (self.presentation_content.is_empty()
                        || self.last_presentation_emit_at.elapsed()
                            >= STREAM_PRESENTATION_FLUSH_INTERVAL
                        || token.contains('\n')
                        || *replace_visible)
                {
                    self.flush_transient()?;
                }
            }
            crate::ai_runtime::model_gateway::StreamEventData::ReasoningSummary {
                summary_id,
                text,
            } => self.observe_reasoning_summary(summary_id, text)?,
            crate::ai_runtime::model_gateway::StreamEventData::Done { .. } => {
                self.persist_reasoning_summaries()?
            }
            crate::ai_runtime::model_gateway::StreamEventData::ToolCall { .. }
            | crate::ai_runtime::model_gateway::StreamEventData::Error { .. } => {}
        }
        Ok(())
    }

    fn on_tools_finished(&mut self) -> AppResult<()> {
        // Unlock provisional/final streaming between model turns, but do not emit
        // "正在生成答复" here: later tool rounds (e.g. read_note after search) must
        // still appear before that stage in the process timeline.
        self.clear_deferred_visible_deltas();
        Ok(())
    }

    fn on_tools_starting(&mut self) -> AppResult<()> {
        self.enable_deferred_visible_deltas();
        self.reset_provisional_answer_if_any();
        Ok(())
    }

    fn has_visible_content(&self) -> bool {
        self.has_visible_content()
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
}
