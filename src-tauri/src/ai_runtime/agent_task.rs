//! Persistent task state for the Agent Task Runtime.

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
        db.with_read_conn(|conn| {
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
                    })
                },
            );
            match result {
                Ok(task) => Ok(Some(task)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
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

    /// Mark a task as aborted.
    pub fn abort_task(db: &Database, task_id: &str) -> AppResult<()> {
        Self::set_status(db, task_id, AgentTaskStatus::Aborted, None, None)
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
