//! AI request lifecycle tracing.
//!
//! 每条 AI 请求在 ai_traces 表中记录一行元数据。
//! 默认不记录完整笔记正文，仅保留 request_id、scene、model、tool 名称、
//! latency、token 数量、状态等诊断信息。

use crate::ai_runtime::AiScene;
use crate::error::AppResult;
use crate::storage::db::Database;
use serde::{Deserialize, Serialize};

/// AI 请求追踪记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceStatus {
    Started,
    ContextAssembled,
    ModelCalled,
    Streaming,
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
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE ai_traces SET
                    status = ?1, model_slot = ?2, provider = ?3,
                    tool_names = ?4, packet_ids = ?5,
                    latency_ms = ?6, token_input = ?7, token_output = ?8,
                    error_code = ?9
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
                    error_code,
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
}
