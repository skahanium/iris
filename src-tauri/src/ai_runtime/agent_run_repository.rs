//! SQLite repository for the unified normal-domain Agent Run facts.
//!
//! This module is deliberately storage-only. It does not resolve envelopes,
//! dispatch providers, emit IPC events, or provide a compatibility path for
//! the legacy Harness. Stage 4 owns those responsibilities.

use crate::ai_runtime::prompt_contract::PROMPT_CONTRACT_VERSION;
use crate::ai_runtime::prompt_profile::PromptProfile;
use crate::ai_runtime::run_contract::{
    transition_to, AssistantRunAccepted, AssistantRunEvent, AssistantRunGetResponse,
    AssistantRunSnapshot, AssistantSessionRef, CapabilityId, ConfirmationTargetSummary, Effect,
    Effort, ExecutionEnvelope, ExplicitAction, RiskClass, RunBudgetPolicy, RunEventPayload,
    RunEventType, RunRecoveryKind, RunState, SafeRunErrorCode, SecurityDomain,
};
use crate::ai_types::{
    ContentPart, ContextReferenceKind, ContextReferenceWire, EditorRangeWire, SourceSpan,
};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;
use rusqlite::{params_from_iter, types::Value as SqlValue, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const MAX_SAFE_EVENT_TEXT_CHARS: usize = 2_000;
const MAX_REASONING_SUMMARY_CHARS: usize = 1_500;

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
    /// Immutable local retrieval boundary for this turn.
    pub(crate) context_scope: crate::ai_runtime::retrieval_scope::ContextScopeDto,
    /// Inline presentation annotations for history restoration.
    pub(crate) display_mentions: Vec<crate::ai_runtime::run_contract::DisplayMention>,
    /// Explicit editor action and snapshot scoped to this Run only.
    pub(crate) explicit_action: Option<crate::ai_runtime::run_contract::ExplicitAction>,
    /// Already-resolved execution boundary for this Run.
    pub(crate) envelope: ExecutionEnvelope,
}

/// Durable acceptance result used to distinguish a new Run from an idempotent replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AcceptRunOutcome {
    pub(crate) accepted: AssistantRunAccepted,
    pub(crate) is_new: bool,
}

/// Immutable facts required to create a new attempt from an existing user turn.
#[derive(Debug, Clone)]
pub(crate) struct RetryRunInput {
    pub(crate) session_key: String,
    pub(crate) source_run_id: String,
    pub(crate) client_request_id: String,
    pub(crate) run_id: String,
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

/// Fixed recovery stage for one consumed Durable Apply confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DurableApplyCheckpointStage {
    Approved,
    Dispatching,
    Applied,
    Completed,
}

impl DurableApplyCheckpointStage {
    fn follows(self, previous: Self) -> bool {
        matches!(
            (previous, self),
            (Self::Approved, Self::Dispatching)
                | (Self::Dispatching, Self::Applied)
                | (Self::Applied, Self::Completed)
        )
    }
}

/// Body-free recovery identity for a consumed Durable Apply confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DurableApplyCheckpoint {
    schema_version: u8,
    confirmation_id: String,
    plan_hash: String,
    stage: DurableApplyCheckpointStage,
    base_content_hashes: Vec<String>,
    expected_post_content_hashes: Vec<String>,
    evidence_ids: Vec<i64>,
}

impl DurableApplyCheckpoint {
    /// Construct a bounded checkpoint that contains identities only.
    pub(crate) fn new(
        confirmation_id: impl Into<String>,
        plan_hash: impl Into<String>,
        stage: DurableApplyCheckpointStage,
        base_content_hashes: Vec<String>,
        expected_post_content_hashes: Vec<String>,
        evidence_ids: Vec<i64>,
    ) -> AppResult<Self> {
        let checkpoint = Self {
            schema_version: 1,
            confirmation_id: confirmation_id.into(),
            plan_hash: plan_hash.into(),
            stage,
            base_content_hashes,
            expected_post_content_hashes,
            evidence_ids,
        };
        checkpoint.validate()?;
        Ok(checkpoint)
    }

    fn validate(&self) -> AppResult<()> {
        let safe_identity = |value: &str| {
            !value.trim().is_empty()
                && value.chars().count() <= 256
                && !value.chars().any(char::is_control)
        };
        if self.schema_version != 1
            || !safe_identity(&self.confirmation_id)
            || !safe_identity(&self.plan_hash)
            || self.base_content_hashes.len() != self.expected_post_content_hashes.len()
            || self.base_content_hashes.len() > 32
            || self
                .base_content_hashes
                .iter()
                .chain(&self.expected_post_content_hashes)
                .any(|hash| !safe_identity(hash))
            || self.evidence_ids.len() > 64
            || self
                .evidence_ids
                .iter()
                .any(|evidence_id| *evidence_id <= 0)
        {
            return Err(AppError::run(SafeRunErrorCode::CheckpointInvalidSchema));
        }
        Ok(())
    }

    pub(crate) const fn stage(&self) -> DurableApplyCheckpointStage {
        self.stage
    }

    pub(crate) fn confirmation_id(&self) -> &str {
        &self.confirmation_id
    }

    pub(crate) fn plan_hash(&self) -> &str {
        &self.plan_hash
    }

    pub(crate) fn base_content_hashes(&self) -> &[String] {
        &self.base_content_hashes
    }

    pub(crate) fn expected_post_content_hashes(&self) -> &[String] {
        &self.expected_post_content_hashes
    }
}

/// One durable checkpoint append for a recoverable Apply step.
#[derive(Debug, Clone)]
pub(crate) struct AppendRunCheckpointInput {
    /// Owning Run identifier.
    pub(crate) run_id: String,
    /// Version the Run Engine observed before persisting the checkpoint.
    pub(crate) state_version: u64,
    /// Versioned body-free recovery data.
    pub(crate) checkpoint: DurableApplyCheckpoint,
}

/// Facts that must commit with a Run's successful terminal transition.
#[derive(Debug, Clone)]
pub(crate) struct FinalizeRunInput {
    pub(crate) run_id: String,
    pub(crate) state_version: u64,
    pub(crate) content: String,
    pub(crate) evidence_ids: Vec<i64>,
    pub(crate) citation_map: Value,
    pub(crate) source_summary: Vec<crate::ai_runtime::provenance::SourceSummaryEntry>,
}

/// Safe process-event history for one latest Run belonging to a logical turn.
#[derive(Debug, Clone)]
pub(crate) struct HistoricalRunProcess {
    /// Stable Run identity used to bind the process view to an assistant message.
    pub(crate) run_id: String,
    /// Only safe progress events; answer deltas are deliberately excluded.
    pub(crate) events: Vec<AssistantRunEvent>,
}

/// Result of consuming a persisted confirmation through one idempotent control request.
pub(crate) enum FrozenConfirmationApproval {
    /// The pending plan was consumed and the Run durably resumed.
    Resumed(AssistantRunEvent),
    /// The same plan had already been consumed by an earlier identical control request.
    AlreadyApplied,
}

/// Result of rejecting a persisted confirmation through one idempotent control request.
pub(crate) enum FrozenConfirmationRejection {
    /// The pending plan was rejected and the Run durably cancelled.
    Cancelled(AssistantRunEvent),
    /// The same plan had already been rejected by an earlier identical control request.
    AlreadyRejected,
}

/// Exact consumed confirmation data that may enter the post-approval executor.
///
/// This is intentionally storage-shaped and never crosses the IPC boundary.
#[derive(Debug, Clone)]
pub(crate) struct ConsumedFrozenConfirmation {
    pub(crate) plan_hash: String,
    pub(crate) plan_json: String,
}
/// Repository for normal-domain Run, Event and intake facts.
pub(crate) struct AgentRunRepository;

impl AgentRunRepository {
    /// Atomically persist the accepted user Turn, Run and first event.
    ///
    /// A repeated `client_request_id` returns the original accepted identity
    /// without adding another user message or event.
    #[allow(
        dead_code,
        reason = "test fixtures accept Runs without external grants"
    )]
    pub(crate) fn accept(db: &Database, input: AcceptRunInput) -> AppResult<AssistantRunAccepted> {
        Self::accept_with_external_grants(db, input, &[])
    }

    /// Atomically persist the accepted Run and its explicit MCP snapshots.
    pub(crate) fn accept_with_external_grants(
        db: &Database,
        input: AcceptRunInput,
        external_tool_grants: &[crate::ai_runtime::run_contract::ExternalToolGrantRef],
    ) -> AppResult<AssistantRunAccepted> {
        Self::accept_with_external_grants_outcome(db, input, external_tool_grants, false)
            .map(|outcome| outcome.accepted)
    }

    /// Atomically accept a Run and report whether this call created it.
    ///
    /// `create_session` reserves session creation for the same transaction as
    /// Run acceptance. This keeps a response-loss replay from creating an
    /// orphan session or changing the idempotency fingerprint.
    pub(crate) fn accept_with_external_grants_outcome(
        db: &Database,
        input: AcceptRunInput,
        external_tool_grants: &[crate::ai_runtime::run_contract::ExternalToolGrantRef],
        create_session: bool,
    ) -> AppResult<AcceptRunOutcome> {
        if input.envelope.security_domain != SecurityDomain::Normal {
            return Err(AppError::run(
                SafeRunErrorCode::ClassifiedDomainNotSupported,
            ));
        }
        let intake_fingerprint = intake_fingerprint(&input, external_tool_grants, create_session)?;
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                if let Some((existing, stored_fingerprint)) =
                    accepted_for_client_request(conn, &input.client_request_id)?
                {
                    if stored_fingerprint.is_some_and(|stored| stored != intake_fingerprint) {
                        return Err(AppError::run(SafeRunErrorCode::IdempotencyConflict));
                    }
                    return Ok(AcceptRunOutcome {
                        accepted: existing,
                        is_new: false,
                    });
                }

                let now = chrono::Utc::now().to_rfc3339();
                let (session_id, session_key) = if create_session {
                    let session_key = format!("run_session:{}", uuid::Uuid::new_v4());
                    conn.execute(
                        "INSERT INTO sessions (session_key, created_at, updated_at)
                         VALUES (?1, ?2, ?2)",
                        rusqlite::params![session_key, now],
                    )?;
                    (conn.last_insert_rowid(), session_key)
                } else {
                    ensure_normal_session(conn, input.session_id, &input.session_key)?;
                    ensure_no_active_top_level_run(conn, input.session_id)?;
                    (input.session_id, input.session_key.clone())
                };
                let (prompt_profile_snapshot_json, prompt_contract_version, prompt_contract_hash) =
                    load_prompt_contract_snapshot(conn)?;
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
                let context_scope_json = serde_json::to_string(&input.context_scope)?;
                let display_mentions_json = serde_json::to_string(&input.display_mentions)?;
                let envelope_json = serde_json::to_string(&input.envelope)?;
                let budget_policy_json =
                    serde_json::to_string(&RunBudgetPolicy::for_envelope(&input.envelope))?;
                let explicit_action_json = input
                    .explicit_action
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?;
                let effect = enum_wire(&input.envelope.effect)?;
                let effort = enum_wire(&input.envelope.effort)?;
                let security_domain = enum_wire(&input.envelope.security_domain)?;
                let risk = enum_wire(&input.envelope.risk)?;
                let message_hash = crate::cas::hash::content_hash_str(&input.message);
                let goal_summary = safe_body_summary(&input.message);

                let seq: i64 = conn.query_row(
                    "SELECT COALESCE(MAX(seq), 0) + 1 FROM session_messages WHERE session_id = ?1",
                    [session_id],
                    |row| row.get(0),
                )?;
                conn.execute(
                    "INSERT INTO session_messages
                 (session_id, seq, role, content, content_parts, content_hash, created_at,
                  turn_id, explicit_references_json, context_scope_json, display_mentions_json)
                 VALUES (?1, ?2, 'user', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        session_id,
                        seq,
                        input.message,
                        content_parts_json,
                        message_hash,
                        now,
                        input.turn_id,
                        explicit_references_json,
                        context_scope_json,
                        display_mentions_json,
                    ],
                )?;
                conn.execute(
                    "INSERT INTO agent_runs
                 (run_id, client_request_id, session_id, turn_id, status, state_version,
                  effect, effort, security_domain, risk, envelope_json, explicit_action_json,
                  goal_summary, budget_policy_json, intake_fingerprint, prompt_profile_snapshot_json,
                  prompt_contract_version, prompt_contract_hash, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'accepted', 0, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17)",
                    rusqlite::params![
                        input.run_id,
                        input.client_request_id,
                        session_id,
                        input.turn_id,
                        effect,
                        effort,
                        security_domain,
                        risk,
                        envelope_json,
                        explicit_action_json,
                        goal_summary,
                        budget_policy_json,
                        intake_fingerprint,
                        prompt_profile_snapshot_json,
                        prompt_contract_version,
                        prompt_contract_hash,
                        now,
                    ],
                )?;
                crate::ai_runtime::mcp_external_tools::freeze_run_grants(
                    conn,
                    &input.run_id,
                    external_tool_grants,
                )?;
                let event = AssistantRunEvent::new(
                    &input.run_id,
                    1,
                    0,
                    RunEventType::Accepted,
                    &now,
                    RunEventPayload::Accepted {
                        turn_id: input.turn_id.clone(),
                        session_key: session_key.clone(),
                        freshness: Some(input.envelope.freshness),
                        web_reason: Some(input.envelope.web_reason),
                    },
                )
                .map_err(AppError::msg)?;
                insert_event(conn, &event)?;
                conn.execute(
                    "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, session_id],
                )?;

                Ok(AcceptRunOutcome {
                    accepted: AssistantRunAccepted {
                        client_request_id: input.client_request_id,
                        run_id: input.run_id,
                        turn_id: input.turn_id,
                        session: AssistantSessionRef {
                            domain: SecurityDomain::Normal,
                            session_key,
                        },
                        state: RunState::Accepted,
                        state_version: 0,
                    },
                    is_new: true,
                })
            })
        })
    }

    /// Atomically create a retry Run for the same most-recent failed user turn.
    ///
    /// This deliberately does not insert a second `session_messages` record.
    /// A newer visible message makes the historical failure ineligible, so a
    /// late provider response can never be inserted into a newer conversation.
    #[cfg(test)]
    pub(crate) fn accept_retry(
        db: &Database,
        input: RetryRunInput,
    ) -> AppResult<AssistantRunAccepted> {
        Self::accept_retry_outcome(db, input).map(|outcome| outcome.accepted)
    }

    /// Accept a retry and report whether this call created the Run.
    ///
    /// Repeated retries with the same `client_request_id` return the original
    /// identity with `is_new=false`; only the first caller may start an
    /// executor or emit a second accepted notification.
    pub(crate) fn accept_retry_outcome(
        db: &Database,
        input: RetryRunInput,
    ) -> AppResult<AcceptRunOutcome> {
        let intake_fingerprint = retry_intake_fingerprint(&input)?;
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                if let Some((existing, stored_fingerprint)) =
                    accepted_for_client_request(conn, &input.client_request_id)?
                {
                    if stored_fingerprint.as_deref() != Some(intake_fingerprint.as_str()) {
                        return Err(AppError::run(SafeRunErrorCode::IdempotencyConflict));
                    }
                    return Ok(AcceptRunOutcome {
                        accepted: existing,
                        is_new: false,
                    });
                }
                let session_id = conn
                    .query_row(
                        "SELECT id FROM sessions WHERE session_key = ?1",
                        [&input.session_key],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(|error| match error {
                        rusqlite::Error::QueryReturnedNoRows => {
                            AppError::run(SafeRunErrorCode::SessionNotFound)
                        }
                        other => other.into(),
                    })?;
                ensure_no_active_top_level_run(conn, session_id)?;
                let source = conn
                    .query_row(
                        "SELECT r.session_id, r.turn_id, r.effect, r.effort, r.security_domain, r.risk,
                                r.envelope_json, r.explicit_action_json, r.goal_summary,
                                r.budget_policy_json,
                                r.prompt_profile_snapshot_json, r.prompt_contract_version,
                                r.prompt_contract_hash
                         FROM agent_runs r
                         JOIN sessions s ON s.id = r.session_id
                         WHERE r.run_id = ?1 AND s.session_key = ?2 AND r.status = 'failed'
                           AND r.rowid = (SELECT latest.rowid FROM agent_runs latest
                                          WHERE latest.session_id = r.session_id
                                            AND latest.turn_id = r.turn_id
                                          ORDER BY latest.created_at DESC, latest.rowid DESC LIMIT 1)
                           AND NOT EXISTS (
                               SELECT 1 FROM session_messages later
                               WHERE later.session_id = r.session_id
                                 AND later.seq > (
                                     SELECT MAX(turn_message.seq)
                                     FROM session_messages turn_message
                                     WHERE turn_message.session_id = r.session_id
                                       AND turn_message.turn_id = r.turn_id
                                 )
                           )",
                        rusqlite::params![input.source_run_id, input.session_key],
                        |row| Ok((
                            row.get::<_, i64>(0)?, row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?, row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?, row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?, row.get::<_, Option<String>>(7)?,
                            row.get::<_, String>(8)?, row.get::<_, String>(9)?,
                            row.get::<_, Option<String>>(10)?, row.get::<_, Option<i64>>(11)?,
                            row.get::<_, Option<String>>(12)?,
                        )),
                    )
                    .optional()?;
                let Some((session_id, turn_id, effect, effort, security_domain, risk, envelope_json, explicit_action_json, goal_summary, stored_budget_policy_json, prompt_profile_snapshot_json, prompt_contract_version, prompt_contract_hash)) = source else {
                    return Err(AppError::run(SafeRunErrorCode::RetryNotAvailable));
                };
                let (_, budget_policy_json) =
                    materialize_budget_policy(&stored_budget_policy_json, &envelope_json)?;
                if budget_policy_json != stored_budget_policy_json {
                    conn.execute(
                        "UPDATE agent_runs
                         SET budget_policy_json = ?1
                         WHERE run_id = ?2 AND budget_policy_json = ?3",
                        rusqlite::params![
                            budget_policy_json,
                            input.source_run_id,
                            stored_budget_policy_json
                        ],
                    )?;
                }
                let now = chrono::Utc::now().to_rfc3339();
                conn.execute(
                    "INSERT INTO agent_runs
                     (run_id, client_request_id, session_id, turn_id, status, state_version,
                      effect, effort, security_domain, risk, envelope_json, explicit_action_json,
                      goal_summary, budget_policy_json, intake_fingerprint,
                      prompt_profile_snapshot_json, prompt_contract_version,
                      prompt_contract_hash, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, 'accepted', 0, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?17)",
                    rusqlite::params![input.run_id, input.client_request_id, session_id, turn_id,
                        effect, effort, security_domain, risk, envelope_json, explicit_action_json,
                        goal_summary, budget_policy_json, intake_fingerprint,
                        prompt_profile_snapshot_json, prompt_contract_version,
                        prompt_contract_hash, now],
                )?;
                let envelope: crate::ai_runtime::run_contract::ExecutionEnvelope =
                    serde_json::from_str(&envelope_json)?;
                let event = AssistantRunEvent::new(
                    &input.run_id, 1, 0, RunEventType::Accepted, &now,
                    RunEventPayload::Accepted {
                        turn_id: turn_id.clone(),
                        session_key: input.session_key.clone(),
                        freshness: Some(envelope.freshness),
                        web_reason: Some(envelope.web_reason),
                    },
                ).map_err(AppError::msg)?;
                insert_event(conn, &event)?;
                conn.execute("UPDATE sessions SET updated_at = ?1 WHERE id = ?2", rusqlite::params![now, session_id])?;
                Ok(AcceptRunOutcome {
                    accepted: AssistantRunAccepted {
                        client_request_id: input.client_request_id,
                        run_id: input.run_id,
                        turn_id,
                        session: AssistantSessionRef { domain: SecurityDomain::Normal, session_key: input.session_key },
                        state: RunState::Accepted,
                        state_version: 0,
                    },
                    is_new: true,
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
                    return Err(AppError::run(SafeRunErrorCode::TerminalState));
                }
                if matches!(&input.payload, RunEventPayload::Completed { .. }) {
                    return Err(AppError::msg("agent_run_finalization_required"));
                }
                if input.state_version != stored_state_version {
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
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
                        #[cfg(test)]
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
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
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

    /// Persist the next body-free Durable Apply checkpoint.
    pub(crate) fn append_checkpoint_step(
        db: &Database,
        input: AppendRunCheckpointInput,
    ) -> AppResult<()> {
        input.checkpoint.validate()?;
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
                    return Err(AppError::run(SafeRunErrorCode::TerminalState));
                }
                if input.state_version != stored_state_version {
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
                }
                let effort = parse_wire::<crate::ai_runtime::run_contract::Effort>(&effort)?;
                if effort != crate::ai_runtime::run_contract::Effort::Durable {
                    return Err(AppError::msg("agent_run_checkpoint_not_durable"));
                }
                ensure_evidence_ids_belong_to_session(
                    conn,
                    session_id,
                    &input.checkpoint.evidence_ids,
                )?;
                let latest = latest_durable_apply_checkpoint_in_conn(conn, &input.run_id)?;
                if let Some(latest) = latest {
                    if latest == input.checkpoint {
                        return Ok(());
                    }
                    if latest.confirmation_id != input.checkpoint.confirmation_id
                        || latest.plan_hash != input.checkpoint.plan_hash
                        || latest.base_content_hashes != input.checkpoint.base_content_hashes
                        || latest.expected_post_content_hashes
                            != input.checkpoint.expected_post_content_hashes
                        || !input.checkpoint.stage.follows(latest.stage)
                    {
                        return Err(AppError::run(SafeRunErrorCode::CheckpointStageConflict));
                    }
                } else if input.checkpoint.stage != DurableApplyCheckpointStage::Approved {
                    return Err(AppError::run(SafeRunErrorCode::CheckpointStageConflict));
                }
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
                        "durable_apply",
                        serde_json::to_value(input.checkpoint.stage)?
                            .as_str()
                            .ok_or_else(|| {
                                AppError::run(SafeRunErrorCode::CheckpointInvalidSchema)
                            })?,
                        "",
                        "",
                        serde_json::to_string(&input.checkpoint)?,
                        serde_json::to_string(&input.checkpoint.evidence_ids)?,
                        now,
                    ],
                )?;
                let updated = conn.execute(
                    "UPDATE agent_runs SET updated_at = ?1
                     WHERE run_id = ?2 AND state_version = ?3",
                    rusqlite::params![now, input.run_id, stored_state_version],
                )?;
                if updated != 1 {
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
                }
                Ok(())
            })
        })
    }

    /// Read the latest persisted Durable Apply checkpoint for startup recovery.
    pub(crate) fn latest_durable_apply_checkpoint(
        db: &Database,
        run_id: &str,
    ) -> AppResult<Option<DurableApplyCheckpoint>> {
        db.with_read_conn(|conn| latest_durable_apply_checkpoint_in_conn(conn, run_id))
    }

    /// Bind an executable frozen plan to the exact checkpoint created when its
    /// confirmation was consumed. Only a not-yet-applied stage may dispatch.
    pub(crate) fn validate_durable_apply_checkpoint_binding(
        db: &Database,
        run_id: &str,
        plan: &crate::ai_runtime::frozen_change_plan::FrozenChangePlan,
    ) -> AppResult<()> {
        let checkpoint = Self::latest_durable_apply_checkpoint(db, run_id)?
            .ok_or_else(|| AppError::run(SafeRunErrorCode::ConfirmationExpired))?;
        let base_content_hashes = plan
            .base_content_hashes()
            .iter()
            .map(|(_, hash)| hash.clone())
            .collect::<Vec<_>>();
        let expected_post_content_hashes = plan
            .expected_post_content_hashes()
            .iter()
            .map(|(_, hash)| hash.clone())
            .collect::<Vec<_>>();
        if checkpoint.confirmation_id != plan.confirmation_id()
            || checkpoint.plan_hash != plan.plan_hash()
            || checkpoint.base_content_hashes != base_content_hashes
            || checkpoint.expected_post_content_hashes != expected_post_content_hashes
            || !matches!(
                checkpoint.stage,
                DurableApplyCheckpointStage::Approved | DurableApplyCheckpointStage::Dispatching
            )
        {
            return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
        }
        Ok(())
    }

    /// Atomically persist final output, terminal Run state, and completed event.
    pub(crate) fn finalize(db: &Database, input: FinalizeRunInput) -> AppResult<String> {
        if input.content.trim().is_empty() || input.content.chars().count() > 32_000 {
            return Err(AppError::run(SafeRunErrorCode::InvalidFinalOutput));
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
                    return Err(AppError::run(SafeRunErrorCode::TerminalState));
                }
                if input.state_version != stored_version {
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
                }
                let completed = transition_to(state, RunState::Completed)
                    .map_err(|_| AppError::run(SafeRunErrorCode::IllegalTransition))?;
                ensure_final_evidence_ids_belong_to_run(
                    conn,
                    &input.run_id,
                    session_id,
                    &input.evidence_ids,
                )?;
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
                if updated != 1 { return Err(AppError::run(SafeRunErrorCode::StateVersionConflict)); }
                let event_seq: u64 = conn.query_row(
                    "SELECT COALESCE(MAX(event_seq), 0) + 1 FROM agent_run_events WHERE run_id = ?1",
                    [&input.run_id], |row| row.get(0),
                )?;
                let event = AssistantRunEvent::new(
                    &input.run_id, event_seq, next_version, RunEventType::Completed, &now,
                    RunEventPayload::Completed {
                        message_id: Some(message_id.clone()),
                        source_summary: input.source_summary,
                    },
                ).map_err(AppError::msg)?;
                insert_event(conn, &event)?;
                conn.execute("UPDATE sessions SET updated_at = ?1 WHERE id = ?2", rusqlite::params![now, session_id])?;
                Ok(message_id)
            })
        })
    }

    /// Persist a sanitized partial assistant reply after the user cancelled a live stream.
    ///
    /// This is intentionally separate from [`Self::finalize`]: the Run stays `cancelled`,
    /// and the partial exists only so the next turn can continue from visible history.
    /// Idempotent per turn — a second call for the same turn is a no-op.
    pub(crate) fn persist_interrupted_assistant_message(
        db: &Database,
        run_id: &str,
        content: &str,
    ) -> AppResult<Option<String>> {
        const MIN_INTERRUPTED_CHARS: usize = 20;
        let sanitized = crate::ai_runtime::text_support::sanitize_meta_analysis_prefix(content);
        let trimmed = sanitized.trim();
        if trimmed.chars().count() < MIN_INTERRUPTED_CHARS {
            return Ok(None);
        }
        if trimmed.chars().count() > 32_000 {
            return Err(AppError::run(SafeRunErrorCode::InvalidFinalOutput));
        }
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                let (session_id, turn_id, status): (i64, String, String) = conn
                    .query_row(
                        "SELECT session_id, turn_id, status FROM agent_runs WHERE run_id = ?1",
                        [run_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .map_err(not_found_or_db)?;
                let state = parse_wire::<RunState>(&status)?;
                if state != RunState::Cancelled {
                    return Err(AppError::msg(
                        "agent_run_interrupt_persist_requires_cancelled",
                    ));
                }
                let existing: Option<i64> = conn
                    .query_row(
                        "SELECT id FROM session_messages
                         WHERE session_id = ?1 AND turn_id = ?2 AND role = 'assistant'
                         LIMIT 1",
                        rusqlite::params![session_id, turn_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                if existing.is_some() {
                    return Ok(None);
                }
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
                     VALUES (?1, ?2, 'assistant', ?3, ?4, ?5, ?6, '[]', '{}')",
                    rusqlite::params![
                        session_id,
                        seq,
                        trimmed,
                        crate::cas::hash::content_hash_str(trimmed),
                        now,
                        turn_id,
                    ],
                )?;
                let message_id = conn.last_insert_rowid().to_string();
                conn.execute(
                    "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, session_id],
                )?;
                Ok(Some(message_id))
            })
        })
    }

    /// Whether the latest assistant message before `before_seq` belongs to a cancelled Run.
    pub(crate) fn latest_assistant_before_was_interrupted(
        db: &Database,
        session_id: i64,
        before_seq: i64,
    ) -> AppResult<bool> {
        db.with_read_conn(|conn| {
            let turn_id: Option<String> = conn
                .query_row(
                    "SELECT turn_id FROM session_messages
                     WHERE session_id = ?1 AND seq < ?2 AND role = 'assistant'
                     ORDER BY seq DESC LIMIT 1",
                    rusqlite::params![session_id, before_seq],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(turn_id) = turn_id else {
                return Ok(false);
            };
            let status: Option<String> = conn
                .query_row(
                    "SELECT status FROM agent_runs
                     WHERE session_id = ?1 AND turn_id = ?2
                     ORDER BY created_at DESC LIMIT 1",
                    rusqlite::params![session_id, turn_id],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(status.as_deref() == Some("cancelled"))
        })
    }

    /// Validate final evidence ownership without writing any model output.
    pub(crate) fn validate_final_evidence(
        db: &Database,
        run_id: &str,
        evidence_ids: &[i64],
    ) -> AppResult<()> {
        db.with_read_conn(|conn| {
            let session_id = conn
                .query_row(
                    "SELECT session_id FROM agent_runs WHERE run_id = ?1",
                    [run_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(not_found_or_db)?;
            ensure_final_evidence_ids_belong_to_run(conn, run_id, session_id, evidence_ids)
        })
    }

    /// Persist one immutable confirmation plan for its owning Run.
    #[cfg(test)]
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
                    return Err(AppError::run(SafeRunErrorCode::SessionNotFound));
                }
                let pending_count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM agent_run_confirmations
                     WHERE run_id = ?1 AND status = 'pending'",
                    [plan.run_id()],
                    |row| row.get(0),
                )?;
                if pending_count != 0 {
                    return Err(AppError::run(SafeRunErrorCode::ConfirmationPending));
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

    /// Atomically persist a frozen plan and transition its running Run to the
    /// single matching confirmation event. A process crash can therefore never
    /// leave a pending plan without an awaiting-confirmation Run state.
    pub(crate) fn request_frozen_confirmation(
        db: &Database,
        plan: &crate::ai_runtime::frozen_change_plan::FrozenChangePlan,
        state_version: u64,
        summary: &str,
    ) -> AppResult<AssistantRunEvent> {
        if summary.trim().is_empty() || summary.chars().count() > MAX_SAFE_EVENT_TEXT_CHARS {
            return Err(AppError::run(SafeRunErrorCode::InvalidChangePlan));
        }
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                let (status, stored_version, session_id): (String, u64, i64) = conn
                    .query_row(
                        "SELECT status, state_version, session_id FROM agent_runs WHERE run_id = ?1",
                        [plan.run_id()],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .map_err(not_found_or_db)?;
                if session_id != plan.session_id() {
                    return Err(AppError::run(SafeRunErrorCode::SessionNotFound));
                }
                if parse_wire::<RunState>(&status)? != RunState::Running {
                    return Err(AppError::run(SafeRunErrorCode::IllegalTransition));
                }
                if stored_version != state_version {
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
                }
                let pending_count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM agent_run_confirmations
                     WHERE run_id = ?1 AND status = 'pending'",
                    [plan.run_id()],
                    |row| row.get(0),
                )?;
                if pending_count != 0 {
                    return Err(AppError::run(SafeRunErrorCode::ConfirmationPending));
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
                let next_state = transition_to(RunState::Running, RunState::AwaitingConfirmation)
                    .map_err(|_| AppError::run(SafeRunErrorCode::IllegalTransition))?;
                let next_state_version = stored_version + 1;
                let updated = conn.execute(
                    "UPDATE agent_runs
                     SET status = ?1, state_version = ?2, updated_at = ?3
                     WHERE run_id = ?4 AND state_version = ?5",
                    rusqlite::params![
                        enum_wire(&next_state)?,
                        next_state_version,
                        now,
                        plan.run_id(),
                        stored_version,
                    ],
                )?;
                if updated != 1 {
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
                }
                let event_seq: u64 = conn.query_row(
                    "SELECT COALESCE(MAX(event_seq), 0) + 1
                     FROM agent_run_events WHERE run_id = ?1",
                    [plan.run_id()],
                    |row| row.get(0),
                )?;
                let event = AssistantRunEvent::new(
                    plan.run_id(),
                    event_seq,
                    next_state_version,
                    RunEventType::ConfirmationRequired,
                    &now,
                    RunEventPayload::ConfirmationRequired {
                        confirmation_id: plan.confirmation_id().to_string(),
                        plan_hash: plan.plan_hash().to_string(),
                        summary: summary.to_string(),
                        effect: Some(Effect::Apply),
                        targets: Some(confirmation_targets(plan.relative_paths())),
                        expires_at: chrono::DateTime::from_timestamp_millis(
                            plan.expires_at_unix_ms(),
                        )
                        .map(|timestamp| timestamp.to_rfc3339()),
                    },
                )
                .map_err(AppError::msg)?;
                insert_event(conn, &event)?;
                Ok(event)
            })
        })
    }

    /// Load the exact plan that was atomically consumed by a successful approval.
    /// The session join makes this safe for the caller that owns the Run.
    pub(crate) fn consumed_frozen_confirmation_for_session(
        db: &Database,
        session_key: &str,
        run_id: &str,
        confirmation_id: &str,
    ) -> AppResult<ConsumedFrozenConfirmation> {
        db.with_read_conn(|conn| {
            conn.query_row(
                "SELECT c.plan_hash, c.plan_json
                 FROM agent_run_confirmations c
                 JOIN agent_runs r ON r.run_id = c.run_id
                 JOIN sessions s ON s.id = r.session_id
                 WHERE c.run_id = ?1 AND c.confirmation_id = ?2
                   AND c.status = 'consumed' AND s.session_key = ?3",
                rusqlite::params![run_id, confirmation_id, session_key],
                |row| {
                    Ok(ConsumedFrozenConfirmation {
                        plan_hash: row.get(0)?,
                        plan_json: row.get(1)?,
                    })
                },
            )
            .map_err(not_found_or_db)
        })
    }

    /// Atomically consume exactly one unexpired plan with its original hash.
    #[cfg(test)]
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
                    return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
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
                let (status, stored_state_version, stored_budget_policy_json, envelope_json): (
                    String,
                    u64,
                    String,
                    String,
                ) = conn
                    .query_row(
                        "SELECT r.status, r.state_version, r.budget_policy_json, r.envelope_json
                         FROM agent_runs r JOIN sessions s ON s.id = r.session_id
                         WHERE r.run_id = ?1 AND s.session_key = ?2",
                        rusqlite::params![run_id, session_key],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .map_err(not_found_or_db)?;
                let (confirmation_status, plan_json): (String, String) = conn
                    .query_row(
                        "SELECT status, plan_json FROM agent_run_confirmations
                         WHERE confirmation_id = ?1 AND run_id = ?2 AND plan_hash = ?3",
                        rusqlite::params![confirmation_id, run_id, plan_hash],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(|error| match error {
                        rusqlite::Error::QueryReturnedNoRows => {
                            AppError::run(SafeRunErrorCode::ConfirmationExpired)
                        }
                        other => other.into(),
                    })?;
                if confirmation_status == "consumed" {
                    return Ok(FrozenConfirmationApproval::AlreadyApplied);
                }
                if confirmation_status != "pending" {
                    return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
                }
                if stored_state_version != expected_state_version {
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
                }
                if parse_wire::<RunState>(&status)? != RunState::AwaitingConfirmation {
                    return Err(AppError::run(SafeRunErrorCode::IllegalTransition));
                }
                let (_, normalized_budget_policy_json) =
                    materialize_budget_policy(&stored_budget_policy_json, &envelope_json)?;
                if normalized_budget_policy_json != stored_budget_policy_json {
                    conn.execute(
                        "UPDATE agent_runs
                         SET budget_policy_json = ?1
                         WHERE run_id = ?2 AND budget_policy_json = ?3",
                        rusqlite::params![
                            normalized_budget_policy_json,
                            run_id,
                            stored_budget_policy_json
                        ],
                    )?;
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
                    return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
                }
                let plan =
                    crate::ai_runtime::frozen_change_plan::FrozenChangePlan::from_persisted_plan_json(
                        &plan_json,
                    )?;
                if plan.confirmation_id() != confirmation_id
                    || plan.run_id() != run_id
                    || plan.plan_hash() != plan_hash
                {
                    return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
                }
                let checkpoint = DurableApplyCheckpoint::new(
                    confirmation_id,
                    plan_hash,
                    DurableApplyCheckpointStage::Approved,
                    plan.base_content_hashes()
                        .iter()
                        .map(|(_, hash)| hash.clone())
                        .collect(),
                    plan.expected_post_content_hashes()
                        .iter()
                        .map(|(_, hash)| hash.clone())
                        .collect(),
                    Vec::new(),
                )?;
                let step_seq: i64 = conn.query_row(
                    "SELECT COALESCE(MAX(step_seq), 0) + 1
                     FROM agent_run_steps WHERE run_id = ?1",
                    [run_id],
                    |row| row.get(0),
                )?;
                let next_state_version = stored_state_version + 1;
                let updated = conn.execute(
                    "UPDATE agent_runs
                     SET status = 'running', state_version = ?1, updated_at = ?2
                     WHERE run_id = ?3 AND state_version = ?4",
                    rusqlite::params![next_state_version, now, run_id, stored_state_version],
                )?;
                if updated != 1 {
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
                }
                conn.execute(
                    "INSERT INTO agent_run_steps
                     (run_id, step_seq, kind, status, input_summary, output_summary,
                      resume_state_json, evidence_refs_json, created_at, updated_at)
                     VALUES (?1, ?2, 'durable_apply', 'approved', '', '', ?3, '[]', ?4, ?4)",
                    rusqlite::params![
                        run_id,
                        step_seq,
                        serde_json::to_string(&checkpoint)?,
                        now,
                    ],
                )?;
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

    /// Resume one startup-classified Durable Apply with optimistic concurrency.
    pub(crate) fn resume_durable_apply(
        db: &Database,
        session_key: &str,
        run_id: &str,
        expected_state_version: u64,
    ) -> AppResult<(AssistantRunEvent, String)> {
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                let (status, stored_state_version, effort, effect): (String, u64, String, String) =
                    conn.query_row(
                        "SELECT r.status, r.state_version, r.effort, r.effect
                         FROM agent_runs r
                         JOIN sessions s ON s.id = r.session_id
                         WHERE r.run_id = ?1 AND s.session_key = ?2",
                        rusqlite::params![run_id, session_key],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .map_err(not_found_or_db)?;
                if stored_state_version != expected_state_version {
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
                }
                if parse_wire::<RunState>(&status)? != RunState::Paused
                    || parse_wire::<Effort>(&effort)? != Effort::Durable
                    || parse_wire::<Effect>(&effect)? != Effect::Apply
                {
                    return Err(AppError::run(SafeRunErrorCode::ControlNotAvailable));
                }
                let payload_json: String = conn
                    .query_row(
                        "SELECT payload_json FROM agent_run_events
                         WHERE run_id = ?1 ORDER BY event_seq DESC LIMIT 1",
                        [run_id],
                        |row| row.get(0),
                    )
                    .map_err(not_found_or_db)?;
                if !matches!(
                    serde_json::from_str::<RunEventPayload>(&payload_json)?,
                    RunEventPayload::Paused {
                        recovery: Some(RunRecoveryKind::ResumeAvailable),
                        ..
                    }
                ) {
                    return Err(AppError::run(SafeRunErrorCode::ControlNotAvailable));
                }
                let confirmation_id: String = conn
                    .query_row(
                        "SELECT confirmation_id FROM agent_run_confirmations
                         WHERE run_id = ?1 AND status = 'consumed'
                         ORDER BY created_at DESC LIMIT 1",
                        [run_id],
                        |row| row.get(0),
                    )
                    .map_err(|_| AppError::run(SafeRunErrorCode::ControlNotAvailable))?;
                let checkpoint = latest_durable_apply_checkpoint_in_conn(conn, run_id)?
                    .ok_or_else(|| AppError::run(SafeRunErrorCode::ControlNotAvailable))?;
                if checkpoint.confirmation_id != confirmation_id
                    || !matches!(
                        checkpoint.stage,
                        DurableApplyCheckpointStage::Approved
                            | DurableApplyCheckpointStage::Dispatching
                    )
                {
                    return Err(AppError::run(SafeRunErrorCode::ControlNotAvailable));
                }
                let next_state_version = stored_state_version + 1;
                let now = chrono::Utc::now().to_rfc3339();
                let updated = conn.execute(
                    "UPDATE agent_runs
                     SET status = 'running', state_version = ?1, updated_at = ?2
                     WHERE run_id = ?3 AND state_version = ?4",
                    rusqlite::params![next_state_version, now, run_id, stored_state_version],
                )?;
                if updated != 1 {
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
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
                        reason: "已重新校验恢复条件，正在继续已确认的变更".into(),
                    },
                )
                .map_err(AppError::msg)?;
                insert_event(conn, &event)?;
                Ok((event, confirmation_id))
            })
        })
    }

    /// Reject an exact pending plan and cancel its Run without dispatching the plan.
    pub(crate) fn reject_frozen_confirmation(
        db: &Database,
        session_key: &str,
        run_id: &str,
        confirmation_id: &str,
        expected_state_version: u64,
        now_unix_ms: i64,
    ) -> AppResult<FrozenConfirmationRejection> {
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
                         WHERE confirmation_id = ?1 AND run_id = ?2",
                        rusqlite::params![confirmation_id, run_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| match error {
                        rusqlite::Error::QueryReturnedNoRows => {
                            AppError::run(SafeRunErrorCode::ConfirmationExpired)
                        }
                        other => other.into(),
                    })?;
                if confirmation_status == "rejected" {
                    return Ok(FrozenConfirmationRejection::AlreadyRejected);
                }
                if confirmation_status != "pending" {
                    return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
                }
                if stored_state_version != expected_state_version {
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
                }
                if parse_wire::<RunState>(&status)? != RunState::AwaitingConfirmation {
                    return Err(AppError::run(SafeRunErrorCode::IllegalTransition));
                }
                let now = chrono::Utc::now().to_rfc3339();
                let rejected = conn.execute(
                    "UPDATE agent_run_confirmations
                     SET status = 'rejected', consumed_at = ?1
                     WHERE confirmation_id = ?2 AND run_id = ?3
                       AND status = 'pending' AND expires_at >= ?4",
                    rusqlite::params![now, confirmation_id, run_id, now_unix_ms],
                )?;
                if rejected != 1 {
                    return Err(AppError::run(SafeRunErrorCode::ConfirmationExpired));
                }
                let next_state_version = stored_state_version + 1;
                let updated = conn.execute(
                    "UPDATE agent_runs
                     SET status = 'cancelled', state_version = ?1, updated_at = ?2
                     WHERE run_id = ?3 AND state_version = ?4",
                    rusqlite::params![next_state_version, now, run_id, stored_state_version],
                )?;
                if updated != 1 {
                    return Err(AppError::run(SafeRunErrorCode::StateVersionConflict));
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
                    RunEventType::Cancelled,
                    &now,
                    RunEventPayload::Cancelled {
                        reason: "user_rejected_change".to_string(),
                    },
                )
                .map_err(AppError::msg)?;
                insert_event(conn, &event)?;
                Ok(FrozenConfirmationRejection::Cancelled(event))
            })
        })
    }
    /// Return only the safe Run snapshot and ordered persisted events.
    pub(crate) fn get(db: &Database, run_id: &str) -> AppResult<Option<AssistantRunGetResponse>> {
        Self::get_scoped(db, run_id, None)
    }

    /// Return the latest recoverable Run for one normal-domain session.
    pub(crate) fn latest_active_for_session(
        db: &Database,
        session_key: &str,
    ) -> AppResult<Option<AssistantRunGetResponse>> {
        let run_id = db.with_read_conn(|conn| {
            conn.query_row(
                "SELECT r.run_id FROM agent_runs r
                 JOIN sessions s ON s.id = r.session_id
                 WHERE s.session_key = ?1
                   AND r.status IN ('accepted', 'preparing', 'running', 'awaiting_confirmation', 'paused', 'verifying')
                 ORDER BY r.updated_at DESC, r.created_at DESC LIMIT 1",
                [session_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(Into::into)
        })?;
        match run_id {
            Some(run_id) => Self::get_for_session(db, session_key, &run_id),
            None => Ok(None),
        }
    }

    /// Load persisted, presentation-safe process events for the latest Run of every requested
    /// turn in one normal-domain session. This intentionally avoids `content_delta` so history
    /// cannot duplicate or confuse the assistant's final message body.
    pub(crate) fn process_events_for_session_turns(
        db: &Database,
        session_key: &str,
        turn_ids: &[String],
    ) -> AppResult<HashMap<String, HistoricalRunProcess>> {
        if turn_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let placeholders = std::iter::repeat_n("?", turn_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let query = format!(
            "SELECT r.run_id, r.turn_id, e.event_seq, e.state_version, e.event_type,
                    e.payload_json, e.created_at
             FROM agent_runs r
             JOIN sessions s ON s.id = r.session_id
             JOIN agent_run_events e ON e.run_id = r.run_id
             WHERE s.session_key = ?
               AND r.turn_id IN ({placeholders})
               AND r.rowid = (
                   SELECT latest.rowid FROM agent_runs latest
                   WHERE latest.session_id = r.session_id AND latest.turn_id = r.turn_id
                   ORDER BY latest.created_at DESC, latest.rowid DESC LIMIT 1
               )
               AND e.event_type IN (
                   'stage_changed', 'reasoning_summary', 'tool_started', 'tool_completed'
               )
             ORDER BY r.turn_id ASC, e.event_seq ASC"
        );
        let mut params = Vec::with_capacity(turn_ids.len() + 1);
        params.push(SqlValue::Text(session_key.to_owned()));
        params.extend(turn_ids.iter().cloned().map(SqlValue::Text));

        db.with_read_conn(|conn| {
            let mut statement = conn.prepare(&query)?;
            let rows = statement.query_map(params_from_iter(params), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?;
            let mut by_turn = HashMap::new();
            for row in rows {
                let (run_id, turn_id, seq, state_version, event_type, payload_json, timestamp) =
                    row?;
                let event = AssistantRunEvent::new(
                    run_id.clone(),
                    seq,
                    state_version,
                    parse_wire::<RunEventType>(&event_type)?,
                    timestamp,
                    serde_json::from_str(&payload_json)?,
                )
                .map_err(AppError::msg)?;
                let process = by_turn
                    .entry(turn_id)
                    .or_insert_with(|| HistoricalRunProcess {
                        run_id,
                        events: Vec::new(),
                    });
                process.events.push(event);
            }
            Ok(by_turn)
        })
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
            let recovery = (state == RunState::Paused)
                .then(|| {
                    events.last().and_then(|event| match event.payload() {
                        RunEventPayload::Paused { recovery, .. } => *recovery,
                        _ => None,
                    })
                })
                .flatten();
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
                    recovery,
                },
                events,
            }))
        })
    }

    /// Rebuild immutable policy input from accepted normal-domain Run facts only.
    ///
    /// This query intentionally reads the persisted envelope and safe explicit
    /// reference metadata, never the user message body, current editor state,
    /// legacy scene, or an unscoped Run.
    pub(crate) fn policy_request_for_session(
        db: &Database,
        session_key: &str,
        run_id: &str,
    ) -> AppResult<Option<crate::ai_runtime::policy_decision_engine::RunPolicyRequest>> {
        db.with_read_conn(|conn| {
            let stored = conn
                .query_row(
                    "SELECT r.envelope_json, m.explicit_references_json
                     FROM agent_runs r
                     JOIN sessions s ON s.id = r.session_id
                     JOIN session_messages m ON m.session_id = r.session_id AND m.turn_id = r.turn_id
                     WHERE r.run_id = ?1 AND s.session_key = ?2 AND m.role = 'user'",
                    rusqlite::params![run_id, session_key],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let Some((envelope_json, references_json)) = stored else {
                return Ok(None);
            };
            let envelope = serde_json::from_str(&envelope_json)?;
            let references: Value = serde_json::from_str(&references_json)?;
            let references = references
                .as_array()
                .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidRequest))?;
            let explicit_reference_paths = references
                .iter()
                .filter_map(|reference| reference.get("filePath"))
                .map(|path| {
                    path.as_str()
                        .filter(|path| !path.trim().is_empty())
                        .map(str::to_string)
                        .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidRequest))
                })
                .collect::<AppResult<Vec<_>>>()?;
            Ok(Some(
                crate::ai_runtime::policy_decision_engine::RunPolicyRequest {
                    envelope,
                    explicit_reference_paths,
                    requested_capabilities: Vec::new(),
                },
            ))
        })
    }

    /// Persist the policy-approved capability set exactly once for a Run.
    /// Repeated dispatch attempts must match byte-for-byte after canonical
    /// sorting; a changed policy result is fail-closed rather than silently
    /// widening an already accepted Run.
    pub(crate) fn persist_authorization_snapshot(
        db: &Database,
        session_key: &str,
        run_id: &str,
        capabilities: &[CapabilityId],
    ) -> AppResult<Vec<CapabilityId>> {
        let canonical = canonical_capabilities(capabilities);
        let canonical_json = serde_json::to_string(&canonical)?;
        let authorization_hash = crate::cas::hash::content_hash_str(&canonical_json);
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                let owned: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM agent_runs r
                     JOIN sessions s ON s.id = r.session_id
                     WHERE r.run_id = ?1 AND s.session_key = ?2",
                    rusqlite::params![run_id, session_key],
                    |row| row.get(0),
                )?;
                if owned != 1 {
                    return Err(AppError::run(SafeRunErrorCode::RunNotFound));
                }
                let existing = conn
                    .query_row(
                        "SELECT allowed_capabilities_json
                         FROM agent_run_authorizations WHERE run_id = ?1",
                        [run_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                if let Some(existing) = existing {
                    if existing != canonical_json {
                        return Err(AppError::msg("agent_run_authorization_conflict"));
                    }
                    return serde_json::from_str(&existing).map_err(AppError::from);
                }
                conn.execute(
                    "INSERT INTO agent_run_authorizations
                     (run_id, allowed_capabilities_json, authorization_hash, created_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![
                        run_id,
                        canonical_json,
                        authorization_hash,
                        chrono::Utc::now().to_rfc3339(),
                    ],
                )?;
                Ok(canonical)
            })
        })
    }

    /// Read one immutable authorization snapshot only through its owning
    /// normal-domain session.
    #[cfg(test)]
    pub(crate) fn authorization_snapshot_for_session(
        db: &Database,
        session_key: &str,
        run_id: &str,
    ) -> AppResult<Option<Vec<CapabilityId>>> {
        db.with_read_conn(|conn| {
            conn.query_row(
                "SELECT a.allowed_capabilities_json
                 FROM agent_run_authorizations a
                 JOIN agent_runs r ON r.run_id = a.run_id
                 JOIN sessions s ON s.id = r.session_id
                 WHERE a.run_id = ?1 AND s.session_key = ?2",
                rusqlite::params![run_id, session_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|json| serde_json::from_str(&json).map_err(AppError::from))
            .transpose()
        })
    }
    /// Read persisted user message and explicit-reference metadata for one normal Run.
    pub(crate) fn prompt_input_for_session(
        db: &Database,
        session_key: &str,
        run_id: &str,
    ) -> AppResult<Option<RunPromptInput>> {
        db.with_read_conn(|conn| {
            let stored = conn
                .query_row(
                    "SELECT r.session_id, m.seq, m.content, m.content_parts,
                            m.explicit_references_json, m.context_scope_json, r.explicit_action_json,
                            r.prompt_profile_snapshot_json
                     FROM agent_runs r
                     JOIN sessions s ON s.id = r.session_id
                     JOIN session_messages m ON m.session_id = r.session_id AND m.turn_id = r.turn_id
                     WHERE r.run_id = ?1 AND s.session_key = ?2 AND m.role = 'user'",
                    rusqlite::params![run_id, session_key],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, Option<String>>(6)?, row.get::<_, Option<String>>(7)?,
                        ))
                    },
                )
                .optional()?;
            let Some((session_id, message_seq_first, message, content_parts_json, references_json, context_scope_json, explicit_action_json, prompt_profile_snapshot_json)) = stored else {
                return Ok(None);
            };
            let explicit_references = serde_json::from_str(&references_json)
                .map_err(|_| AppError::run(SafeRunErrorCode::InvalidExplicitReference))?;
            Ok(Some(RunPromptInput {
                session_id,
                message_seq_first,
                user_message: message,
                content_parts: content_parts_json
                    .map(|value| serde_json::from_str(&value))
                    .transpose()?,
                explicit_references,
                retrieval_scope: serde_json::from_str(&context_scope_json)
                    .map_err(|_| AppError::run(SafeRunErrorCode::InvalidRetrievalScope))?,
                explicit_action: explicit_action_json
                    .map(|value| serde_json::from_str(&value))
                    .transpose()
                    .map_err(|_| AppError::run(SafeRunErrorCode::InvalidRequest))?,
                prompt_profile_snapshot: prompt_profile_snapshot_json
                    .map(|value| serde_json::from_str(&value))
                    .transpose()
                    .map_err(|_| AppError::msg("agent_run_invalid_prompt_profile_snapshot"))?,
            }))
        })
    }
    /// Read the immutable execution budget only when the normal-domain session matches.
    ///
    /// Legacy `{}` rows are deterministically materialized once from the
    /// persisted execution envelope before the policy is returned.
    pub(crate) fn budget_policy_for_session(
        db: &Database,
        session_key: &str,
        run_id: &str,
    ) -> AppResult<Option<RunBudgetPolicy>> {
        db.with_conn(|conn| {
            let stored = conn
                .query_row(
                    "SELECT r.budget_policy_json, r.envelope_json
                     FROM agent_runs r
                     JOIN sessions s ON s.id = r.session_id
                     WHERE r.run_id = ?1 AND s.session_key = ?2",
                    rusqlite::params![run_id, session_key],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let Some((stored_policy, envelope_json)) = stored else {
                return Ok(None);
            };
            let (policy, normalized_policy) =
                materialize_budget_policy(&stored_policy, &envelope_json)?;
            if normalized_policy != stored_policy {
                conn.execute(
                    "UPDATE agent_runs
                     SET budget_policy_json = ?1
                     WHERE run_id = ?2 AND budget_policy_json = ?3",
                    rusqlite::params![normalized_policy, run_id, stored_policy],
                )?;
            }
            Ok(Some(policy))
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

fn materialize_budget_policy(
    stored_policy: &str,
    envelope_json: &str,
) -> AppResult<(RunBudgetPolicy, String)> {
    let envelope: ExecutionEnvelope = serde_json::from_str(envelope_json)
        .map_err(|_| AppError::run(SafeRunErrorCode::InvalidBudgetPolicy))?;
    let canonical_policy = RunBudgetPolicy::for_envelope(&envelope);
    let normalized = serde_json::to_string(&canonical_policy)
        .map_err(|_| AppError::run(SafeRunErrorCode::InvalidBudgetPolicy))?;
    if stored_policy == "{}" {
        return Ok((canonical_policy, normalized));
    }
    if let Ok(stored_policy) = serde_json::from_str::<RunBudgetPolicy>(stored_policy) {
        if stored_policy != canonical_policy {
            return Err(AppError::run(SafeRunErrorCode::InvalidBudgetPolicy));
        }
        return Ok((stored_policy, normalized));
    }
    let legacy_policy: LegacyRunBudgetPolicyV1 = serde_json::from_str(stored_policy)
        .map_err(|_| AppError::run(SafeRunErrorCode::InvalidBudgetPolicy))?;
    if legacy_policy != LegacyRunBudgetPolicyV1::from(&canonical_policy) {
        return Err(AppError::run(SafeRunErrorCode::InvalidBudgetPolicy));
    }
    Ok((canonical_policy, normalized))
}

/// The complete persisted v1 shape before frozen token fields were added.
///
/// This is intentionally exact rather than permissive: only a policy that is
/// otherwise identical to the persisted envelope may be materialized once.
#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyRunBudgetPolicyV1 {
    schema_version: u8,
    profile: crate::ai_runtime::run_contract::RunBudgetProfile,
    max_model_turns: u32,
    max_tool_calls: u32,
    max_child_runs: u32,
    child_max_model_turns: u32,
    child_max_tool_calls: u32,
    child_input_tokens_per_turn: u32,
    child_output_tokens_per_turn: u32,
    post_confirmation_max_model_turns: u32,
}

impl From<&RunBudgetPolicy> for LegacyRunBudgetPolicyV1 {
    fn from(policy: &RunBudgetPolicy) -> Self {
        Self {
            schema_version: policy.schema_version,
            profile: policy.profile,
            max_model_turns: policy.max_model_turns,
            max_tool_calls: policy.max_tool_calls,
            max_child_runs: policy.max_child_runs,
            child_max_model_turns: policy.child_max_model_turns,
            child_max_tool_calls: policy.child_max_tool_calls,
            child_input_tokens_per_turn: policy.child_input_tokens_per_turn,
            child_output_tokens_per_turn: policy.child_output_tokens_per_turn,
            post_confirmation_max_model_turns: policy.post_confirmation_max_model_turns,
        }
    }
}

/// Persisted explicit-reference facts that may be resolved for one Run.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredExplicitReference {
    pub(crate) id: String,
    pub(crate) kind: ContextReferenceKind,
    pub(crate) file_path: Option<String>,
    pub(crate) content_hash: Option<String>,
    pub(crate) utf8_range: Option<SourceSpan>,
    pub(crate) stale: bool,
    pub(crate) invalid_reason: Option<String>,
}

/// Persisted inputs that may reach the scene-free Provider prompt builder.
#[derive(Debug, Clone)]
pub(crate) struct RunPromptInput {
    pub(crate) session_id: i64,
    pub(crate) message_seq_first: i64,
    pub(crate) user_message: String,
    pub(crate) content_parts: Option<Vec<ContentPart>>,
    pub(crate) explicit_references: Vec<StoredExplicitReference>,
    pub(crate) retrieval_scope: crate::ai_runtime::retrieval_scope::ContextScopeDto,
    pub(crate) explicit_action: Option<ExplicitAction>,
    /// Frozen, normalized profile for the exact accepted Run. `None` is only
    /// for rows created before the v2 contract migration.
    pub(crate) prompt_profile_snapshot: Option<PromptProfile>,
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

fn canonical_capabilities(capabilities: &[CapabilityId]) -> Vec<CapabilityId> {
    let mut canonical = capabilities.to_vec();
    canonical.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    canonical.dedup_by(|left, right| left.as_str() == right.as_str());
    canonical
}

fn ensure_normal_session(conn: &Connection, session_id: i64, session_key: &str) -> AppResult<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE id = ?1 AND session_key = ?2",
        rusqlite::params![session_id, session_key],
        |row| row.get(0),
    )?;
    if count == 1 {
        Ok(())
    } else {
        Err(AppError::run(SafeRunErrorCode::SessionNotFound))
    }
}

fn ensure_no_active_top_level_run(conn: &Connection, session_id: i64) -> AppResult<()> {
    let active_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM agent_runs
         WHERE session_id = ?1
           AND status IN ('accepted', 'preparing', 'running', 'awaiting_confirmation', 'paused', 'verifying')",
        [session_id],
        |row| row.get(0),
    )?;
    if active_count == 0 {
        Ok(())
    } else {
        Err(AppError::run(SafeRunErrorCode::ActiveRunExists))
    }
}

/// Build the immutable, non-secret prompt-profile metadata in the same intake
/// transaction that writes the user message and Run ledger row.
fn load_prompt_contract_snapshot(conn: &Connection) -> AppResult<(String, i64, String)> {
    let profile_json = conn
        .query_row(
            "SELECT value FROM user_profile WHERE key = 'ai_prompt_profile'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let profile = profile_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<PromptProfile>(json).ok())
        .unwrap_or_default();
    let snapshot_json = profile.snapshot_json()?;
    let contract_hash =
        crate::cas::hash::content_hash_str(&format!("{PROMPT_CONTRACT_VERSION}:{snapshot_json}"));
    Ok((snapshot_json, PROMPT_CONTRACT_VERSION, contract_hash))
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
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::run(SafeRunErrorCode::ConfirmationMissing)
            }
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
            effect,
            targets,
            expires_at,
            ..
        } if event_confirmation_id == confirmation_id => Ok(Some(
            crate::ai_runtime::run_contract::PendingConfirmationSummary {
                confirmation_id,
                summary,
                effect,
                targets,
                expires_at,
            },
        )),
        _ => Err(AppError::run(SafeRunErrorCode::ConfirmationMissing)),
    }
}

fn confirmation_targets(paths: &[String]) -> Vec<ConfirmationTargetSummary> {
    paths
        .iter()
        .map(|path| ConfirmationTargetSummary {
            kind: if path.starts_with("application://") {
                "other".to_string()
            } else if path.ends_with(".md") {
                "note".to_string()
            } else {
                "file".to_string()
            },
            label: bounded_confirmation_target_label(path),
            risk: RiskClass::BoundedWrite,
        })
        .collect()
}

fn bounded_confirmation_target_label(path: &str) -> String {
    const MAX_LABEL_CHARS: usize = 240;
    let normalized = path.trim().replace('\\', "/");
    let mut chars = normalized.chars();
    let prefix = chars.by_ref().take(MAX_LABEL_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn accepted_for_client_request(
    conn: &Connection,
    client_request_id: &str,
) -> AppResult<Option<(AssistantRunAccepted, Option<String>)>> {
    let result = conn.query_row(
        "SELECT r.client_request_id, r.run_id, r.turn_id, s.session_key, r.status, r.state_version,
                r.intake_fingerprint
         FROM agent_runs r JOIN sessions s ON s.id = r.session_id
         WHERE r.client_request_id = ?1",
        [client_request_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, u64>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        },
    );
    match result {
        Ok((
            client_request_id,
            run_id,
            turn_id,
            session_key,
            status,
            state_version,
            intake_fingerprint,
        )) => Ok(Some((
            AssistantRunAccepted {
                client_request_id,
                run_id,
                turn_id,
                session: AssistantSessionRef {
                    domain: SecurityDomain::Normal,
                    session_key,
                },
                state: parse_wire::<RunState>(&status)?,
                state_version,
            },
            intake_fingerprint,
        ))),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn intake_fingerprint(
    input: &AcceptRunInput,
    external_tool_grants: &[crate::ai_runtime::run_contract::ExternalToolGrantRef],
    create_session: bool,
) -> AppResult<String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct IntakeFingerprint<'a> {
        session_key: Option<&'a str>,
        message: &'a str,
        content_parts: &'a Option<Vec<ContentPart>>,
        explicit_references: &'a [ContextReferenceWire],
        context_scope: &'a crate::ai_runtime::retrieval_scope::ContextScopeDto,
        display_mentions: &'a [crate::ai_runtime::run_contract::DisplayMention],
        explicit_action: &'a Option<ExplicitAction>,
        envelope: &'a ExecutionEnvelope,
        external_tool_grants: &'a [crate::ai_runtime::run_contract::ExternalToolGrantRef],
    }

    let canonical = serde_json::to_vec(&IntakeFingerprint {
        session_key: (!create_session).then_some(input.session_key.as_str()),
        message: &input.message,
        content_parts: &input.content_parts,
        explicit_references: &input.explicit_references,
        context_scope: &input.context_scope,
        display_mentions: &input.display_mentions,
        explicit_action: &input.explicit_action,
        envelope: &input.envelope,
        external_tool_grants,
    })?;
    Ok(hex::encode(Sha256::digest(canonical)))
}

fn retry_intake_fingerprint(input: &RetryRunInput) -> AppResult<String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RetryFingerprint<'a> {
        session_key: &'a str,
        source_run_id: &'a str,
    }

    let canonical = serde_json::to_vec(&RetryFingerprint {
        session_key: &input.session_key,
        source_run_id: &input.source_run_id,
    })?;
    Ok(hex::encode(Sha256::digest(canonical)))
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
                .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidEvent))?,
            serialized["seq"]
                .as_u64()
                .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidEvent))?,
            serialized["stateVersion"]
                .as_u64()
                .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidEvent))?,
            serialized["type"]
                .as_str()
                .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidEvent))?,
            serde_json::to_string(&serialized["payload"])?,
            serialized["timestamp"]
                .as_str()
                .ok_or_else(|| AppError::run(SafeRunErrorCode::InvalidEvent))?,
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
    if let RunEventPayload::ReasoningSummary { summary_id, text } = payload {
        if summary_id.trim().is_empty()
            || summary_id.chars().count() > 160
            || text.trim().is_empty()
            || text.chars().count() > MAX_REASONING_SUMMARY_CHARS
        {
            return Err(AppError::msg("agent_run_invalid_reasoning_summary"));
        }
    }
    match payload {
        RunEventPayload::ToolStarted {
            capability,
            tool_call_id,
        } if capability == "spawn_subagent"
            && !crate::ai_runtime::subagent_coordinator::is_persisted_subagent_id(tool_call_id) =>
        {
            return Err(AppError::run(SafeRunErrorCode::InvalidSubagentLifecycle));
        }
        RunEventPayload::ToolCompleted {
            capability,
            tool_call_id,
            summary,
            ..
        } if capability == "spawn_subagent"
            && (!crate::ai_runtime::subagent_coordinator::is_persisted_subagent_id(
                tool_call_id,
            ) || summary.chars().count() > 600
                || crate::ai_runtime::agent_permissions::audit_contains_sensitive_summary(
                    summary,
                )) =>
        {
            return Err(AppError::run(SafeRunErrorCode::InvalidSubagentLifecycle));
        }
        _ => {}
    }
    if let RunEventPayload::ToolCompleted {
        capability,
        subagent_batch_report: Some(report),
        ..
    } = payload
    {
        if capability != "spawn_subagent"
            || report.items.is_empty()
            || report.items.len()
                > crate::ai_runtime::subagent_coordinator::MAX_SUBAGENT_BATCH_TASKS
            || report.items.iter().any(|item| {
                !crate::ai_runtime::subagent_coordinator::is_persisted_subagent_id(
                        &item.subagent_id,
                    )
                    || item.summary.chars().count() > 600
                    || crate::ai_runtime::agent_permissions::audit_contains_sensitive_summary(
                        &item.summary,
                    )
                    || item.findings.len() > 8
                    || item
                        .findings
                        .iter()
                        .any(|value| {
                            value.chars().count() > 500
                                || crate::ai_runtime::agent_permissions::audit_contains_sensitive_summary(value)
                        })
                    || item.evidence_ids.len() > 8
                    || item
                        .evidence_ids
                        .iter()
                        .any(|value| !matches!(value.parse::<i64>(), Ok(id) if id > 0))
                    || item.confidence > 100
                    || item.open_questions.len() > 8
                    || item
                        .open_questions
                        .iter()
                        .any(|value| {
                            value.chars().count() > 500
                                || crate::ai_runtime::agent_permissions::audit_contains_sensitive_summary(value)
                        })
                    || item.errors.len() > 4
                    || item.errors.iter().any(|value| {
                        value.trim().is_empty()
                            || value.chars().count() > 96
                            || crate::ai_runtime::agent_permissions::audit_contains_sensitive_summary(
                                value,
                            )
                            || !value.chars().all(|character| {
                                character.is_ascii_lowercase()
                                    || character.is_ascii_digit()
                                    || character == '_'
                            })
                    })
            })
        {
            return Err(AppError::run(SafeRunErrorCode::InvalidSubagentBatchReport));
        }
    }
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
        | RunEventPayload::ReasoningSummary { .. }
        | RunEventPayload::ToolStarted { .. }
        | RunEventPayload::ToolCompleted { .. }
        | RunEventPayload::CapabilityDegraded { .. }
        | RunEventPayload::WebVerificationFailed { .. }
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
        return Err(AppError::run(SafeRunErrorCode::UnknownToolCallId));
    }
    Ok(())
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

/// Final answer evidence must be registered by the exact Run. Session ownership
/// alone is insufficient because a prior Run may have searched the same topic.
fn ensure_final_evidence_ids_belong_to_run(
    conn: &Connection,
    run_id: &str,
    session_id: i64,
    evidence_ids: &[i64],
) -> AppResult<()> {
    for evidence_id in evidence_ids {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*)
             FROM agent_run_evidence run_evidence
             JOIN session_evidence evidence ON evidence.id = run_evidence.evidence_id
             WHERE run_evidence.run_id = ?1
               AND run_evidence.evidence_id = ?2
               AND evidence.session_id = ?3
               AND evidence.retired_at IS NULL",
            rusqlite::params![run_id, evidence_id, session_id],
            |row| row.get(0),
        )?;
        if count != 1 {
            return Err(AppError::msg("agent_run_evidence_not_registered_by_run"));
        }
    }
    Ok(())
}

fn latest_durable_apply_checkpoint_in_conn(
    conn: &Connection,
    run_id: &str,
) -> AppResult<Option<DurableApplyCheckpoint>> {
    let stored = conn
        .query_row(
            "SELECT resume_state_json
             FROM agent_run_steps
             WHERE run_id = ?1 AND kind = 'durable_apply'
             ORDER BY step_seq DESC
             LIMIT 1",
            [run_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    stored
        .map(|stored| {
            let checkpoint: DurableApplyCheckpoint = serde_json::from_str(&stored)
                .map_err(|_| AppError::run(SafeRunErrorCode::CheckpointInvalidSchema))?;
            checkpoint.validate()?;
            Ok(checkpoint)
        })
        .transpose()
}

fn not_found_or_db(error: rusqlite::Error) -> AppError {
    if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
        AppError::run(SafeRunErrorCode::RunNotFound)
    } else {
        error.into()
    }
}
