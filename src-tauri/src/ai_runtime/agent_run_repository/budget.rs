//! Versioned model-budget snapshots in existing Run steps.
use super::*;

impl AgentRunRepository {
    /// Load the last versioned model-budget checkpoint and whether this is an
    /// untouched Run. Legacy active Runs must not regain unknown allowances.
    pub(crate) fn model_budget_snapshot(
        db: &Database,
        run_id: &str,
    ) -> AppResult<(Option<Value>, bool)> {
        db.with_read_conn(|conn| {
            let status: String = conn.query_row("SELECT status FROM agent_runs WHERE run_id = ?1", [run_id], |row| row.get(0)).map_err(not_found_or_db)?;
            let stored: Option<String> = conn.query_row(
                "SELECT resume_state_json FROM agent_run_steps WHERE run_id = ?1 AND kind = 'model_budget' ORDER BY step_seq DESC LIMIT 1",
                [run_id], |row| row.get(0)).optional()?;
            let fresh = matches!(status.as_str(), "accepted" | "preparing");
            Ok((stored.map(|value| serde_json::from_str(&value)).transpose()?, fresh))
        })
    }

    /// Append a body-free budget snapshot using the existing step transaction.
    /// The revision check prevents concurrent or stale owners from refreshing it.
    pub(crate) fn persist_model_budget_snapshot(
        db: &Database,
        run_id: &str,
        prior_revision: u64,
        snapshot: Value,
    ) -> AppResult<()> {
        if snapshot.get("schema_version").and_then(Value::as_u64) != Some(1)
            || snapshot.get("revision").and_then(Value::as_u64)
                != Some(prior_revision.saturating_add(1))
        {
            return Err(AppError::run(SafeRunErrorCode::CheckpointInvalidSchema));
        }
        db.with_conn(|conn| in_immediate_transaction(conn, |conn| {
            let status: String = conn.query_row("SELECT status FROM agent_runs WHERE run_id = ?1", [run_id], |row| row.get(0)).map_err(not_found_or_db)?;
            if parse_wire::<RunState>(&status)?.is_terminal() { return Err(AppError::run(SafeRunErrorCode::TerminalState)); }
            let latest: Option<String> = conn.query_row(
                "SELECT resume_state_json FROM agent_run_steps WHERE run_id = ?1 AND kind = 'model_budget' ORDER BY step_seq DESC LIMIT 1",
                [run_id], |row| row.get(0)).optional()?;
            let revision = latest.map(|value| serde_json::from_str::<Value>(&value)).transpose()?
                .and_then(|value| value.get("revision").and_then(Value::as_u64)).unwrap_or(0);
            if revision != prior_revision { return Err(AppError::run(SafeRunErrorCode::StateVersionConflict)); }
            let step_seq: i64 = conn.query_row("SELECT COALESCE(MAX(step_seq), 0) + 1 FROM agent_run_steps WHERE run_id = ?1", [run_id], |row| row.get(0))?;
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute("INSERT INTO agent_run_steps (run_id, step_seq, kind, status, input_summary, output_summary, resume_state_json, evidence_refs_json, created_at, updated_at) VALUES (?1, ?2, 'model_budget', 'checkpoint', '', '', ?3, '[]', ?4, ?4)",
                rusqlite::params![run_id, step_seq, serde_json::to_string(&snapshot)?, now])?;
            Ok(())
        }))
    }
}
