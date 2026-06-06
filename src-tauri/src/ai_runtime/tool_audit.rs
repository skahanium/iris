//! ToolAudit — persistent tool call audit log with sensitive info sanitization.
//!
//! Records every tool call for debugging and traceability.
//! Strictly enforces sensitive info rules:
//! - Never records API keys, tokens, passwords
//! - Never records full note content
//! - `read_note`: only records path, max_chars, truncated
//! - `replace_selection` / `insert_text_at_cursor`: only records length, hash, risk level
//! - Other tools: summarizes arguments and results to max 500 chars

use crate::error::AppResult;
use crate::storage::db::Database;

/// A single tool audit entry (matches 020_tool_audit schema).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolAuditEntry {
    pub id: i64,
    pub request_id: String,
    pub harness_round: i64,
    pub tool_name: String,
    pub arguments_summary: Option<String>,
    pub result_summary: Option<String>,
    pub success: bool,
    pub duration_ms: Option<i64>,
    pub scene: Option<String>,
    pub subagent_depth: i64,
    pub created_at: String,
}

/// Input for recording a tool audit entry.
pub struct ToolAuditInput<'a> {
    pub request_id: &'a str,
    pub harness_round: u32,
    pub tool_name: &'a str,
    pub arguments: &'a serde_json::Value,
    pub result: &'a serde_json::Value,
    pub success: bool,
    pub duration_ms: u64,
    pub scene: Option<&'a str>,
    pub subagent_depth: u32,
}

/// Record a tool call audit entry.
///
/// Sanitizes arguments and result before storage.
pub fn record_audit(db: &Database, input: &ToolAuditInput<'_>) -> AppResult<()> {
    let args_summary = sanitize_arguments(input.tool_name, input.arguments);
    let result_summary = sanitize_result(input.tool_name, input.result, input.success);

    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO tool_audit \
             (request_id, harness_round, tool_name, arguments_summary, result_summary, \
              success, duration_ms, scene, subagent_depth) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                input.request_id,
                input.harness_round as i64,
                input.tool_name,
                args_summary,
                result_summary,
                input.success as i64,
                input.duration_ms as i64,
                input.scene,
                input.subagent_depth as i64,
            ],
        )?;
        Ok(())
    })
}

/// Sanitize tool arguments for audit storage.
///
/// Applies tool-specific rules to avoid recording sensitive data.
fn sanitize_arguments(tool_name: &str, args: &serde_json::Value) -> Option<String> {
    let obj = args.as_object()?;
    match tool_name {
        "read_note" => {
            // Only record path and max_chars, never content
            let path = obj.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let max_chars = obj.get("max_chars").and_then(|v| v.as_u64());
            Some(format!("path={path}, max_chars={max_chars:?}"))
        }
        "replace_selection" | "insert_text_at_cursor" => {
            // Record length, hash, and risk level — never content
            let text_key = if tool_name == "replace_selection" {
                "replacement"
            } else {
                "text"
            };
            let text = obj.get(text_key).and_then(|v| v.as_str()).unwrap_or("");
            let len = text.chars().count();
            let hash = content_hash(text);
            let risk = assess_write_risk(text);
            Some(format!("len={len}, hash={hash}, risk={risk}"))
        }
        "web_search" => {
            // Record query only, not full args
            let query = obj.get("query").and_then(|v| v.as_str()).unwrap_or("");
            Some(format!("query={query}"))
        }
        "fetch_web_page" => {
            // Record URL only
            let url = obj.get("url").and_then(|v| v.as_str()).unwrap_or("");
            Some(format!("url={url}"))
        }
        _ => {
            // Generic: truncate to 500 chars
            let s = serde_json::to_string(args).unwrap_or_default();
            Some(truncate_summary(&s, 500))
        }
    }
}

/// Sanitize tool result for audit storage.
fn sanitize_result(tool_name: &str, result: &serde_json::Value, success: bool) -> Option<String> {
    if !success {
        let err = result
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Some(truncate_summary(err, 200));
    }
    match tool_name {
        "read_note" => {
            // Only record path and truncation status, never content
            let path = result.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let truncated = result
                .get("truncated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Some(format!("path={path}, truncated={truncated}"))
        }
        "replace_selection" | "insert_text_at_cursor" => {
            // Record success/failure only
            Some("ok".into())
        }
        "search_hybrid" | "search_semantic" | "search_keyword" => {
            let count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
            Some(format!("results={count}"))
        }
        _ => {
            let s = serde_json::to_string(result).unwrap_or_default();
            Some(truncate_summary(&s, 500))
        }
    }
}

/// Truncate a string to max_len chars, adding "…" if truncated.
fn truncate_summary(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len])
    }
}

/// Simple content hash for audit (SHA-256 hex, first 16 chars).
fn content_hash(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Assess write risk level based on content characteristics.
fn assess_write_risk(text: &str) -> &'static str {
    let len = text.chars().count();
    if len > 5000 {
        "high"
    } else if len > 1000 {
        "medium"
    } else {
        "low"
    }
}

/// Query audit entries by request_id.
pub fn query_by_request(db: &Database, request_id: &str) -> AppResult<Vec<ToolAuditEntry>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, request_id, harness_round, tool_name, \
             arguments_summary, result_summary, success, duration_ms, \
             scene, subagent_depth, created_at \
             FROM tool_audit WHERE request_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([request_id], |row| {
            Ok(ToolAuditEntry {
                id: row.get(0)?,
                request_id: row.get(1)?,
                harness_round: row.get(2)?,
                tool_name: row.get(3)?,
                arguments_summary: row.get(4)?,
                result_summary: row.get(5)?,
                success: row.get::<_, i64>(6)? != 0,
                duration_ms: row.get(7)?,
                scene: row.get(8)?,
                subagent_depth: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?;
        Ok(rows.flatten().collect())
    })
}

/// Count audit entries by request_id.
pub fn count_by_request(db: &Database, request_id: &str) -> AppResult<i64> {
    db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM tool_audit WHERE request_id = ?1",
            [request_id],
            |row| row.get(0),
        )?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    fn test_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn assess_write_risk_levels() {
        assert_eq!(assess_write_risk("short"), "low");
        assert_eq!(assess_write_risk(&"a".repeat(1500)), "medium");
        assert_eq!(assess_write_risk(&"a".repeat(6000)), "high");
    }

    #[test]
    fn sanitize_read_note_args() {
        let args = serde_json::json!({"path": "notes/test.md", "max_chars": 5000});
        let summary = sanitize_arguments("read_note", &args).unwrap();
        assert!(summary.contains("notes/test.md"));
        assert!(summary.contains("5000"));
        assert!(!summary.contains("content"));
    }

    #[test]
    fn sanitize_insert_text_args() {
        let args = serde_json::json!({"text": "这是一段很长的中文文本用于测试"});
        let summary = sanitize_arguments("insert_text_at_cursor", &args).unwrap();
        assert!(summary.contains("len="));
        assert!(summary.contains("hash="));
        assert!(summary.contains("risk="));
        assert!(!summary.contains("中文"));
    }

    #[test]
    fn sanitize_replace_selection_args() {
        let args = serde_json::json!({"replacement": "new text content here"});
        let summary = sanitize_arguments("replace_selection", &args).unwrap();
        assert!(summary.contains("len="));
        assert!(summary.contains("hash="));
        assert!(summary.contains("risk="));
        assert!(!summary.contains("new text"));
    }

    #[test]
    fn sanitize_search_args_generic() {
        let args = serde_json::json!({"query": "test query", "limit": 10});
        let summary = sanitize_arguments("search_hybrid", &args).unwrap();
        assert!(summary.contains("test query"));
    }

    #[test]
    fn sanitize_read_note_result() {
        let result = serde_json::json!({"path": "test.md", "content": "full note content", "truncated": false});
        let summary = sanitize_result("read_note", &result, true).unwrap();
        assert!(summary.contains("test.md"));
        assert!(!summary.contains("full note content"));
    }

    #[test]
    fn sanitize_error_result() {
        let result = serde_json::json!({"error": "something failed"});
        let summary = sanitize_result("any_tool", &result, false).unwrap();
        assert!(summary.contains("something failed"));
    }

    #[test]
    fn sanitize_search_result() {
        let result = serde_json::json!({"results": [], "count": 5});
        let summary = sanitize_result("search_hybrid", &result, true).unwrap();
        assert!(summary.contains("results=5"));
    }

    #[test]
    fn truncate_summary_short() {
        assert_eq!(truncate_summary("hello", 10), "hello");
    }

    #[test]
    fn truncate_summary_long() {
        let s = "a".repeat(600);
        let result = truncate_summary(&s, 500);
        assert!(result.chars().count() <= 501); // 500 + "…"
        assert!(result.ends_with('…'));
    }

    #[test]
    fn content_hash_deterministic() {
        let h1 = content_hash("test content");
        let h2 = content_hash("test content");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn content_hash_different_inputs() {
        let h1 = content_hash("content A");
        let h2 = content_hash("content B");
        assert_ne!(h1, h2);
    }

    #[test]
    fn record_and_query_audit() {
        let db = test_db();
        // Create required tables (normally done by migrate_up)
        db.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS ai_traces (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    request_id TEXT NOT NULL UNIQUE,
                    scene TEXT NOT NULL,
                    model_slot TEXT, provider TEXT,
                    tool_names JSON, packet_ids JSON,
                    latency_ms INTEGER, token_input INTEGER, token_output INTEGER,
                    status TEXT NOT NULL, error_code TEXT,
                    created_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS tool_audit (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    request_id TEXT NOT NULL, harness_round INTEGER NOT NULL,
                    tool_name TEXT NOT NULL, arguments_summary TEXT,
                    result_summary TEXT, success INTEGER NOT NULL DEFAULT 0,
                    duration_ms INTEGER, scene TEXT,
                    subagent_depth INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );",
            )?;
            conn.execute(
                "INSERT INTO ai_traces (request_id, scene, status, created_at)
                 VALUES ('req-1', 'knowledge_lookup', 'running', datetime('now'))",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        record_audit(
            &db,
            &ToolAuditInput {
                request_id: "req-1",
                harness_round: 1,
                tool_name: "read_note",
                arguments: &serde_json::json!({"path": "test.md", "max_chars": 5000}),
                result: &serde_json::json!({"path": "test.md", "truncated": false}),
                success: true,
                duration_ms: 150,
                scene: Some("knowledge_lookup"),
                subagent_depth: 0,
            },
        )
        .unwrap();

        let entries = query_by_request(&db, "req-1").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tool_name, "read_note");
        assert!(entries[0].success);
        assert!(entries[0]
            .arguments_summary
            .as_ref()
            .unwrap()
            .contains("test.md"));

        let count = count_by_request(&db, "req-1").unwrap();
        assert_eq!(count, 1);
    }
}
