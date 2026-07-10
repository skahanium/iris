//! AI request lifecycle tracing.
//!
//! 每条 AI 请求在 ai_traces 表中记录一行元数据。
//! 默认不记录完整笔记正文，仅保留 request_id、scene、model、tool 名称、
//! latency、token 数量、状态等诊断信息。

use crate::ai_runtime::AiScene;
use crate::error::AppResult;
use crate::storage::db::Database;
use serde::{Deserialize, Serialize};

/// Redact classified paths, document title metadata, suspicious tokens, API keys,
/// and file path leaks from diagnostic strings.
///
/// Strips:
/// - `.classified/` path segments
/// - `title`, `document`, `document_title`, and `note_title` key/value pairs
/// - Content following the `涉密` marker through end of line
/// - Long base64-looking tokens (40+ consecutive base64 chars)
/// - API key patterns (`sk-*`, `key=*`, `token=*`, `Bearer *`)
/// - Absolute file paths (`/Users/...`, `/home/...`, `/tmp/...`)
pub fn redact_classified_leaks(input: &str) -> String {
    let mut out = input.to_string();

    // 1. Redact .classified/ path segments
    while let Some(start) = out.find(".classified/") {
        let end = out[start..]
            .find(['/', '"', '\'', ' ', '\n'])
            .map(|p| start + p)
            .unwrap_or(out.len());
        out.replace_range(start..end, "[REDACTED]");
    }

    // 2. Redact content after 涉密 markers through end-of-line
    while let Some(marker_start) = out.find("涉密") {
        let after_marker = marker_start + "涉密".len();
        let line_end = out[after_marker..]
            .find('\n')
            .map(|p| after_marker + p)
            .unwrap_or(out.len());
        out.replace_range(marker_start..line_end, "[REDACTED]");
    }

    // 3. Redact explicit title/document metadata fields that may carry
    // classified document names in provider or tool errors.
    let metadata_keys = ["title", "document", "document_title", "note_title"];
    for key in metadata_keys {
        for marker in [format!("\"{key}\":"), format!("{key}:")] {
            while let Some(start) = out.find(&marker) {
                let value_start = start + marker.len();
                let end = out[value_start..]
                    .find([',', '\n', '}', ']'])
                    .map(|p| value_start + p)
                    .unwrap_or(out.len());
                out.replace_range(start..end, &format!("{key}:\"[REDACTED]\""));
            }
        }
    }

    // 4. Redact long base64-looking tokens (40+ consecutive base64 chars)
    //    Scan byte-by-byte, tracking runs of [A-Za-z0-9+/=].
    let mut result = String::with_capacity(out.len());
    let mut run_start: Option<usize> = None;
    for (byte_idx, ch) in out.char_indices() {
        if ch.is_ascii_alphanumeric() || ch == '+' || ch == '/' || ch == '=' {
            if run_start.is_none() {
                run_start = Some(byte_idx);
            }
        } else {
            if let Some(start) = run_start {
                if byte_idx - start >= 40 {
                    result.push_str("[REDACTED:TOKEN]");
                } else {
                    result.push_str(&out[start..byte_idx]);
                }
            }
            result.push(ch);
            run_start = None;
        }
    }
    // Flush any trailing run
    if let Some(start) = run_start {
        let end = out.len();
        if end - start >= 40 {
            result.push_str("[REDACTED:TOKEN]");
        } else {
            result.push_str(&out[start..end]);
        }
    }
    out = result;

    // 5. Redact API key patterns: sk-*, key=VALUE, token=VALUE
    let api_prefixes: &[&str] = &["sk-", "key=", "token=", "secret="];
    for prefix in api_prefixes {
        while let Some(start) = out.find(prefix) {
            let val_start = start + prefix.len();
            let end = out[val_start..]
                .find(|c: char| {
                    c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == '}' || c == ']'
                })
                .map(|p| val_start + p)
                .unwrap_or(out.len());
            // Only redact if value is long enough to look like a real secret
            if end - val_start >= 16 {
                out.replace_range(start..end, "[REDACTED:SECRET]");
            } else {
                break;
            }
        }
    }

    // 6. Redact Bearer tokens
    while let Some(start) = out.find("Bearer ") {
        let val_start = start + "Bearer ".len();
        let end = out[val_start..]
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == '}')
            .map(|p| val_start + p)
            .unwrap_or(out.len());
        if end - val_start >= 16 {
            out.replace_range(start..end, "[REDACTED:TOKEN]");
        } else {
            break;
        }
    }

    // 7. Redact absolute file paths (Unix-style: /Users/..., /home/..., /tmp/...)
    let path_prefixes: &[&str] = &["/Users/", "/home/", "/tmp/", "/var/", "/opt/"];
    for prefix in path_prefixes {
        while let Some(start) = out.find(prefix) {
            let end = out[start..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '\n')
                .map(|p| start + p)
                .unwrap_or(out.len());
            out.replace_range(start..end, "[REDACTED:PATH]");
        }
    }

    out
}

/// AI 请求追踪记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiTrace {
    pub request_id: String,
    pub scene: AiScene,
    pub model_slot: Option<String>,
    pub provider: Option<String>,
    pub tool_names: Option<Vec<String>>,
    pub packet_ids: Option<Vec<String>>,
    pub latency_ms: Option<u64>,
    pub token_input: Option<u32>,
    pub token_output: Option<u32>,
    pub status: TraceStatus,
    pub error_code: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceStatus {
    Started,
    ContextAssembled,
    ModelCalled,
    Streaming,
    /// Harness paused waiting for user tool confirmation; checkpoint must remain.
    AwaitingToolConfirmation,
    Completed,
    Failed,
    Aborted,
}

impl TraceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TraceStatus::Started => "started",
            TraceStatus::ContextAssembled => "context_assembled",
            TraceStatus::ModelCalled => "model_called",
            TraceStatus::Streaming => "streaming",
            TraceStatus::AwaitingToolConfirmation => "awaiting_tool_confirmation",
            TraceStatus::Completed => "completed",
            TraceStatus::Failed => "failed",
            TraceStatus::Aborted => "aborted",
        }
    }
}

/// Trace recorder: 将 AiTrace 写入 ai_traces 表。
pub struct TraceRecorder;

impl TraceRecorder {
    /// 创建一条新的 trace 记录（status = started）。
    pub fn start(db: &Database, request_id: &str, scene: AiScene) -> AppResult<()> {
        let scene_str = serde_json::to_string(&scene).unwrap_or_else(|_| format!("{:?}", scene));
        // strip quotes
        let scene_str = scene_str.trim_matches('"');
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO ai_traces (request_id, scene, status, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![request_id, scene_str, TraceStatus::Started.as_str(), now],
            )?;
            Ok(())
        })
    }

    /// Persist harness checkpoint JSON for recovery/debug.
    pub fn save_checkpoint(
        db: &Database,
        request_id: &str,
        checkpoint: &serde_json::Value,
    ) -> AppResult<()> {
        let json = serde_json::to_string(checkpoint)
            .map_err(|e| crate::error::AppError::msg(format!("checkpoint serialize: {e}")))?;
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE ai_traces SET checkpoint = ?1 WHERE request_id = ?2",
                rusqlite::params![json, request_id],
            )?;
            Ok(())
        })
    }

    /// 更新 trace 状态。
    pub fn update_status(db: &Database, request_id: &str, status: TraceStatus) -> AppResult<()> {
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE ai_traces SET status = ?1 WHERE request_id = ?2",
                rusqlite::params![status.as_str(), request_id],
            )?;
            Ok(())
        })
    }

    /// 完成 trace：记录最终状态、model、latency、tokens。
    #[allow(clippy::too_many_arguments)]
    pub fn complete(
        db: &Database,
        request_id: &str,
        status: TraceStatus,
        model_slot: Option<&str>,
        provider: Option<&str>,
        tool_names: Option<&[String]>,
        packet_ids: Option<&[String]>,
        latency_ms: Option<u64>,
        token_input: Option<u32>,
        token_output: Option<u32>,
        error_code: Option<&str>,
    ) -> AppResult<()> {
        let tools_json = tool_names.map(|names| serde_json::to_string(names).unwrap_or_default());
        let packets_json = packet_ids.map(|ids| serde_json::to_string(ids).unwrap_or_default());
        let safe_error = error_code.map(redact_classified_leaks);
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE ai_traces SET
                    status = ?1, model_slot = ?2, provider = ?3,
                    tool_names = ?4, packet_ids = ?5,
                    latency_ms = ?6, token_input = ?7, token_output = ?8,
                    error_code = ?9,
                    checkpoint = CASE WHEN ?1 IN ('completed', 'failed', 'aborted') THEN NULL ELSE checkpoint END
                    /* awaiting_tool_confirmation keeps checkpoint */
                 WHERE request_id = ?10",
                rusqlite::params![
                    status.as_str(),
                    model_slot,
                    provider,
                    tools_json,
                    packets_json,
                    latency_ms,
                    token_input,
                    token_output,
                    safe_error.as_deref(),
                    request_id,
                ],
            )?;
            Ok(())
        })
    }

    /// 获取最近 N 条 trace 记录。
    pub fn recent(db: &Database, limit: u32) -> AppResult<Vec<AiTrace>> {
        db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT request_id, scene, model_slot, provider, tool_names,
                        packet_ids, latency_ms, token_input, token_output,
                        status, error_code, created_at
                 FROM ai_traces ORDER BY created_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit], |row| {
                let scene_str: String = row.get(1)?;
                let scene: AiScene = serde_json::from_str(&format!("\"{scene_str}\""))
                    .unwrap_or(AiScene::KnowledgeLookup);
                Ok(AiTrace {
                    request_id: row.get(0)?,
                    scene,
                    model_slot: row.get(2)?,
                    provider: row.get(3)?,
                    tool_names: row
                        .get::<_, Option<String>>(4)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    packet_ids: row
                        .get::<_, Option<String>>(5)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    latency_ms: row.get(6)?,
                    token_input: row.get(7)?,
                    token_output: row.get(8)?,
                    status: {
                        let s: String = row.get(9)?;
                        match s.as_str() {
                            "completed" => TraceStatus::Completed,
                            "failed" => TraceStatus::Failed,
                            "aborted" => TraceStatus::Aborted,
                            _ => TraceStatus::Started,
                        }
                    },
                    error_code: row.get(10)?,
                    created_at: row.get(11)?,
                })
            })?;
            let mut traces = Vec::new();
            for row in rows {
                traces.push(row?);
            }
            Ok(traces)
        })
    }

    /// Mark stale awaiting tool confirmations as failed so checkpoints do not hang forever.
    pub fn expire_stale_tool_confirmations(db: &Database, ttl_minutes: i64) -> AppResult<usize> {
        let cutoff = chrono::Utc::now() - chrono::Duration::minutes(ttl_minutes.max(1));
        db.with_conn(|conn| {
            let changed = conn.execute(
                "UPDATE ai_traces
                 SET status = ?1, error_code = ?2, checkpoint = NULL
                 WHERE status = ?3 AND created_at < ?4",
                rusqlite::params![
                    TraceStatus::Failed.as_str(),
                    "tool_confirmation_expired",
                    TraceStatus::AwaitingToolConfirmation.as_str(),
                    cutoff.to_rfc3339(),
                ],
            )?;
            Ok(changed)
        })
    }
    /// 清理超过 N 天的 trace 记录。
    pub fn cleanup_older_than(db: &Database, days: i64) -> AppResult<usize> {
        db.with_conn(|conn| {
            let cutoff = chrono::Utc::now() - chrono::Duration::days(days);
            let count = conn.execute(
                "DELETE FROM ai_traces WHERE created_at < ?1",
                [cutoff.to_rfc3339()],
            )?;
            Ok(count)
        })
    }
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    fn setup_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn trace_start_and_complete() {
        let db = setup_db();
        let rid = "test-req-001";

        TraceRecorder::start(&db, rid, AiScene::KnowledgeLookup).unwrap();
        TraceRecorder::update_status(&db, rid, TraceStatus::ContextAssembled).unwrap();
        TraceRecorder::complete(
            &db,
            rid,
            TraceStatus::Completed,
            Some("fast"),
            Some("deepseek"),
            Some(&["search_hybrid".into()]),
            Some(&["pkt-1".into(), "pkt-2".into()]),
            Some(420),
            Some(1500),
            Some(300),
            None,
        )
        .unwrap();

        let traces = TraceRecorder::recent(&db, 10).unwrap();
        assert_eq!(traces.len(), 1);
        let t = &traces[0];
        assert_eq!(t.request_id, rid);
        assert_eq!(t.model_slot.as_deref(), Some("fast"));
        assert_eq!(t.provider.as_deref(), Some("deepseek"));
        assert_eq!(t.latency_ms, Some(420));
    }

    #[test]
    fn trace_records_failed_status() {
        let db = setup_db();
        let rid = "test-fail-001";
        TraceRecorder::start(&db, rid, AiScene::DraftingAssist).unwrap();
        TraceRecorder::complete(
            &db,
            rid,
            TraceStatus::Failed,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("TIMEOUT"),
        )
        .unwrap();

        let traces = TraceRecorder::recent(&db, 10).unwrap();
        assert_eq!(traces[0].error_code.as_deref(), Some("TIMEOUT"));
        assert!(matches!(traces[0].status, TraceStatus::Failed));
    }

    #[test]
    fn expire_stale_tool_confirmation_traces_marks_failed_with_stable_code() {
        let db = Database::open_in_memory().unwrap();
        TraceRecorder::start(&db, "old-awaiting", AiScene::KnowledgeLookup).unwrap();
        TraceRecorder::update_status(&db, "old-awaiting", TraceStatus::AwaitingToolConfirmation)
            .unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE ai_traces SET created_at = ?1 WHERE request_id = ?2",
                rusqlite::params!["2020-01-01T00:00:00Z", "old-awaiting"],
            )?;
            Ok(())
        })
        .unwrap();

        let expired = TraceRecorder::expire_stale_tool_confirmations(&db, 30).unwrap();

        assert_eq!(expired, 1);
        let trace = TraceRecorder::recent(&db, 1).unwrap().remove(0);
        assert_eq!(trace.status, TraceStatus::Failed);
        assert_eq!(
            trace.error_code.as_deref(),
            Some("tool_confirmation_expired")
        );
    }
    #[test]
    fn trace_cleanup_removes_old_records() {
        let db = setup_db();
        TraceRecorder::start(&db, "old-req", AiScene::KnowledgeLookup).unwrap();
        // Directly set created_at to old date
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE ai_traces SET created_at = '2020-01-01T00:00:00Z' WHERE request_id = 'old-req'",
                [],
            )?;
            Ok(())
        }).unwrap();

        let removed = TraceRecorder::cleanup_older_than(&db, 365).unwrap();
        assert!(removed >= 1);

        let traces = TraceRecorder::recent(&db, 10).unwrap();
        assert!(traces.is_empty());
    }

    #[test]
    fn redact_removes_classified_paths() {
        let input = "error in .classified/ai-threads/secret.cst file";
        let redacted = redact_classified_leaks(input);
        assert!(
            !redacted.contains(".classified/"),
            "redacted output must not contain .classified/ paths: {redacted}"
        );
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn redact_preserves_normal_paths() {
        let input = "error in /notes/report.md file";
        let redacted = redact_classified_leaks(input);
        assert_eq!(redacted, input);
    }

    #[test]
    fn redact_strips_nested_classified_segments() {
        let input = "path: /vault/.classified/sub/deep/file.cst";
        let redacted = redact_classified_leaks(input);
        assert!(
            !redacted.contains(".classified/"),
            "nested classified paths must be redacted: {redacted}"
        );
    }

    #[test]
    fn redact_strips_shemi_marker_and_trailing_content() {
        let input = "error: 涉密文件不可访问 /notes/secret.md";
        let redacted = redact_classified_leaks(input);
        assert!(
            !redacted.contains("涉密"),
            "涉密 marker and trailing content must be redacted: {redacted}"
        );
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn redact_strips_long_base64_token() {
        let token = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVoxMjM0NTY3ODkwYWJjZGVm";
        let input = format!("leaked token: {token} in error");
        let redacted = redact_classified_leaks(&input);
        assert!(
            !redacted.contains(token),
            "long base64 token must be redacted: {redacted}"
        );
        assert!(redacted.contains("[REDACTED:TOKEN]"));
    }

    #[test]
    fn redact_preserves_short_tokens() {
        let input = "short id: abc123def456 is fine";
        let redacted = redact_classified_leaks(input);
        assert_eq!(redacted, input, "short tokens must not be redacted");
    }

    #[test]
    fn redact_strips_sk_api_key() {
        let input = "auth failed with key sk-abc123def456ghi789jkl012mno";
        let redacted = redact_classified_leaks(input);
        assert!(
            !redacted.contains("sk-abc123def456ghi789jkl012mno"),
            "sk-* API key must be redacted: {redacted}"
        );
        assert!(redacted.contains("[REDACTED:SECRET]"));
    }

    #[test]
    fn redact_strips_key_equals_pattern() {
        let input = "config has key=supersecretvalue12345678 ok";
        let redacted = redact_classified_leaks(input);
        assert!(
            !redacted.contains("supersecretvalue12345678"),
            "key=* pattern must be redacted: {redacted}"
        );
    }

    #[test]
    fn redact_strips_bearer_token() {
        let input = "request failed with Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9payload";
        let redacted = redact_classified_leaks(input);
        assert!(
            !redacted.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9payload"),
            "Bearer token must be redacted: {redacted}"
        );
    }

    #[test]
    fn redact_strips_absolute_file_path() {
        let input = "error reading /Users/alice/Documents/secret-notes.md in vault";
        let redacted = redact_classified_leaks(input);
        assert!(
            !redacted.contains("/Users/alice/Documents/secret-notes.md"),
            "absolute file path must be redacted: {redacted}"
        );
        assert!(redacted.contains("[REDACTED:PATH]"));
    }

    #[test]
    fn redact_preserves_short_key_equals() {
        let input = "key=ab is fine";
        let redacted = redact_classified_leaks(input);
        assert_eq!(redacted, input, "short key= values must not be redacted");
    }
}
