//! Persistent task state for the Agent Task Runtime.

use crate::ai_runtime::deliberation::{DeliberationState, VerificationSummary};
use crate::ai_runtime::session::SessionManager;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_CHECKPOINT_STRING_CHARS: usize = 2_000;
const MAX_CHECKPOINT_ARRAY_ITEMS: usize = 100;
const MAX_CHECKPOINT_OBJECT_KEYS: usize = 50;
const UNSAFE_CHECKPOINT_KEYS: &[&str] = &[
    "api_key",
    "apikey",
    "authorization",
    "content",
    "content_parts",
    "full_content",
    "full_context",
    "full_prompt",
    "messages",
    "note_body",
    "password",
    "prompt",
    "raw",
    "raw_output",
    "secret",
    "token",
    "tool_calls",
    "tool_results",
    "transcript",
];

/// Coarse task execution mode used for scheduling and UI decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskKind {
    /// A single-turn conversational request that should not surface heavy task UI.
    Lightweight,
    /// A multi-step task that may checkpoint and resume.
    Complex,
}

impl AgentTaskKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lightweight => "lightweight",
            Self::Complex => "complex",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "complex" => Self::Complex,
            _ => Self::Lightweight,
        }
    }
}

/// Durable task state. Terminal states must not be resumed without a new task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskStatus {
    /// The task has been created but not yet picked up.
    Queued,
    /// The task is actively executing.
    Running,
    /// The task is blocked on an explicit user/tool confirmation.
    AwaitingConfirmation,
    /// The task paused because its current budget was exhausted safely.
    PausedBudget,
    /// The task paused after a recoverable runtime error.
    PausedRecoverable,
    /// The task completed normally.
    Completed,
    /// The task failed without leaking unsafe data into task state.
    FailedSafe,
    /// The task was explicitly aborted.
    Aborted,
}

impl AgentTaskStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::AwaitingConfirmation => "awaiting_confirmation",
            Self::PausedBudget => "paused_budget",
            Self::PausedRecoverable => "paused_recoverable",
            Self::Completed => "completed",
            Self::FailedSafe => "failed_safe",
            Self::Aborted => "aborted",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "queued" => Self::Queued,
            "awaiting_confirmation" => Self::AwaitingConfirmation,
            "paused_budget" => Self::PausedBudget,
            "paused_recoverable" => Self::PausedRecoverable,
            "completed" => Self::Completed,
            "failed_safe" => Self::FailedSafe,
            "aborted" => Self::Aborted,
            _ => Self::Running,
        }
    }

    /// Parse a wire status, returning `None` for unknown values.
    pub fn parse_wire(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "awaiting_confirmation" => Some(Self::AwaitingConfirmation),
            "paused_budget" => Some(Self::PausedBudget),
            "paused_recoverable" => Some(Self::PausedRecoverable),
            "completed" => Some(Self::Completed),
            "failed_safe" => Some(Self::FailedSafe),
            "aborted" => Some(Self::Aborted),
            _ => None,
        }
    }
}

/// Input required to create a task record.
#[derive(Debug, Clone)]
pub struct CreateTaskInput {
    /// Request id generated for the AI call.
    pub request_id: String,
    /// Owning session id; deletion cascades through this relationship.
    pub session_id: i64,
    /// Task execution mode.
    pub kind: AgentTaskKind,
    /// Raw user input used only to derive a safe summary.
    pub user_input: String,
    /// Budget policy for this task.
    pub budget_policy: Value,
}

/// Task metadata returned to runtime callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    /// Stable task id. For now this is the request id.
    pub task_id: String,
    /// AI request id associated with the task.
    pub request_id: String,
    /// Owning session id.
    pub session_id: i64,
    /// Task execution mode.
    pub kind: AgentTaskKind,
    /// Current lifecycle status.
    pub status: AgentTaskStatus,
    /// Summary-shaped goal; never a full prompt or note body copy.
    pub user_goal_summary: String,
    /// JSON budget policy.
    pub budget_policy: Value,
    /// Creation timestamp.
    pub created_at: String,
    /// Last update timestamp.
    pub updated_at: String,
    /// Terminal timestamp when present.
    pub completed_at: Option<String>,
    /// Sanitized failure code.
    pub error_code: Option<String>,
    /// Sanitized failure message.
    pub error_message: Option<String>,
    /// Latest deliberation state for the request, when available.
    pub deliberation_state: Option<DeliberationState>,
    /// Latest verification summary for the request, when available.
    pub verification_summary: Option<VerificationSummary>,
}

/// Summary-only task step returned to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskStep {
    /// Step row id.
    pub id: i64,
    /// Owning task id.
    pub task_id: String,
    /// Monotonic step sequence within the task.
    pub step_seq: i64,
    /// Step kind, such as `research` or `budget_pause`.
    pub kind: String,
    /// Step lifecycle status.
    pub status: AgentTaskStatus,
    /// Safe input summary.
    pub input_summary: String,
    /// Safe output summary.
    pub output_summary: String,
    /// Evidence packet ids referenced by this step; never packet bodies.
    pub evidence_packet_ids: Vec<String>,
    /// Creation timestamp.
    pub created_at: String,
    /// Last update timestamp.
    pub updated_at: String,
}

/// Summary-only task event returned to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskEvent {
    /// Event row id.
    pub id: i64,
    /// Owning task id.
    pub task_id: String,
    /// Event type.
    pub event_type: String,
    /// Safe event message.
    pub message: String,
    /// Creation timestamp.
    pub created_at: String,
}

/// Filter used by task list IPC and runtime recovery surfaces.
#[derive(Debug, Clone, Default)]
pub struct TaskListFilter {
    /// Restrict results to a single AI session.
    pub session_id: Option<i64>,
    /// Restrict results to one lifecycle status.
    pub status: Option<AgentTaskStatus>,
}

/// Minimal safe continuation plan reconstructed from task metadata and checkpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskResumePlan {
    /// Stable task id to continue; resume never creates an unrelated task id.
    pub task_id: String,
    /// Original AI request id.
    pub request_id: String,
    /// Owning session id, revalidated immediately before continuation.
    pub session_id: i64,
    /// Intent summary from task budget policy, when known.
    pub agent_intent: Option<String>,
    /// Migration-only scene hint retained for existing harness routing.
    pub legacy_scene_hint: Option<String>,
    /// Hash of the vault/task scope used when the task paused.
    pub vault_scope_hash: Option<String>,
    /// Packet ids selected or carried into the safe continuation.
    pub selected_packet_ids: Vec<String>,
    /// Summary of evidence state; never raw note body or tool output.
    pub evidence_ledger_summary: Option<String>,
    /// Last safe step name from which continuation may proceed.
    pub last_safe_step: Option<String>,
    /// Budget policy copied from task metadata.
    pub budget_policy: Value,
    /// Permissions that must be checked again before continuation.
    pub required_permissions: Vec<String>,
    /// Summary-shaped goal for the next segment.
    pub continuation_goal: Option<String>,
    /// Reference ids carried into the next segment.
    pub evidence_refs: Vec<String>,
    /// Next planned action for the continuation executor.
    pub next_action: Option<String>,
    /// Non-authoritative budget hint; safety boundaries are recomputed at runtime.
    pub remaining_budget_hint: Option<Value>,
}

/// Runtime facts checked immediately before a paused task is resumed.
#[derive(Debug, Clone, Default)]
pub struct AgentTaskResumePreflight {
    /// Current UI/session context. When supplied, it must match the paused task.
    pub current_session_id: Option<i64>,
    /// Current vault/task scope hash.
    pub current_vault_scope_hash: Option<String>,
    /// Note paths that still resolve inside the current vault.
    pub accessible_note_paths: Vec<String>,
    /// Evidence packet ids available to the continuation executor.
    pub available_packet_ids: Vec<String>,
    /// Enabled skill names in the current vault.
    pub enabled_skill_names: Vec<String>,
    /// Permission atoms with active grants or low-risk in-request availability.
    pub active_permissions: Vec<String>,
    /// Current model capability slot.
    pub current_model_slot: Option<String>,
}

/// Safe checkpoint input for budget/round-limit pauses.
#[derive(Debug, Clone)]
pub struct BudgetPauseCheckpointInput {
    pub finish_reason: &'static str,
    pub selected_packet_ids: Vec<String>,
    pub evidence_packet_ids: Vec<String>,
    pub evidence_ledger_summary: String,
    pub continuation_goal: String,
    pub last_safe_step: String,
    pub next_action: String,
    pub remaining_budget_hint: Value,
}

/// Storage-backed runtime facade for task lifecycle mutations.
pub struct AgentTaskRuntime;

impl AgentTaskRuntime {
    /// Create a task in running state and return its task id.
    pub fn create_task(db: &Database, input: CreateTaskInput) -> AppResult<String> {
        let task_id = input.request_id.clone();
        let summary = summarize_user_goal(&input.user_input);
        let budget_policy_json = serde_json::to_string(&input.budget_policy)?;
        let now = chrono::Utc::now().to_rfc3339();

        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO agent_tasks
                 (task_id, request_id, session_id, kind, status, user_goal_summary,
                  budget_policy_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                rusqlite::params![
                    task_id,
                    input.request_id,
                    input.session_id,
                    input.kind.as_str(),
                    AgentTaskStatus::Running.as_str(),
                    summary,
                    budget_policy_json,
                    now,
                ],
            )?;
            Ok(task_id)
        })
    }

    /// Return a task by id, or `None` when it has been deleted with its session.
    pub fn get_task(db: &Database, task_id: &str) -> AppResult<Option<AgentTask>> {
        let mut task = db.with_read_conn(|conn| {
            let result = conn.query_row(
                "SELECT task_id, request_id, session_id, kind, status, user_goal_summary,
                        budget_policy_json, created_at, updated_at, completed_at,
                        error_code, error_message
                 FROM agent_tasks
                 WHERE task_id = ?1",
                [task_id],
                |row| {
                    let budget_policy_json: String = row.get(6)?;
                    let budget_policy = serde_json::from_str(&budget_policy_json)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    let kind: String = row.get(3)?;
                    let status: String = row.get(4)?;
                    Ok(AgentTask {
                        task_id: row.get(0)?,
                        request_id: row.get(1)?,
                        session_id: row.get(2)?,
                        kind: AgentTaskKind::parse(&kind),
                        status: AgentTaskStatus::parse(&status),
                        user_goal_summary: row.get(5)?,
                        budget_policy,
                        created_at: row.get(7)?,
                        updated_at: row.get(8)?,
                        completed_at: row.get(9)?,
                        error_code: row.get(10)?,
                        error_message: row.get(11)?,
                        deliberation_state: None,
                        verification_summary: None,
                    })
                },
            );
            match result {
                Ok(task) => Ok(Some(task)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })?;
        if let Some(task) = &mut task {
            hydrate_deliberation_state(db, task)?;
        }
        Ok(task)
    }

    /// List task metadata for a session/status filter.
    pub fn list_tasks(db: &Database, filter: TaskListFilter) -> AppResult<Vec<AgentTask>> {
        let mut tasks = db.with_read_conn(|conn| {
            let mut tasks = Vec::new();
            match (filter.session_id, filter.status) {
                (Some(session_id), Some(status)) => {
                    let mut stmt = conn.prepare(
                        "SELECT task_id, request_id, session_id, kind, status, user_goal_summary,
                                budget_policy_json, created_at, updated_at, completed_at,
                                error_code, error_message
                         FROM agent_tasks
                         WHERE session_id = ?1 AND status = ?2
                         ORDER BY updated_at DESC",
                    )?;
                    let rows = stmt.query_map(
                        rusqlite::params![session_id, status.as_str()],
                        row_to_agent_task,
                    )?;
                    for row in rows {
                        tasks.push(row?);
                    }
                }
                (Some(session_id), None) => {
                    let mut stmt = conn.prepare(
                        "SELECT task_id, request_id, session_id, kind, status, user_goal_summary,
                                budget_policy_json, created_at, updated_at, completed_at,
                                error_code, error_message
                         FROM agent_tasks
                         WHERE session_id = ?1
                         ORDER BY updated_at DESC",
                    )?;
                    let rows = stmt.query_map([session_id], row_to_agent_task)?;
                    for row in rows {
                        tasks.push(row?);
                    }
                }
                (None, Some(status)) => {
                    let mut stmt = conn.prepare(
                        "SELECT task_id, request_id, session_id, kind, status, user_goal_summary,
                                budget_policy_json, created_at, updated_at, completed_at,
                                error_code, error_message
                         FROM agent_tasks
                         WHERE status = ?1
                         ORDER BY updated_at DESC",
                    )?;
                    let rows = stmt.query_map([status.as_str()], row_to_agent_task)?;
                    for row in rows {
                        tasks.push(row?);
                    }
                }
                (None, None) => {
                    let mut stmt = conn.prepare(
                        "SELECT task_id, request_id, session_id, kind, status, user_goal_summary,
                                budget_policy_json, created_at, updated_at, completed_at,
                                error_code, error_message
                         FROM agent_tasks
                         ORDER BY updated_at DESC",
                    )?;
                    let rows = stmt.query_map([], row_to_agent_task)?;
                    for row in rows {
                        tasks.push(row?);
                    }
                }
            }
            Ok(tasks)
        })?;
        hydrate_deliberation_states(db, &mut tasks)?;
        Ok(tasks)
    }

    /// List summary-shaped steps for a task without exposing checkpoints.
    pub fn list_steps(db: &Database, task_id: &str) -> AppResult<Vec<AgentTaskStep>> {
        db.with_read_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, task_id, step_seq, kind, status, input_summary, output_summary,
                        checkpoint_json, evidence_packet_ids, created_at, updated_at
                 FROM agent_task_steps
                 WHERE task_id = ?1
                 ORDER BY step_seq ASC",
            )?;
            let rows = stmt.query_map([task_id], row_to_agent_task_step)?;
            let mut steps = Vec::new();
            for row in rows {
                steps.push(row?);
            }
            Ok(steps)
        })
    }

    /// List summary-shaped events for a task without exposing payload JSON.
    pub fn list_events(db: &Database, task_id: &str) -> AppResult<Vec<AgentTaskEvent>> {
        db.with_read_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, task_id, event_type, message, created_at
                 FROM agent_task_events
                 WHERE task_id = ?1
                 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map([task_id], |row| {
                Ok(AgentTaskEvent {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    event_type: row.get(2)?,
                    message: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?;
            let mut events = Vec::new();
            for row in rows {
                events.push(row?);
            }
            Ok(events)
        })
    }

    /// Replace a task's safe budget policy after routing/context facts are known.
    pub fn update_budget_policy(
        db: &Database,
        task_id: &str,
        budget_policy: Value,
    ) -> AppResult<()> {
        validate_checkpoint(&budget_policy)?;
        let budget_policy_json = serde_json::to_string(&budget_policy)?;
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE agent_tasks
                 SET budget_policy_json = ?1, updated_at = ?2
                 WHERE task_id = ?3",
                rusqlite::params![budget_policy_json, now, task_id],
            )?;
            if updated == 0 {
                return Err(AppError::msg("agent task not found"));
            }
            Ok(())
        })
    }

    /// Return the durable task id associated with a request id.
    pub fn task_id_for_request(db: &Database, request_id: &str) -> AppResult<Option<String>> {
        db.with_read_conn(|conn| {
            let result = conn.query_row(
                "SELECT task_id FROM agent_tasks WHERE request_id = ?1",
                [request_id],
                |row| row.get(0),
            );
            match result {
                Ok(task_id) => Ok(Some(task_id)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    /// Revalidate a paused task and return a safe resume plan without mutating task state.
    pub fn prepare_resume_plan(db: &Database, task_id: &str) -> AppResult<AgentTaskResumePlan> {
        let task =
            Self::get_task(db, task_id)?.ok_or_else(|| AppError::msg("agent task not found"))?;
        if !matches!(
            task.status,
            AgentTaskStatus::PausedBudget | AgentTaskStatus::PausedRecoverable
        ) {
            return Err(AppError::msg("agent task not resumable"));
        }
        if SessionManager::get_session(db, task.session_id)?.is_none() {
            return Err(AppError::msg("agent task session not found"));
        }

        let checkpoint = latest_checkpoint(db, task_id)?;
        validate_checkpoint(&checkpoint)?;
        let plan = AgentTaskResumePlan {
            task_id: task.task_id.clone(),
            request_id: task.request_id.clone(),
            session_id: task.session_id,
            agent_intent: string_field(&task.budget_policy, "agent_intent")
                .or_else(|| string_field(&task.budget_policy, "intent")),
            legacy_scene_hint: string_field(&task.budget_policy, "legacy_scene_hint"),
            vault_scope_hash: string_field(&task.budget_policy, "vault_scope_hash"),
            selected_packet_ids: string_array_field(&checkpoint, "selected_packet_ids")
                .or_else(|| string_array_field(&checkpoint, "evidence_packet_ids"))
                .or_else(|| string_array_field(&checkpoint, "packet_ids"))
                .unwrap_or_default(),
            evidence_ledger_summary: string_field(&checkpoint, "evidence_ledger_summary")
                .or_else(|| string_field(&checkpoint, "summary")),
            last_safe_step: string_field(&checkpoint, "last_safe_step"),
            budget_policy: task.budget_policy.clone(),
            required_permissions: string_array_field(&task.budget_policy, "required_permissions")
                .unwrap_or_default(),
            continuation_goal: string_field(&checkpoint, "continuation_goal"),
            evidence_refs: string_array_field(&checkpoint, "evidence_refs")
                .or_else(|| string_array_field(&checkpoint, "evidence_packet_ids"))
                .or_else(|| string_array_field(&checkpoint, "packet_ids"))
                .unwrap_or_default(),
            next_action: string_field(&checkpoint, "next_action"),
            remaining_budget_hint: checkpoint.get("remaining_budget_hint").cloned(),
        };

        Ok(plan)
    }

    /// Validate resume invariants that can drift while a task is paused.
    pub fn validate_resume_preflight(
        plan: &AgentTaskResumePlan,
        preflight: &AgentTaskResumePreflight,
    ) -> AppResult<()> {
        let mut failures = Vec::new();

        if let Some(current_session_id) = preflight.current_session_id {
            if current_session_id != plan.session_id {
                failures.push("session changed");
            }
        }

        if let Some(expected) = plan.vault_scope_hash.as_deref() {
            if preflight.current_vault_scope_hash.as_deref() != Some(expected) {
                failures.push("vault scope changed");
            }
        }

        if let Some(note_path) = string_field(&plan.budget_policy, "note_path") {
            if !preflight
                .accessible_note_paths
                .iter()
                .any(|path| path == &note_path)
            {
                failures.push("note path unavailable");
            }
        }

        for packet_id in &plan.selected_packet_ids {
            if !preflight
                .available_packet_ids
                .iter()
                .any(|available| available == packet_id)
            {
                failures.push("selected packet unavailable");
                break;
            }
        }

        for skill in string_array_field(&plan.budget_policy, "required_skills").unwrap_or_default()
        {
            if !preflight
                .enabled_skill_names
                .iter()
                .any(|enabled| enabled == &skill)
            {
                failures.push("skill unavailable");
                break;
            }
        }

        for permission in &plan.required_permissions {
            if !preflight
                .active_permissions
                .iter()
                .any(|active| active == permission)
            {
                failures.push("permission expired");
                break;
            }
        }

        if let Some(required) = string_field(&plan.budget_policy, "required_model_slot") {
            if preflight.current_model_slot.as_deref() != Some(required.as_str()) {
                failures.push("model capability changed");
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            failures.sort_unstable();
            failures.dedup();
            Err(AppError::msg(format!(
                "agent task resume preflight failed: {}",
                failures.join("; ")
            )))
        }
    }

    /// Mark a preflight-validated task running and record a safe resume event.
    pub fn begin_resume(db: &Database, task_id: &str, plan: &AgentTaskResumePlan) -> AppResult<()> {
        Self::set_status(db, task_id, AgentTaskStatus::Running, None, None)?;
        Self::record_event(
            db,
            task_id,
            "resume",
            "resume started",
            serde_json::json!({
                "request_id": plan.request_id,
                "last_safe_step": plan.last_safe_step,
                "next_action": plan.next_action,
                "evidence_ref_count": plan.evidence_refs.len(),
                "selected_packet_count": plan.selected_packet_ids.len(),
            }),
        )?;
        Ok(())
    }

    /// Build the canonical safe checkpoint for budget and round-limit pauses.
    pub fn build_budget_pause_checkpoint(input: BudgetPauseCheckpointInput) -> Value {
        serde_json::json!({
            "summary": "segment paused before reliable final answer",
            "finish_reason": input.finish_reason,
            "selected_packet_ids": input.selected_packet_ids,
            "evidence_packet_ids": input.evidence_packet_ids.clone(),
            "evidence_refs": input.evidence_packet_ids,
            "evidence_ledger_summary": input.evidence_ledger_summary,
            "continuation_goal": input.continuation_goal,
            "last_safe_step": input.last_safe_step,
            "next_action": input.next_action,
            "remaining_budget_hint": input.remaining_budget_hint,
        })
    }

    /// Append a summary-shaped step checkpoint.
    pub fn record_step(
        db: &Database,
        task_id: &str,
        kind: &str,
        status: AgentTaskStatus,
        input_summary: &str,
        output_summary: &str,
        checkpoint: Value,
    ) -> AppResult<()> {
        validate_checkpoint(&checkpoint)?;
        let checkpoint_json = serde_json::to_string(&checkpoint)?;
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            let step_seq: i64 = conn.query_row(
                "SELECT COALESCE(MAX(step_seq), 0) + 1 FROM agent_task_steps WHERE task_id = ?1",
                [task_id],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT INTO agent_task_steps
                 (task_id, step_seq, kind, status, input_summary, output_summary,
                  checkpoint_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                rusqlite::params![
                    task_id,
                    step_seq,
                    kind,
                    status.as_str(),
                    truncate_summary(input_summary, 160),
                    truncate_summary(output_summary, 160),
                    checkpoint_json,
                    now,
                ],
            )?;
            Ok(())
        })
    }

    /// Mark a task completed.
    pub fn complete_task(db: &Database, task_id: &str) -> AppResult<()> {
        Self::set_status(db, task_id, AgentTaskStatus::Completed, None, None)
    }

    /// Mark a task as awaiting user or tool confirmation.
    pub fn await_confirmation(db: &Database, task_id: &str) -> AppResult<()> {
        Self::set_status(
            db,
            task_id,
            AgentTaskStatus::AwaitingConfirmation,
            None,
            None,
        )
    }

    /// Pause after exhausting the current segment budget and persist a safe checkpoint.
    pub fn pause_budget(
        db: &Database,
        task_id: &str,
        output_summary: &str,
        checkpoint: Value,
    ) -> AppResult<()> {
        Self::record_step(
            db,
            task_id,
            "budget_pause",
            AgentTaskStatus::PausedBudget,
            "segment budget exhausted",
            output_summary,
            checkpoint,
        )?;
        Self::set_status(db, task_id, AgentTaskStatus::PausedBudget, None, None)
    }

    /// Mark a task as failed without storing sensitive error details.
    pub fn fail_safe(db: &Database, task_id: &str, error_code: &str) -> AppResult<()> {
        Self::set_status(
            db,
            task_id,
            AgentTaskStatus::FailedSafe,
            Some(error_code),
            Some("Task failed safely"),
        )
    }

    /// Mark a task as safely paused after a recoverable resume failure.
    pub fn pause_recoverable(db: &Database, task_id: &str, error_code: &str) -> AppResult<()> {
        Self::set_status(
            db,
            task_id,
            AgentTaskStatus::PausedRecoverable,
            Some(error_code),
            Some("Task paused safely and can be retried"),
        )
    }

    /// Mark a task as aborted.
    pub fn abort_task(db: &Database, task_id: &str) -> AppResult<()> {
        Self::set_status(db, task_id, AgentTaskStatus::Aborted, None, None)
    }

    /// Abort all non-terminal tasks whose recovery state is no longer valid.
    ///
    /// Used by lifecycle boundaries such as cache clearing and vault switching.
    pub fn abort_recoverable_tasks(
        db: &Database,
        reason_code: &str,
        message: &str,
    ) -> AppResult<usize> {
        let now = chrono::Utc::now().to_rfc3339();
        let message = truncate_summary(message, 160);
        db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT task_id
                 FROM agent_tasks
                 WHERE status IN (
                    'queued',
                    'running',
                    'awaiting_confirmation',
                    'paused_budget',
                    'paused_recoverable'
                 )",
            )?;
            let task_ids = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(stmt);

            for task_id in &task_ids {
                conn.execute(
                    "UPDATE agent_tasks
                     SET status = 'aborted',
                         updated_at = ?1,
                         completed_at = ?1,
                         error_code = ?2,
                         error_message = ?3
                     WHERE task_id = ?4",
                    rusqlite::params![now, reason_code, message, task_id],
                )?;
                conn.execute(
                    "INSERT INTO agent_task_events
                     (task_id, event_type, message, payload_json, created_at)
                     VALUES (?1, 'lifecycle_cleanup', ?2, ?3, ?4)",
                    rusqlite::params![
                        task_id,
                        message,
                        serde_json::json!({ "reason": reason_code }).to_string(),
                        now,
                    ],
                )?;
            }

            Ok(task_ids.len())
        })
    }

    /// Record a lightweight task event with JSON-shaped payload.
    pub fn record_event(
        db: &Database,
        task_id: &str,
        event_type: &str,
        message: &str,
        payload: Value,
    ) -> AppResult<()> {
        let payload_json = serde_json::to_string(&payload)?;
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO agent_task_events
                 (task_id, event_type, message, payload_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    task_id,
                    event_type,
                    truncate_summary(message, 160),
                    payload_json,
                    now,
                ],
            )?;
            Ok(())
        })
    }

    fn set_status(
        db: &Database,
        task_id: &str,
        status: AgentTaskStatus,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> AppResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let completed_at = if matches!(
            status,
            AgentTaskStatus::Completed | AgentTaskStatus::FailedSafe | AgentTaskStatus::Aborted
        ) {
            Some(now.as_str())
        } else {
            None
        };
        db.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE agent_tasks
                 SET status = ?1,
                     updated_at = ?2,
                     completed_at = ?3,
                     error_code = ?4,
                     error_message = ?5
                 WHERE task_id = ?6",
                rusqlite::params![
                    status.as_str(),
                    now,
                    completed_at,
                    error_code,
                    error_message,
                    task_id,
                ],
            )?;
            if updated == 0 {
                return Err(AppError::msg("agent task not found"));
            }
            Ok(())
        })
    }
}

fn row_to_agent_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentTask> {
    let budget_policy_json: String = row.get(6)?;
    let budget_policy = serde_json::from_str(&budget_policy_json).unwrap_or(Value::Null);
    let kind: String = row.get(3)?;
    let status: String = row.get(4)?;
    Ok(AgentTask {
        task_id: row.get(0)?,
        request_id: row.get(1)?,
        session_id: row.get(2)?,
        kind: AgentTaskKind::parse(&kind),
        status: AgentTaskStatus::parse(&status),
        user_goal_summary: row.get(5)?,
        budget_policy,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        completed_at: row.get(9)?,
        error_code: row.get(10)?,
        error_message: row.get(11)?,
        deliberation_state: None,
        verification_summary: None,
    })
}

fn hydrate_deliberation_state(db: &Database, task: &mut AgentTask) -> AppResult<()> {
    if let Some((state, summary)) =
        crate::ai_runtime::deliberation::load_deliberation_state(db, &task.request_id)?
    {
        task.deliberation_state = Some(state);
        task.verification_summary = Some(summary);
    }
    Ok(())
}

fn hydrate_deliberation_states(db: &Database, tasks: &mut [AgentTask]) -> AppResult<()> {
    for task in tasks {
        hydrate_deliberation_state(db, task)?;
    }
    Ok(())
}

fn row_to_agent_task_step(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentTaskStep> {
    let status: String = row.get(4)?;
    let checkpoint_json: String = row.get(7)?;
    let evidence_ids_json: String = row.get(8)?;
    let checkpoint = serde_json::from_str(&checkpoint_json).unwrap_or(Value::Null);
    let stored_evidence_ids = serde_json::from_str::<Vec<String>>(&evidence_ids_json)
        .unwrap_or_default()
        .into_iter()
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>();
    let evidence_packet_ids = if stored_evidence_ids.is_empty() {
        string_array_field(&checkpoint, "evidence_packet_ids")
            .or_else(|| string_array_field(&checkpoint, "packet_ids"))
            .unwrap_or_default()
    } else {
        stored_evidence_ids
    };

    Ok(AgentTaskStep {
        id: row.get(0)?,
        task_id: row.get(1)?,
        step_seq: row.get(2)?,
        kind: row.get(3)?,
        status: AgentTaskStatus::parse(&status),
        input_summary: row.get(5)?,
        output_summary: row.get(6)?,
        evidence_packet_ids,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn latest_checkpoint(db: &Database, task_id: &str) -> AppResult<Value> {
    db.with_read_conn(|conn| {
        let result = conn.query_row(
            "SELECT checkpoint_json
             FROM agent_task_steps
             WHERE task_id = ?1
             ORDER BY step_seq DESC
             LIMIT 1",
            [task_id],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(json) => Ok(serde_json::from_str(&json).unwrap_or_else(|_| serde_json::json!({}))),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(serde_json::json!({})),
            Err(err) => Err(err.into()),
        }
    })
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(ToString::to_string)
}

fn string_array_field(value: &Value, key: &str) -> Option<Vec<String>> {
    let items = value.get(key)?.as_array()?;
    Some(
        items
            .iter()
            .filter_map(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(ToString::to_string)
            .collect(),
    )
}

fn summarize_user_goal(input: &str) -> String {
    let normalized = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let digest = Sha256::digest(normalized.as_bytes());
    let hash = hex::encode(&digest[..4]);
    let preview = truncate_summary(&normalized, 42);
    let summary = format!(
        "chars={} sha256={} preview={preview}",
        normalized.chars().count(),
        hash
    );
    truncate_summary(&summary, 80)
}

fn truncate_summary(input: &str, max_chars: usize) -> String {
    let mut out = input.chars().take(max_chars).collect::<String>();
    if input.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

fn validate_checkpoint(value: &Value) -> AppResult<()> {
    validate_checkpoint_value(value, "$")
}

fn validate_checkpoint_value(value: &Value, path: &str) -> AppResult<()> {
    match value {
        Value::Object(map) => {
            if map.len() > MAX_CHECKPOINT_OBJECT_KEYS {
                return Err(AppError::msg(format!(
                    "unsafe checkpoint: too many keys at {path}"
                )));
            }
            for (key, nested) in map {
                let normalized = key.to_ascii_lowercase();
                if UNSAFE_CHECKPOINT_KEYS
                    .iter()
                    .any(|unsafe_key| normalized.contains(unsafe_key))
                    && !is_allowed_budget_hint_counter(path, nested)
                {
                    return Err(AppError::msg(format!(
                        "unsafe checkpoint: key `{key}` is not allowed"
                    )));
                }
                validate_checkpoint_value(nested, &format!("{path}.{key}"))?;
            }
            Ok(())
        }
        Value::Array(items) => {
            if items.len() > MAX_CHECKPOINT_ARRAY_ITEMS {
                return Err(AppError::msg(format!(
                    "unsafe checkpoint: too many array items at {path}"
                )));
            }
            for (idx, nested) in items.iter().enumerate() {
                validate_checkpoint_value(nested, &format!("{path}[{idx}]"))?;
            }
            Ok(())
        }
        Value::String(text) => {
            if text.chars().count() > MAX_CHECKPOINT_STRING_CHARS {
                return Err(AppError::msg(format!(
                    "unsafe checkpoint: string too long at {path}"
                )));
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

fn is_allowed_budget_hint_counter(path: &str, value: &Value) -> bool {
    path == "$.remaining_budget_hint" && value.is_number()
}
