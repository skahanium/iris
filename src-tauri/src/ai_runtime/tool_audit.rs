//! Run-scoped tool audit storage with content-safe summaries.

use crate::error::AppResult;
use crate::storage::db::Database;

/// One sanitized tool execution record belonging to a Run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolAuditEntry {
    pub id: i64,
    pub run_id: String,
    pub run_step: i64,
    pub tool_name: String,
    pub arguments_summary: Option<String>,
    pub result_summary: Option<String>,
    pub success: bool,
    pub duration_ms: Option<i64>,
    pub subagent_depth: i64,
    pub created_at: String,
}

/// Input for recording one Run-scoped tool execution.
pub struct ToolAuditInput<'a> {
    pub run_id: &'a str,
    pub run_step: u32,
    pub tool_name: &'a str,
    pub arguments: &'a serde_json::Value,
    pub result: &'a serde_json::Value,
    pub error: Option<&'a str>,
    pub success: bool,
    pub duration_ms: u64,
    pub subagent_depth: u32,
}

/// Persist a sanitized audit record without raw note, credential, or web content.
pub fn record_audit(db: &Database, input: &ToolAuditInput<'_>) -> AppResult<()> {
    let mut arguments_summary = sanitize_arguments(input.tool_name, input.arguments);
    if !input.success {
        if let Some(class) = classify_failure(input.result, input.error) {
            arguments_summary = Some(match arguments_summary {
                Some(summary) => format!("{summary}, failure_class={class}"),
                None => format!("failure_class={class}"),
            });
        }
    }
    let result_summary = sanitize_result(input.tool_name, input.result, input.error, input.success);
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO tool_audit \
             (run_id, run_step, tool_name, arguments_summary, result_summary, success, duration_ms, subagent_depth) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                input.run_id,
                input.run_step as i64,
                input.tool_name,
                arguments_summary,
                result_summary,
                input.success as i64,
                input.duration_ms as i64,
                input.subagent_depth as i64,
            ],
        )?;
        Ok(())
    })
}

/// Query sanitized tool records for one Run.
pub fn query_by_run(db: &Database, run_id: &str) -> AppResult<Vec<ToolAuditEntry>> {
    db.with_read_conn(|conn| {
        let mut statement = conn.prepare(
            "SELECT id, run_id, run_step, tool_name, arguments_summary, result_summary, \
             success, duration_ms, subagent_depth, created_at \
             FROM tool_audit WHERE run_id = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map([run_id], |row| {
            Ok(ToolAuditEntry {
                id: row.get(0)?,
                run_id: row.get(1)?,
                run_step: row.get(2)?,
                tool_name: row.get(3)?,
                arguments_summary: row.get(4)?,
                result_summary: row.get(5)?,
                success: row.get::<_, i64>(6)? != 0,
                duration_ms: row.get(7)?,
                subagent_depth: row.get(8)?,
                created_at: row.get(9)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    })
}

/// Count tool records for one Run.
pub fn count_by_run(db: &Database, run_id: &str) -> AppResult<i64> {
    db.with_read_conn(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM tool_audit WHERE run_id = ?1",
            [run_id],
            |row| row.get(0),
        )
        .map_err(Into::into)
    })
}

fn sanitize_arguments(tool_name: &str, arguments: &serde_json::Value) -> Option<String> {
    let object = arguments.as_object()?;
    match tool_name {
        "read_note" => Some(format!(
            "path={}, max_chars={:?}",
            object
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(""),
            object.get("max_chars").and_then(|value| value.as_u64())
        )),
        "replace_selection" | "insert_text_at_cursor" => {
            let key = if tool_name == "replace_selection" {
                "replacement"
            } else {
                "text"
            };
            let text = object
                .get(key)
                .and_then(|value| value.as_str())
                .unwrap_or("");
            Some(format!(
                "len={}, hash={}",
                text.chars().count(),
                audit_hash(text)
            ))
        }
        "web_search" => {
            let query = object
                .get("query")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let url_count = object
                .get("urls")
                .and_then(|value| value.as_array())
                .map_or(0, Vec::len);
            Some(format!(
                "query_hash={}, url_count={url_count}",
                audit_hash(query)
            ))
        }
        _ => Some(json_shape_summary(arguments)),
    }
}

fn sanitize_result(
    tool_name: &str,
    result: &serde_json::Value,
    error: Option<&str>,
    success: bool,
) -> Option<String> {
    if !success {
        return Some(truncate_summary(
            error
                .filter(|value| !value.trim().is_empty())
                .or_else(|| result.get("error").and_then(|value| value.as_str()))
                .unwrap_or("unknown error"),
            200,
        ));
    }
    match tool_name {
        "read_note" => Some(format!(
            "path={}, truncated={}",
            result
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(""),
            result
                .get("truncated")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        )),
        "replace_selection" | "insert_text_at_cursor" => Some("ok".into()),
        _ => Some(json_shape_summary(result)),
    }
}

fn classify_failure(result: &serde_json::Value, error: Option<&str>) -> Option<String> {
    let raw = result
        .get("failure_class")
        .and_then(|value| value.as_str())
        .or_else(|| result.get("failure_kind").and_then(|value| value.as_str()))
        .or(error)
        .or_else(|| result.get("error").and_then(|value| value.as_str()))?;
    let lower = raw.to_ascii_lowercase();
    let class = if lower.contains("auth") || lower.contains("credential") {
        "provider_auth_missing"
    } else if lower.contains("timeout") {
        "provider_timeout"
    } else if lower.contains("denied") || lower.contains("policy") {
        "policy_denied"
    } else {
        "unknown"
    };
    Some(class.into())
}

fn audit_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(&Sha256::digest(text.as_bytes())[..12])
}

fn json_shape_summary(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(object) => format!("shape=object, keys={}", object.len()),
        serde_json::Value::Array(items) => format!("shape=array, len={}", items.len()),
        serde_json::Value::String(_) => "shape=string".into(),
        serde_json::Value::Number(_) => "shape=number".into(),
        serde_json::Value::Bool(_) => "shape=bool".into(),
        serde_json::Value::Null => "shape=null".into(),
    }
}

fn truncate_summary(text: &str, max_chars: usize) -> String {
    let output: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        format!("{output}...")
    } else {
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::agent_run_repository::{AcceptRunInput, AgentRunRepository};
    use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
    use crate::ai_runtime::run_contract::{
        ContextMode, Effect, Effort, ExecutionEnvelope, Freshness, MaterialNeed, Modality,
        RiskClass, SecurityDomain,
    };

    fn audit_db() -> (Database, String) {
        let db = Database::open_in_memory().expect("database");
        let session = NormalSessionRepository::create(&db).expect("normal session");
        let run_id = "tool-audit-run".to_string();
        AgentRunRepository::accept(
            &db,
            AcceptRunInput {
                session_id: session.session_id,
                session_key: session.session_key,
                client_request_id: "tool-audit-client-request".to_string(),
                run_id: run_id.clone(),
                turn_id: "tool-audit-turn".to_string(),
                message: "record a sanitized tool result".to_string(),
                content_parts: None,
                explicit_references: vec![],
                explicit_action: None,
                envelope: ExecutionEnvelope {
                    effect: Effect::Answer,
                    context: ContextMode::ExplicitReferences,
                    freshness: Freshness::Offline,
                    effort: Effort::Direct,
                    security_domain: SecurityDomain::Normal,
                    risk: RiskClass::ReadOnly,
                    modalities: vec![Modality::Text],
                    material_needs: vec![MaterialNeed::Reference],
                    required_capabilities: vec![],
                    explicit_constraints: vec![],
                },
            },
        )
        .expect("accepted run");
        (db, run_id)
    }

    #[test]
    fn audit_is_keyed_by_run_and_step() {
        let (db, run_id) = audit_db();
        record_audit(
            &db,
            &ToolAuditInput {
                run_id: &run_id,
                run_step: 1,
                tool_name: "read_note",
                arguments: &serde_json::json!({"path":"notes/a.md"}),
                result: &serde_json::json!({"path":"notes/a.md","truncated":false}),
                error: None,
                success: true,
                duration_ms: 1,
                subagent_depth: 0,
            },
        )
        .expect("record");
        assert_eq!(count_by_run(&db, &run_id).expect("count"), 1);
        let item = query_by_run(&db, &run_id).expect("query").remove(0);
        assert_eq!(item.run_id, run_id);
        assert_eq!(item.run_step, 1);
        assert!(item.arguments_summary.unwrap().contains("notes/a.md"));
    }

    #[test]
    fn web_query_and_body_are_not_persisted() {
        let summary = sanitize_arguments("web_search", &serde_json::json!({
            "query":"private query", "urls":["https://example.test/private"], "page_body":"private body"
        })).expect("summary");
        assert!(summary.contains("query_hash="));
        assert!(!summary.contains("private query"));
        assert!(!summary.contains("private body"));
    }
}
