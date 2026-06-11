use crate::app::AppState;
use crate::error::{AppError, AppResult};

pub(super) async fn scheduled_task_create_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let title = args["title"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing title"))?
        .trim();
    let prompt = args["prompt"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing prompt"))?
        .trim();
    let schedule = args["schedule"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing schedule"))?
        .trim();
    if title.is_empty() || prompt.is_empty() || schedule.is_empty() {
        return Err(AppError::msg(
            "scheduled_task_create requires non-empty fields",
        ));
    }
    let id = state.db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO scheduled_tasks (title, prompt, schedule, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, datetime('now'), datetime('now'))",
            rusqlite::params![title, prompt, schedule],
        )?;
        Ok(conn.last_insert_rowid())
    })?;
    Ok(serde_json::json!({
        "ok": true,
        "id": id,
        "note": "Task registered only; Iris does not run proactive tasks without a scheduler/automation approval path.",
    }))
}

pub(super) async fn scheduled_task_list_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let include_disabled = args
        .get("include_disabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let tasks = state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, prompt, schedule, enabled, updated_at
             FROM scheduled_tasks
             WHERE ?1 OR enabled = 1
             ORDER BY updated_at DESC
             LIMIT 50",
        )?;
        let rows = stmt.query_map([include_disabled], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "title": row.get::<_, String>(1)?,
                "prompt": row.get::<_, String>(2)?,
                "schedule": row.get::<_, String>(3)?,
                "enabled": row.get::<_, i64>(4)? != 0,
                "updated_at": row.get::<_, String>(5)?,
            }))
        })?;
        Ok(rows.flatten().collect::<Vec<_>>())
    })?;
    Ok(serde_json::json!({ "tasks": tasks, "count": tasks.len() }))
}

pub(super) async fn scheduled_task_delete_tool(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let id = args["id"]
        .as_i64()
        .ok_or_else(|| AppError::msg("missing id"))?;
    let deleted = state
        .db
        .with_conn(|conn| Ok(conn.execute("DELETE FROM scheduled_tasks WHERE id = ?1", [id])?))?;
    Ok(serde_json::json!({ "ok": deleted > 0, "id": id }))
}
