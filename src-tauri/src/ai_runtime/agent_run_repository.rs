//! SQLite repository for the unified normal-domain Agent Run facts.
//!
//! This module is deliberately storage-only. It does not resolve envelopes,
//! dispatch providers, emit IPC events, or provide a compatibility path for
//! the legacy Harness. Stage 4 owns those responsibilities.

use crate::ai_runtime::run_contract::{
    transition_to, AssistantRunAccepted, AssistantRunEvent, AssistantRunGetResponse,
    AssistantRunSnapshot, AssistantSessionRef, ExecutionEnvelope, RunEventPayload, RunEventType,
    RunState, SecurityDomain,
};
use crate::ai_types::{
    ContentPart, ContextReferenceKind, ContextReferenceWire, EditorRangeWire, SourceSpan,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_SAFE_EVENT_TEXT_CHARS: usize = 2_000;
const MAX_CHECKPOINT_STRING_CHARS: usize = 512;
const MAX_CHECKPOINT_SAFE_STATE_DEPTH: usize = 4;
const MAX_CHECKPOINT_SAFE_STATE_ITEMS: usize = 64;

/// Facts Request Intake must atomically write before any execution work.
#[derive(Debug, Clone)]
pub(crate) struct AcceptRunInput {
    /// Internal normal-domain session foreign key resolved by Request Intake.
    pub(crate) session_id: i64,
    /// Opaque session key that must match `session_id` before persistence.
    pub(crate) session_key: String,
    /// Client-supplied idempotency key.
    pub(crate) client_request_id: String,
    /// Stable identifier allocated for a new Run.
    pub(crate) run_id: String,
    /// Stable logical turn identifier allocated for the user message and Run.
    pub(crate) turn_id: String,
    /// Full user body; this is persisted only in `session_messages`.
    pub(crate) message: String,
    /// Optional full multimodal user content; persisted only in `session_messages`.
    pub(crate) content_parts: Option<Vec<ContentPart>>,
    /// Explicit references whose persisted form excludes excerpts.
    pub(crate) explicit_references: Vec<ContextReferenceWire>,
    /// Already-resolved execution boundary for this Run.
    pub(crate) envelope: ExecutionEnvelope,
}

/// Safe event append request. Sequence numbers are allocated by the repository.
#[derive(Debug, Clone)]
pub(crate) struct AppendRunEventInput {
    /// Owning Run identifier.
    pub(crate) run_id: String,
    /// Version the Run Engine observed before emitting this event.
    pub(crate) state_version: u64,
    /// Event discriminator, validated against the payload.
    pub(crate) event_type: RunEventType,
    /// Safe, UI-oriented event payload.
    pub(crate) payload: RunEventPayload,
}

/// One durable, summary-shaped checkpoint for a recoverable Run Step.
#[derive(Debug, Clone)]
pub(crate) struct AppendRunCheckpointInput {
    /// Owning Run identifier.
    pub(crate) run_id: String,
    /// Version the Run Engine observed before persisting the checkpoint.
    pub(crate) state_version: u64,
    /// Stable executor step kind.
    pub(crate) kind: String,
    /// Safe executor-facing step status.
    pub(crate) status: String,
    /// Bounded summary of the safe input already consumed.
    pub(crate) input_summary: String,
    /// Bounded summary of the safe output already produced.
    pub(crate) output_summary: String,
    /// Versioned and validated resume data; never a raw Harness snapshot.
    pub(crate) checkpoint: Value,
}

/// Facts that must commit with a Run's successful terminal transition.
#[derive(Debug, Clone)]
pub(crate) struct FinalizeRunInput {
    pub(crate) run_id: String,
    pub(crate) state_version: u64,
    pub(crate) content: String,
    pub(crate) evidence_ids: Vec<i64>,
    pub(crate) citation_map: Value,
}

/// Result of consuming a persisted confirmation through one idempotent control request.
pub(crate) enum FrozenConfirmationApproval {
    /// The pending plan was consumed and the Run durably resumed.
    Resumed(AssistantRunEvent),
    /// The same plan had already been consumed by an earlier identical control request.
    AlreadyApplied,
}

/// Repository for normal-domain Run, Event and intake facts.
pub(crate) struct AgentRunRepository;

impl AgentRunRepository {
    /// Atomically persist the accepted user Turn, Run and first event.
    ///
    /// A repeated `client_request_id` returns the original accepted identity
    /// without adding another user message or event.
    pub(crate) fn accept(db: &Database, input: AcceptRunInput) -> AppResult<AssistantRunAccepted> {
        if input.envelope.security_domain != SecurityDomain::Normal {
            return Err(AppError::msg("agent_run_classified_domain_not_supported"));
        }
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                if let Some(existing) = accepted_for_client_request(conn, &input.client_request_id)?
                {
                    return Ok(existing);
                }

                ensure_normal_session(conn, input.session_id, &input.session_key)?;
                let now = chrono::Utc::now().to_rfc3339();
                let content_parts_json = input
                    .content_parts
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?;
                let explicit_references_json = serde_json::to_string(
                    &input
                        .explicit_references
                        .iter()
                        .map(PersistedExplicitReference::from)
                        .collect::<Vec<_>>(),
                )?;
                let envelope_json = serde_json::to_string(&input.envelope)?;
                let effect = enum_wire(&input.envelope.effect)?;
                let effort = enum_wire(&input.envelope.effort)?;
                let security_domain = enum_wire(&input.envelope.security_domain)?;
                let risk = enum_wire(&input.envelope.risk)?;
                let message_hash = crate::cas::hash::content_hash_str(&input.message);
                let goal_summary = safe_body_summary(&input.message);

                let seq: i64 = conn.query_row(
                    "SELECT COALESCE(MAX(seq), 0) + 1 FROM session_messages WHERE session_id = ?1",
                    [input.session_id],
                    |row| row.get(0),
                )?;
                conn.execute(
                    "INSERT INTO session_messages
                 (session_id, seq, role, content, content_parts, content_hash, created_at,
                  turn_id, explicit_references_json)
                 VALUES (?1, ?2, 'user', ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        input.session_id,
                        seq,
                        input.message,
                        content_parts_json,
                        message_hash,
                        now,
                        input.turn_id,
                        explicit_references_json,
                    ],
                )?;
                conn.execute(
                    "INSERT INTO agent_runs
                 (run_id, client_request_id, session_id, turn_id, status, state_version,
                  effect, effort, security_domain, risk, envelope_json, goal_summary,
                  created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'accepted', 0, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
                    rusqlite::params![
                        input.run_id,
                        input.client_request_id,
                        input.session_id,
                        input.turn_id,
                        effect,
                        effort,
                        security_domain,
                        risk,
                        envelope_json,
                        goal_summary,
                        now,
                    ],
                )?;
                let event = AssistantRunEvent::new(
                    &input.run_id,
                    1,
                    0,
                    RunEventType::Accepted,
                    &now,
                    RunEventPayload::Accepted {
                        turn_id: input.turn_id.clone(),
                        session_key: input.session_key.clone(),
                    },
                )
                .map_err(AppError::msg)?;
                insert_event(conn, &event)?;
                conn.execute(
                    "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, input.session_id],
                )?;

                Ok(AssistantRunAccepted {
                    run_id: input.run_id,
                    turn_id: input.turn_id,
                    session: AssistantSessionRef {
                        domain: SecurityDomain::Normal,
                        session_key: input.session_key,
                    },
                    state: RunState::Accepted,
                    state_version: 0,
                })
            })
        })
    }

    /// Append one safe event with the next strict Run-local sequence number.
    pub(crate) fn append_event(
        db: &Database,
        input: AppendRunEventInput,
    ) -> AppResult<AssistantRunEvent> {
        validate_safe_event_payload(&input.payload)?;
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                let (status, stored_state_version): (String, u64) = conn
                    .query_row(
                        "SELECT status, state_version FROM agent_runs WHERE run_id = ?1",
                        [&input.run_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(not_found_or_db)?;
                let state = parse_wire::<RunState>(&status)?;
                if state.is_terminal() {
                    return Err(AppError::msg("agent_run_terminal_state"));
                }
                if matches!(&input.payload, RunEventPayload::Completed { .. }) {
                    return Err(AppError::msg("agent_run_finalization_required"));
                }
                if input.state_version != stored_state_version {
                    return Err(AppError::msg("agent_run_state_version_conflict"));
                }
                validate_tool_call_lifecycle(conn, &input.run_id, &input.payload)?;
                let next_state = state_for_event(&input.payload).unwrap_or(state);
                let next_state = transition_to(state, next_state).map_err(|error| {
                    AppError::msg(match error {
                        crate::ai_runtime::run_contract::RunStateTransitionError::TerminalState => {
                            "agent_run_terminal_state"
                        }
                        crate::ai_runtime::run_contract::RunStateTransitionError::IllegalTransition => {
                            "agent_run_illegal_transition"
                        }
                        crate::ai_runtime::run_contract::RunStateTransitionError::StateVersionConflict => {
                            "agent_run_state_version_conflict"
                        }
                    })
                })?;
                let next_state_version = if next_state == state {
                    stored_state_version
                } else {
                    stored_state_version + 1
                };
                let now = chrono::Utc::now().to_rfc3339();
                let updated = conn.execute(
                    "UPDATE agent_runs
                     SET status = ?1, state_version = ?2, updated_at = ?3
                     WHERE run_id = ?4 AND state_version = ?5",
                    rusqlite::params![
                        enum_wire(&next_state)?,
                        next_state_version,
                        now,
                        input.run_id,
                        stored_state_version,
                    ],
                )?;
                if updated != 1 {
                    return Err(AppError::msg("agent_run_state_version_conflict"));
                }
                let seq: u64 = conn.query_row(
                "SELECT COALESCE(MAX(event_seq), 0) + 1 FROM agent_run_events WHERE run_id = ?1",
                [&input.run_id],
                |row| row.get(0),
            )?;
                let event = AssistantRunEvent::new(
                    &input.run_id,
                    seq,
                    next_state_version,
                    input.event_type,
                    now,
                    input.payload,
                )
                .map_err(AppError::msg)?;
                insert_event(conn, &event)?;
                Ok(event)
            })
        })
    }

    /// Persist a validated checkpoint only for a durable or safely blocked Run.
    pub(crate) fn append_checkpoint_step(
        db: &Database,
        input: AppendRunCheckpointInput,
    ) -> AppResult<()> {
        let evidence_ids = validate_checkpoint_schema(&input.checkpoint)?;
        validate_checkpoint_step_input(&input)?;
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                let (status, stored_state_version, effort, session_id): (String, u64, String, i64) =
                    conn.query_row(
                        "SELECT status, state_version, effort, session_id
                         FROM agent_runs WHERE run_id = ?1",
                        [&input.run_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .map_err(not_found_or_db)?;
                let state = parse_wire::<RunState>(&status)?;
                if state.is_terminal() {
                    return Err(AppError::msg("agent_run_terminal_state"));
                }
                if input.state_version != stored_state_version {
                    return Err(AppError::msg("agent_run_state_version_conflict"));
                }
                let effort = parse_wire::<crate::ai_runtime::run_contract::Effort>(&effort)?;
                let checkpoint_allowed = effort == crate::ai_runtime::run_contract::Effort::Durable
                    || matches!(state, RunState::Paused | RunState::AwaitingConfirmation);
                if !checkpoint_allowed {
                    return Err(AppError::msg("agent_run_checkpoint_not_durable"));
                }
                ensure_evidence_ids_belong_to_session(conn, session_id, &evidence_ids)?;
                let step_seq: i64 = conn.query_row(
                    "SELECT COALESCE(MAX(step_seq), 0) + 1
                     FROM agent_run_steps WHERE run_id = ?1",
                    [&input.run_id],
                    |row| row.get(0),
                )?;
                let now = chrono::Utc::now().to_rfc3339();
                conn.execute(
                    "INSERT INTO agent_run_steps
                     (run_id, step_seq, kind, status, input_summary, output_summary,
                      resume_state_json, evidence_refs_json, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                    rusqlite::params![
                        input.run_id,
                        step_seq,
                        input.kind,
                        input.status,
                        input.input_summary,
                        input.output_summary,
                        serde_json::to_string(&input.checkpoint)?,
                        serde_json::to_string(&evidence_ids)?,
                        now,
                    ],
                )?;
                let updated = conn.execute(
                    "UPDATE agent_runs SET updated_at = ?1
                     WHERE run_id = ?2 AND state_version = ?3",
                    rusqlite::params![now, input.run_id, stored_state_version],
                )?;
                if updated != 1 {
                    return Err(AppError::msg("agent_run_state_version_conflict"));
                }
                Ok(())
            })
        })
    }

    /// Atomically persist final output, terminal Run state, and completed event.
    pub(crate) fn finalize(db: &Database, input: FinalizeRunInput) -> AppResult<String> {
        if input.content.trim().is_empty() || input.content.chars().count() > 32_000 {
            return Err(AppError::msg("agent_run_invalid_final_output"));
        }
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                let (session_id, turn_id, status, stored_version): (i64, String, String, u64) = conn
                    .query_row(
                        "SELECT session_id, turn_id, status, state_version FROM agent_runs WHERE run_id = ?1",
                        [&input.run_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .map_err(not_found_or_db)?;
                let state = parse_wire::<RunState>(&status)?;
                if state.is_terminal() {
                    return Err(AppError::msg("agent_run_terminal_state"));
                }
                if input.state_version != stored_version {
                    return Err(AppError::msg("agent_run_state_version_conflict"));
                }
                let completed = transition_to(state, RunState::Completed)
                    .map_err(|_| AppError::msg("agent_run_illegal_transition"))?;
                ensure_evidence_ids_belong_to_session(conn, session_id, &input.evidence_ids)?;
                let now = chrono::Utc::now().to_rfc3339();
                let seq: i64 = conn.query_row(
                    "SELECT COALESCE(MAX(seq), 0) + 1 FROM session_messages WHERE session_id = ?1",
                    [session_id],
                    |row| row.get(0),
                )?;
                conn.execute(
                    "INSERT INTO session_messages
                     (session_id, seq, role, content, content_hash, created_at, turn_id,
                      evidence_refs_json, citation_map_json)
                     VALUES (?1, ?2, 'assistant', ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        session_id,
                        seq,
                        input.content,
                        crate::cas::hash::content_hash_str(&input.content),
                        now,
                        turn_id,
                        serde_json::to_string(&input.evidence_ids)?,
                        serde_json::to_string(&input.citation_map)?,
                    ],
                )?;
                let message_id = conn.last_insert_rowid().to_string();
                let next_version = stored_version + 1;
                let updated = conn.execute(
                    "UPDATE agent_runs
                     SET status = ?1, state_version = ?2, updated_at = ?3, completed_at = ?3
                     WHERE run_id = ?4 AND state_version = ?5",
                    rusqlite::params![
                        enum_wire(&completed)?,
                        next_version,
                        now,
                        input.run_id,
                        stored_version,
                    ],
                )?;
                if updated != 1 { return Err(AppError::msg("agent_run_state_version_conflict")); }
                let event_seq: u64 = conn.query_row(
                    "SELECT COALESCE(MAX(event_seq), 0) + 1 FROM agent_run_events WHERE run_id = ?1",
                    [&input.run_id], |row| row.get(0),
                )?;
                let event = AssistantRunEvent::new(
                    &input.run_id, event_seq, next_version, RunEventType::Completed, &now,
                    RunEventPayload::Completed { message_id: Some(message_id.clone()) },
                ).map_err(AppError::msg)?;
                insert_event(conn, &event)?;
                conn.execute("UPDATE sessions SET updated_at = ?1 WHERE id = ?2", rusqlite::params![now, session_id])?;
                Ok(message_id)
            })
        })
    }

    /// Persist one immutable confirmation plan for its owning Run.
    pub(crate) fn save_frozen_confirmation(
        db: &Database,
        plan: &crate::ai_runtime::frozen_change_plan::FrozenChangePlan,
    ) -> AppResult<()> {
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM agent_runs
                     WHERE run_id = ?1 AND session_id = ?2",
                    rusqlite::params![plan.run_id(), plan.session_id()],
                    |row| row.get(0),
                )?;
                if count != 1 {
                    return Err(AppError::msg("agent_run_session_not_found"));
                }
                let pending_count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM agent_run_confirmations
                     WHERE run_id = ?1 AND status = 'pending'",
                    [plan.run_id()],
                    |row| row.get(0),
                )?;
                if pending_count != 0 {
                    return Err(AppError::msg("agent_run_confirmation_pending"));
                }
                let now = chrono::Utc::now().to_rfc3339();
                conn.execute(
                    "INSERT INTO agent_run_confirmations
                     (confirmation_id, run_id, plan_hash, plan_json, expires_at, status, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
                    rusqlite::params![
                        plan.confirmation_id(),
                        plan.run_id(),
                        plan.plan_hash(),
                        plan.persisted_plan_json()?,
                        plan.expires_at_unix_ms(),
                        now,
                    ],
                )?;
                Ok(())
            })
        })
    }

    /// Atomically consume exactly one unexpired plan with its original hash.
    pub(crate) fn consume_frozen_confirmation(
        db: &Database,
        run_id: &str,
        confirmation_id: &str,
        plan_hash: &str,
        now_unix_ms: i64,
    ) -> AppResult<()> {
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                let now = chrono::Utc::now().to_rfc3339();
                let updated = conn.execute(
                    "UPDATE agent_run_confirmations
                     SET status = 'consumed', consumed_at = ?1
                     WHERE confirmation_id = ?2 AND run_id = ?3 AND plan_hash = ?4
                       AND status = 'pending' AND expires_at >= ?5",
                    rusqlite::params![now, confirmation_id, run_id, plan_hash, now_unix_ms],
                )?;
                if updated != 1 {
                    return Err(AppError::msg("agent_run_confirmation_expired"));
                }
                Ok(())
            })
        })
    }

    /// Consume an exact pending plan and resume its Run in one transaction.
    pub(crate) fn approve_frozen_confirmation(
        db: &Database,
        session_key: &str,
        run_id: &str,
        confirmation_id: &str,
        plan_hash: &str,
        expected_state_version: u64,
        now_unix_ms: i64,
    ) -> AppResult<FrozenConfirmationApproval> {
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                let (status, stored_state_version): (String, u64) = conn
                    .query_row(
                        "SELECT r.status, r.state_version
                         FROM agent_runs r JOIN sessions s ON s.id = r.session_id
                         WHERE r.run_id = ?1 AND s.session_key = ?2",
                        rusqlite::params![run_id, session_key],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(not_found_or_db)?;
                let confirmation_status: String = conn
                    .query_row(
                        "SELECT status FROM agent_run_confirmations
                         WHERE confirmation_id = ?1 AND run_id = ?2 AND plan_hash = ?3",
                        rusqlite::params![confirmation_id, run_id, plan_hash],
                        |row| row.get(0),
                    )
                    .map_err(|error| match error {
                        rusqlite::Error::QueryReturnedNoRows => {
                            AppError::msg("agent_run_confirmation_expired")
                        }
                        other => other.into(),
                    })?;
                if confirmation_status == "consumed" {
                    return Ok(FrozenConfirmationApproval::AlreadyApplied);
                }
                if confirmation_status != "pending" {
                    return Err(AppError::msg("agent_run_confirmation_expired"));
                }
                if stored_state_version != expected_state_version {
                    return Err(AppError::msg("agent_run_state_version_conflict"));
                }
                if parse_wire::<RunState>(&status)? != RunState::AwaitingConfirmation {
                    return Err(AppError::msg("agent_run_illegal_transition"));
                }
                let now = chrono::Utc::now().to_rfc3339();
                let consumed = conn.execute(
                    "UPDATE agent_run_confirmations
                     SET status = 'consumed', consumed_at = ?1
                     WHERE confirmation_id = ?2 AND run_id = ?3 AND plan_hash = ?4
                       AND status = 'pending' AND expires_at >= ?5",
                    rusqlite::params![now, confirmation_id, run_id, plan_hash, now_unix_ms],
                )?;
                if consumed != 1 {
                    return Err(AppError::msg("agent_run_confirmation_expired"));
                }
                let next_state_version = stored_state_version + 1;
                let updated = conn.execute(
                    "UPDATE agent_runs
                     SET status = 'running', state_version = ?1, updated_at = ?2
                     WHERE run_id = ?3 AND state_version = ?4",
                    rusqlite::params![next_state_version, now, run_id, stored_state_version],
                )?;
                if updated != 1 {
                    return Err(AppError::msg("agent_run_state_version_conflict"));
                }
                let event_seq: u64 = conn.query_row(
                    "SELECT COALESCE(MAX(event_seq), 0) + 1
                     FROM agent_run_events WHERE run_id = ?1",
                    [run_id],
                    |row| row.get(0),
                )?;
                let event = AssistantRunEvent::new(
                    run_id,
                    event_seq,
                    next_state_version,
                    RunEventType::Resumed,
                    &now,
                    RunEventPayload::Resumed {
                        reason: "已确认变更计划，正在继续处理".to_string(),
                    },
                )
                .map_err(AppError::msg)?;
                insert_event(conn, &event)?;
                Ok(FrozenConfirmationApproval::Resumed(event))
            })
        })
    }

    /// Return only the safe Run snapshot and ordered persisted events.
    pub(crate) fn get(db: &Database, run_id: &str) -> AppResult<Option<AssistantRunGetResponse>> {
        Self::get_scoped(db, run_id, None)
    }

    fn get_scoped(
        db: &Database,
        run_id: &str,
        session_key: Option<&str>,
    ) -> AppResult<Option<AssistantRunGetResponse>> {
        db.with_read_conn(|conn| {
            let run = conn.query_row(
                "SELECT r.run_id, r.turn_id, s.session_key, r.status, r.state_version,
                        (SELECT m.id FROM session_messages m
                         WHERE m.session_id = r.session_id AND m.turn_id = r.turn_id
                           AND m.role = 'assistant'
                         ORDER BY m.seq DESC LIMIT 1)
                 FROM agent_runs r JOIN sessions s ON s.id = r.session_id
                 WHERE r.run_id = ?1 AND (?2 IS NULL OR s.session_key = ?2)",
                rusqlite::params![run_id, session_key],
                |row| {
                    let status: String = row.get(3)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        status,
                        row.get::<_, u64>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                    ))
                },
            );
            let (run_id, turn_id, session_key, status, state_version, final_message_id) = match run
            {
                Ok(run) => run,
                Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            let state = parse_wire::<RunState>(&status)?;
            let mut statement = conn.prepare(
                "SELECT event_seq, state_version, event_type, payload_json, created_at
                 FROM agent_run_events WHERE run_id = ?1 ORDER BY event_seq ASC",
            )?;
            let events = statement
                .query_map([&run_id], |row| {
                    let event_type: String = row.get(2)?;
                    let payload_json: String = row.get(3)?;
                    Ok((
                        row.get::<_, u64>(0)?,
                        row.get::<_, u64>(1)?,
                        event_type,
                        payload_json,
                        row.get::<_, String>(4)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(
                    |(seq, event_version, event_type, payload_json, timestamp)| {
                        AssistantRunEvent::new(
                            &run_id,
                            seq,
                            event_version,
                            parse_wire::<RunEventType>(&event_type)?,
                            timestamp,
                            serde_json::from_str(&payload_json)?,
                        )
                        .map_err(AppError::msg)
                    },
                )
                .collect::<AppResult<Vec<_>>>()?;
            let pending_confirmation = pending_confirmation_summary(conn, &run_id, state)?;
            Ok(Some(AssistantRunGetResponse {
                run: AssistantRunSnapshot {
                    run_id,
                    turn_id,
                    session: AssistantSessionRef {
                        domain: SecurityDomain::Normal,
                        session_key,
                    },
                    state,
                    state_version,
                    final_message_id: final_message_id.map(|id| id.to_string()),
                    pending_confirmation,
                    recovery: None,
                },
                events,
            }))
        })
    }

    /// Read a Run only when its opaque normal-domain session key matches.
    pub(crate) fn get_for_session(
        db: &Database,
        session_key: &str,
        run_id: &str,
    ) -> AppResult<Option<AssistantRunGetResponse>> {
        Self::get_scoped(db, run_id, Some(session_key))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedExplicitReference<'a> {
    id: &'a str,
    kind: ContextReferenceKind,
    file_path: Option<&'a str>,
    content_hash: Option<&'a str>,
    utf8_range: Option<&'a SourceSpan>,
    editor_range: Option<&'a EditorRangeWire>,
    heading_path: Option<&'a str>,
    anchor: Option<&'a str>,
    stale: bool,
    invalid_reason: Option<&'a str>,
}

impl<'a> From<&'a ContextReferenceWire> for PersistedExplicitReference<'a> {
    fn from(reference: &'a ContextReferenceWire) -> Self {
        Self {
            id: &reference.id,
            kind: reference.kind,
            file_path: reference.file_path.as_deref(),
            content_hash: reference.content_hash.as_deref(),
            utf8_range: reference.utf8_range.as_ref(),
            editor_range: reference.editor_range.as_ref(),
            heading_path: reference.heading_path.as_deref(),
            anchor: reference.anchor.as_deref(),
            stale: reference.stale,
            invalid_reason: reference.invalid_reason.as_deref(),
        }
    }
}

fn in_immediate_transaction<T>(
    conn: &Connection,
    operation: impl FnOnce(&Connection) -> AppResult<T>,
) -> AppResult<T> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    match operation(conn) {
        Ok(value) => match conn.execute_batch("COMMIT") {
            Ok(()) => Ok(value),
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(error.into())
            }
        },
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn ensure_normal_session(conn: &Connection, session_id: i64, session_key: &str) -> AppResult<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions
         WHERE id = ?1 AND session_key = ?2 AND scene = '' AND note_path IS NULL",
        rusqlite::params![session_id, session_key],
        |row| row.get(0),
    )?;
    if count == 1 {
        Ok(())
    } else {
        Err(AppError::msg("agent_run_session_not_found"))
    }
}

fn pending_confirmation_summary(
    conn: &Connection,
    run_id: &str,
    state: RunState,
) -> AppResult<Option<crate::ai_runtime::run_contract::PendingConfirmationSummary>> {
    if state != RunState::AwaitingConfirmation {
        return Ok(None);
    }
    let confirmation_id = conn
        .query_row(
            "SELECT confirmation_id FROM agent_run_confirmations
             WHERE run_id = ?1 AND status = 'pending'
             ORDER BY created_at DESC LIMIT 1",
            [run_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::msg("agent_run_confirmation_missing"),
            other => other.into(),
        })?;
    let payload_json: String = conn.query_row(
        "SELECT payload_json FROM agent_run_events
             WHERE run_id = ?1 AND event_type = 'confirmation_required'
             ORDER BY event_seq DESC LIMIT 1",
        [run_id],
        |row| row.get(0),
    )?;
    let payload: RunEventPayload = serde_json::from_str(&payload_json)?;
    match payload {
        RunEventPayload::ConfirmationRequired {
            confirmation_id: event_confirmation_id,
            summary,
            ..
        } if event_confirmation_id == confirmation_id => Ok(Some(
            crate::ai_runtime::run_contract::PendingConfirmationSummary {
                confirmation_id,
                summary,
            },
        )),
        _ => Err(AppError::msg("agent_run_confirmation_missing")),
    }
}

fn accepted_for_client_request(
    conn: &Connection,
    client_request_id: &str,
) -> AppResult<Option<AssistantRunAccepted>> {
    let result = conn.query_row(
        "SELECT r.run_id, r.turn_id, s.session_key, r.status, r.state_version
         FROM agent_runs r JOIN sessions s ON s.id = r.session_id
         WHERE r.client_request_id = ?1",
        [client_request_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, u64>(4)?,
            ))
        },
    );
    match result {
        Ok((run_id, turn_id, session_key, status, state_version)) => {
            Ok(Some(AssistantRunAccepted {
                run_id,
                turn_id,
                session: AssistantSessionRef {
                    domain: SecurityDomain::Normal,
                    session_key,
                },
                state: parse_wire::<RunState>(&status)?,
                state_version,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn insert_event(conn: &Connection, event: &AssistantRunEvent) -> AppResult<()> {
    let serialized = serde_json::to_value(event)?;
    conn.execute(
        "INSERT INTO agent_run_events
         (run_id, event_seq, state_version, event_type, payload_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            serialized["runId"]
                .as_str()
                .ok_or_else(|| AppError::msg("agent_run_invalid_event"))?,
            serialized["seq"]
                .as_u64()
                .ok_or_else(|| AppError::msg("agent_run_invalid_event"))?,
            serialized["stateVersion"]
                .as_u64()
                .ok_or_else(|| AppError::msg("agent_run_invalid_event"))?,
            serialized["type"]
                .as_str()
                .ok_or_else(|| AppError::msg("agent_run_invalid_event"))?,
            serde_json::to_string(&serialized["payload"])?,
            serialized["timestamp"]
                .as_str()
                .ok_or_else(|| AppError::msg("agent_run_invalid_event"))?,
        ],
    )?;
    Ok(())
}

fn enum_wire<T: Serialize>(value: &T) -> AppResult<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::msg("agent_run_invalid_enum"))
}

fn parse_wire<T: serde::de::DeserializeOwned>(value: &str) -> AppResult<T> {
    serde_json::from_value(Value::String(value.to_owned())).map_err(AppError::from)
}

fn safe_body_summary(body: &str) -> String {
    let hash = Sha256::digest(body.as_bytes());
    format!(
        "chars={} sha256={}",
        body.chars().count(),
        hex::encode(&hash[..8])
    )
}

fn validate_safe_event_payload(payload: &RunEventPayload) -> AppResult<()> {
    let payload_json = serde_json::to_string(payload)?;
    if payload_json.chars().count() > MAX_SAFE_EVENT_TEXT_CHARS {
        return Err(AppError::msg("agent_run_event_payload_too_large"));
    }
    Ok(())
}

fn state_for_event(payload: &RunEventPayload) -> Option<RunState> {
    match payload {
        RunEventPayload::StageChanged { state, .. } => Some(*state),
        RunEventPayload::ConfirmationRequired { .. } => Some(RunState::AwaitingConfirmation),
        RunEventPayload::Paused { .. } => Some(RunState::Paused),
        RunEventPayload::Resumed { .. } => Some(RunState::Running),
        RunEventPayload::Completed { .. } => Some(RunState::Completed),
        RunEventPayload::Failed { .. } => Some(RunState::Failed),
        RunEventPayload::Cancelled { .. } => Some(RunState::Cancelled),
        RunEventPayload::Accepted { .. }
        | RunEventPayload::ContentDelta { .. }
        | RunEventPayload::ToolStarted { .. }
        | RunEventPayload::ToolCompleted { .. }
        | RunEventPayload::PermissionDenied { .. }
        | RunEventPayload::ProviderSwitched { .. }
        | RunEventPayload::EvidenceRegistered { .. } => None,
    }
}

fn validate_tool_call_lifecycle(
    conn: &Connection,
    run_id: &str,
    payload: &RunEventPayload,
) -> AppResult<()> {
    let (tool_call_id, started) = match payload {
        RunEventPayload::ToolStarted { tool_call_id, .. } => (tool_call_id, true),
        RunEventPayload::ToolCompleted { tool_call_id, .. } => (tool_call_id, false),
        _ => return Ok(()),
    };
    let mut statement = conn.prepare(
        "SELECT payload_json FROM agent_run_events
         WHERE run_id = ?1 AND event_type IN ('tool_started', 'tool_completed')",
    )?;
    let events = statement
        .query_map([run_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut saw_start = false;
    let mut saw_completion = false;
    for event_json in events {
        match serde_json::from_str::<RunEventPayload>(&event_json)? {
            RunEventPayload::ToolStarted {
                tool_call_id: existing,
                ..
            } if existing == tool_call_id.as_str() => saw_start = true,
            RunEventPayload::ToolCompleted {
                tool_call_id: existing,
                ..
            } if existing == tool_call_id.as_str() => saw_completion = true,
            _ => {}
        }
    }
    if started && saw_start {
        return Err(AppError::msg("agent_run_duplicate_tool_call_id"));
    }
    if !started && (!saw_start || saw_completion) {
        return Err(AppError::msg("agent_run_unknown_tool_call_id"));
    }
    Ok(())
}

fn validate_checkpoint_step_input(input: &AppendRunCheckpointInput) -> AppResult<()> {
    for value in [&input.kind, &input.status] {
        if value.trim().is_empty() || value.chars().count() > 120 {
            return Err(AppError::msg("agent_run_invalid_checkpoint_step"));
        }
    }
    for summary in [&input.input_summary, &input.output_summary] {
        if !is_safe_checkpoint_summary(summary) {
            return Err(AppError::msg("agent_run_invalid_checkpoint_step"));
        }
    }
    Ok(())
}

fn is_safe_checkpoint_summary(summary: &str) -> bool {
    if summary.chars().count() > MAX_SAFE_EVENT_TEXT_CHARS || summary.lines().count() > 3 {
        return false;
    }
    let normalized = summary.to_ascii_lowercase();
    ![
        "authorization:",
        "bearer ",
        "api_key",
        "api key",
        "access token",
        "refresh token",
        "password",
        "secret",
        "system prompt",
        "user prompt",
        "-----begin",
    ]
    .iter()
    .any(|forbidden| normalized.contains(forbidden))
}

fn ensure_evidence_ids_belong_to_session(
    conn: &Connection,
    session_id: i64,
    evidence_ids: &[i64],
) -> AppResult<()> {
    for evidence_id in evidence_ids {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM session_evidence WHERE id = ?1 AND session_id = ?2",
            rusqlite::params![evidence_id, session_id],
            |row| row.get(0),
        )?;
        if count != 1 {
            return Err(AppError::msg("agent_run_evidence_not_found"));
        }
    }
    Ok(())
}

fn validate_checkpoint_schema(checkpoint: &Value) -> AppResult<Vec<i64>> {
    let object = checkpoint
        .as_object()
        .ok_or_else(|| AppError::msg("agent_run_checkpoint_invalid_schema"))?;
    const REQUIRED_FIELDS: [&str; 11] = [
        "schemaVersion",
        "executor",
        "goalSummary",
        "completedStepIds",
        "pendingStepId",
        "evidenceIds",
        "requiredCapabilities",
        "requiredPermissions",
        "pendingConfirmationId",
        "budgetRemaining",
        "safeState",
    ];
    if object.len() != REQUIRED_FIELDS.len()
        || REQUIRED_FIELDS
            .iter()
            .any(|field| !object.contains_key(*field))
        || object
            .keys()
            .any(|field| !REQUIRED_FIELDS.contains(&field.as_str()))
        || object.get("schemaVersion").and_then(Value::as_u64) != Some(1)
    {
        return Err(AppError::msg("agent_run_checkpoint_invalid_schema"));
    }
    validate_checkpoint_string(object, "executor", false)?;
    validate_checkpoint_string(object, "goalSummary", false)?;
    validate_checkpoint_optional_string(object, "pendingStepId")?;
    validate_checkpoint_optional_string(object, "pendingConfirmationId")?;
    validate_checkpoint_string_array(object, "completedStepIds")?;
    let evidence_ids = validate_checkpoint_evidence_id_array(object, "evidenceIds")?;
    validate_checkpoint_string_array(object, "requiredCapabilities")?;
    validate_checkpoint_string_array(object, "requiredPermissions")?;

    let budget = object
        .get("budgetRemaining")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::msg("agent_run_checkpoint_invalid_schema"))?;
    if budget.len() != 2
        || budget.get("modelCalls").and_then(Value::as_u64).is_none()
        || budget.get("toolCalls").and_then(Value::as_u64).is_none()
    {
        return Err(AppError::msg("agent_run_checkpoint_invalid_schema"));
    }

    let safe_state = object
        .get("safeState")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::msg("agent_run_checkpoint_invalid_schema"))?;
    validate_checkpoint_safe_value(&Value::Object(safe_state.clone()), 0)?;
    Ok(evidence_ids)
}

fn validate_checkpoint_string(
    object: &serde_json::Map<String, Value>,
    field: &str,
    optional: bool,
) -> AppResult<()> {
    let Some(value) = object.get(field) else {
        return Err(AppError::msg("agent_run_checkpoint_invalid_schema"));
    };
    if optional && value.is_null() {
        return Ok(());
    }
    let Some(text) = value.as_str() else {
        return Err(AppError::msg("agent_run_checkpoint_invalid_schema"));
    };
    if (!optional && text.trim().is_empty()) || text.chars().count() > MAX_CHECKPOINT_STRING_CHARS {
        return Err(AppError::msg("agent_run_checkpoint_invalid_schema"));
    }
    Ok(())
}

fn validate_checkpoint_optional_string(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> AppResult<()> {
    validate_checkpoint_string(object, field, true)
}

fn validate_checkpoint_string_array(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> AppResult<Vec<String>> {
    let items = object
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::msg("agent_run_checkpoint_invalid_schema"))?;
    if items.len() > MAX_CHECKPOINT_SAFE_STATE_ITEMS {
        return Err(AppError::msg("agent_run_checkpoint_invalid_schema"));
    }
    items
        .iter()
        .map(|item| {
            let Some(value) = item.as_str() else {
                return Err(AppError::msg("agent_run_checkpoint_invalid_schema"));
            };
            if value.trim().is_empty() || value.chars().count() > MAX_CHECKPOINT_STRING_CHARS {
                return Err(AppError::msg("agent_run_checkpoint_invalid_schema"));
            }
            Ok(value.to_owned())
        })
        .collect()
}

fn validate_checkpoint_evidence_id_array(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> AppResult<Vec<i64>> {
    let items = object
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::msg("agent_run_checkpoint_invalid_schema"))?;
    if items.len() > MAX_CHECKPOINT_SAFE_STATE_ITEMS {
        return Err(AppError::msg("agent_run_checkpoint_invalid_schema"));
    }
    items
        .iter()
        .map(|item| {
            item.as_i64()
                .filter(|evidence_id| *evidence_id > 0)
                .ok_or_else(|| AppError::msg("agent_run_checkpoint_invalid_schema"))
        })
        .collect()
}

fn validate_checkpoint_safe_value(value: &Value, depth: usize) -> AppResult<()> {
    if depth > MAX_CHECKPOINT_SAFE_STATE_DEPTH {
        return Err(AppError::msg("agent_run_checkpoint_invalid_schema"));
    }
    match value {
        Value::Object(map) => {
            if map.len() > MAX_CHECKPOINT_SAFE_STATE_ITEMS {
                return Err(AppError::msg("agent_run_checkpoint_invalid_schema"));
            }
            for (key, nested) in map {
                let normalized = key.to_ascii_lowercase();
                if [
                    "api",
                    "key",
                    "token",
                    "secret",
                    "password",
                    "authorization",
                    "header",
                    "prompt",
                    "body",
                    "content",
                    "excerpt",
                    "response",
                    "parameter",
                ]
                .iter()
                .any(|unsafe_key| normalized.contains(unsafe_key))
                {
                    return Err(AppError::msg("agent_run_checkpoint_unsafe_key"));
                }
                validate_checkpoint_safe_value(nested, depth + 1)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            if items.len() > MAX_CHECKPOINT_SAFE_STATE_ITEMS {
                return Err(AppError::msg("agent_run_checkpoint_invalid_schema"));
            }
            for item in items {
                validate_checkpoint_safe_value(item, depth + 1)?;
            }
            Ok(())
        }
        Value::String(text) if text.chars().count() <= MAX_CHECKPOINT_STRING_CHARS => Ok(()),
        Value::String(_) => Err(AppError::msg("agent_run_checkpoint_invalid_schema")),
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

fn not_found_or_db(error: rusqlite::Error) -> AppError {
    if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
        AppError::msg("agent_run_not_found")
    } else {
        error.into()
    }
}
