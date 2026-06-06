//! Harness helpers: history compression, evidence compaction, thinking extraction, checkpoints.

use serde::{Deserialize, Serialize};

use crate::ai_runtime::harness::UsageSource;
use crate::ai_runtime::model_gateway::{LlmMessage, TokenUsage, ToolCall};
use crate::ai_runtime::ContextPacket;
use crate::error::AppResult;
use crate::storage::db::Database;

const MAX_FULL_HISTORY: usize = 4;
const HISTORY_SUMMARY_THRESHOLD: usize = 10;

/// Compress old history into a single system summary + recent turns.
pub fn compress_history_messages(history: &[(String, String)]) -> Vec<(String, String)> {
    if history.len() <= HISTORY_SUMMARY_THRESHOLD {
        return history.to_vec();
    }
    let split = history.len().saturating_sub(MAX_FULL_HISTORY);
    let (old, recent) = history.split_at(split);
    let mut summary_parts = Vec::new();
    for (role, content) in old {
        let snippet: String = content.chars().take(80).collect();
        let suffix = if content.chars().count() > 80 {
            "…"
        } else {
            ""
        };
        summary_parts.push(format!("{role}: {snippet}{suffix}"));
    }
    let summary = summary_parts.join(" | ");
    let mut out = vec![("system".to_string(), format!("[历史摘要] {summary}"))];
    out.extend(recent.iter().cloned());
    out
}

/// Estimate token count from char length (rough).
pub fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() / 4).max(1)
}

const MAX_EVIDENCE_PACKETS: usize = 100;

/// Compact evidence packets: hard cap on count, then trim low-score excerpts to token budget.
pub fn compact_evidence(packets: &mut Vec<ContextPacket>, token_budget: usize) {
    if packets.is_empty() {
        return;
    }
    // Hard cap on packet count — sort by score, keep top N
    if packets.len() > MAX_EVIDENCE_PACKETS {
        packets.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        packets.truncate(MAX_EVIDENCE_PACKETS);
    }
    // Trim low-score excerpts to fit within token budget
    packets.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut used = 0usize;
    for packet in packets.iter_mut() {
        let est = estimate_tokens(&packet.excerpt);
        if used + est > token_budget {
            packet.excerpt = format!("[已压缩] {}", packet.title);
            used += 50;
        } else {
            used += est;
        }
    }
}

/// Extract `<thinking>...</thinking>` blocks for UI (stripped from visible content).
pub fn extract_thinking_blocks(content: &str) -> (String, Option<String>) {
    const OPEN: &str = "<thinking>";
    const CLOSE: &str = "</thinking>";
    if !content.contains(OPEN) {
        return (content.to_string(), None);
    }
    let mut thinking = String::new();
    let mut visible = String::new();
    let mut rest = content;
    while let Some(start) = rest.find(OPEN) {
        visible.push_str(&rest[..start]);
        rest = &rest[start + OPEN.len()..];
        if let Some(end) = rest.find(CLOSE) {
            thinking.push_str(rest[..end].trim());
            thinking.push('\n');
            rest = &rest[end + CLOSE.len()..];
        } else {
            thinking.push_str(rest.trim());
            rest = "";
            break;
        }
    }
    visible.push_str(rest);
    let thinking_opt = if thinking.trim().is_empty() {
        None
    } else {
        Some(thinking.trim().to_string())
    };
    (visible.trim().to_string(), thinking_opt)
}

/// Serializable harness input snapshot for resume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessCheckpointMeta {
    pub scene: String,
    pub session_id: i64,
    pub note_path: Option<String>,
    pub note_title: Option<String>,
    pub selection_excerpt: Option<String>,
    pub cold_start_packets: Vec<ContextPacket>,
    pub web_search_enabled: bool,
    pub depth: u32,
}

/// Full harness state for checkpoint save/restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessCheckpoint {
    pub meta: HarnessCheckpointMeta,
    pub round: u32,
    pub messages: Vec<LlmMessage>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_results: Vec<serde_json::Value>,
    pub evidence_packets: Vec<ContextPacket>,
    pub usage: TokenUsage,
    #[serde(default)]
    pub usage_source: UsageSource,
    pub bonus_round_used: bool,
}

/// Save harness checkpoint to `ai_traces`.
pub fn save_harness_checkpoint(
    db: &Database,
    request_id: &str,
    checkpoint: &HarnessCheckpoint,
) -> AppResult<()> {
    let value = serde_json::to_value(checkpoint)
        .map_err(|e| crate::error::AppError::msg(format!("checkpoint: {e}")))?;
    crate::ai_runtime::trace::TraceRecorder::save_checkpoint(db, request_id, &value)
}

/// Load checkpoint if present and trace not completed.
pub fn load_harness_checkpoint(
    db: &Database,
    request_id: &str,
) -> AppResult<Option<HarnessCheckpoint>> {
    db.with_conn(|conn| {
        let row: Result<(Option<String>, String), rusqlite::Error> = conn.query_row(
            "SELECT checkpoint, status FROM ai_traces WHERE request_id = ?1",
            [request_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        );
        match row {
            Ok((Some(json), status))
                if status != "completed" && status != "failed" && status != "aborted" =>
            {
                let cp: HarnessCheckpoint = match serde_json::from_str(&json) {
                    Ok(cp) => cp,
                    Err(_) => return Ok(None),
                };
                Ok(Some(cp))
            }
            Ok(_) => Ok(None),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_history_keeps_recent() {
        let history: Vec<_> = (0..12)
            .map(|i| ("user".to_string(), format!("message {i}")))
            .collect();
        let out = compress_history_messages(&history);
        assert!(out.len() < history.len());
        assert!(out[0].0 == "system");
        assert!(out.last().unwrap().1.contains("message 11"));
    }

    #[test]
    fn extract_thinking() {
        let (visible, think) = extract_thinking_blocks("答案<thinking>先检索法规</thinking>因此…");
        assert!(visible.contains("答案"));
        assert!(think.unwrap().contains("检索"));
    }

    #[test]
    fn load_checkpoint_allows_awaiting_tool_confirmation_status() {
        let db = crate::storage::db::Database::open_in_memory().unwrap();
        let rid = "cp-await-confirm";
        crate::ai_runtime::trace::TraceRecorder::start(
            &db,
            rid,
            crate::ai_runtime::AiScene::KnowledgeLookup,
        )
        .unwrap();
        crate::ai_runtime::trace::TraceRecorder::update_status(
            &db,
            rid,
            crate::ai_runtime::trace::TraceStatus::AwaitingToolConfirmation,
        )
        .unwrap();
        let cp = HarnessCheckpoint {
            meta: HarnessCheckpointMeta {
                scene: "knowledge_lookup".into(),
                session_id: 1,
                note_path: None,
                note_title: None,
                selection_excerpt: None,
                cold_start_packets: vec![],
                web_search_enabled: false,
                depth: 0,
            },
            round: 1,
            messages: vec![],
            tool_calls: vec![],
            tool_results: vec![],
            evidence_packets: vec![],
            usage: crate::ai_runtime::model_gateway::TokenUsage::default(),
            usage_source: UsageSource::Provider,
            bonus_round_used: false,
        };
        save_harness_checkpoint(&db, rid, &cp).unwrap();
        let loaded = load_harness_checkpoint(&db, rid).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().round, 1);
    }
}
