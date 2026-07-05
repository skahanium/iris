//! Harness helpers: history compression, evidence compaction, thinking extraction, checkpoints.

use serde::{Deserialize, Serialize};

use crate::ai_runtime::harness::UsageSource;
use crate::ai_runtime::model_gateway::{LlmMessage, TokenUsage, ToolCall};
use crate::ai_runtime::{
    agent_task_policy::AgentTaskPolicy, CapabilitySlot, ContextPacket, EndpointFamily,
    SkillActivationPlanSummary,
};
use crate::error::AppResult;
use crate::storage::db::Database;

const MAX_FULL_HISTORY: usize = 4;
const HISTORY_SUMMARY_THRESHOLD: usize = 10;
const MAX_HISTORY_MESSAGE_TOKENS: usize = 2_000;
const CONTEXT_TRUNCATION_MARKER: &str = "\n...（已按上下文预算截断）";

/// Compress old history into a single system summary + recent turns.
pub fn compress_history_messages(history: &[(String, String)]) -> Vec<(String, String)> {
    let mut preserved_system = Vec::new();
    let mut compressible = Vec::with_capacity(history.len());
    for (role, content) in history {
        if role == "system" && content.contains("## ConversationMemory") {
            preserved_system.push((role.clone(), content.clone()));
        } else {
            compressible.push((role.clone(), content.clone()));
        }
    }

    if compressible.len() <= HISTORY_SUMMARY_THRESHOLD {
        preserved_system.extend(compressible);
        return trim_history_message_bodies(preserved_system);
    }

    let split = compressible.len().saturating_sub(MAX_FULL_HISTORY);
    let (old, recent) = compressible.split_at(split);
    let mut summary_parts = Vec::new();
    for (role, content) in old {
        let snippet: String = content.chars().take(80).collect();
        let suffix = if content.chars().count() > 80 {
            "…"
        } else {
            ""
        };
        summary_parts.push(format!("- {role}: {snippet}{suffix}"));
    }
    let summary = summary_parts.join(
        "
",
    );
    let mut out = preserved_system;
    out.push((
        "system".to_string(),
        format!(
            "[历史摘要]
{summary}"
        ),
    ));
    out.extend(recent.iter().cloned());
    trim_history_message_bodies(out)
}

fn trim_history_message_bodies(history: Vec<(String, String)>) -> Vec<(String, String)> {
    let latest_user_index = history.iter().rposition(|(role, _)| role == "user");
    history
        .into_iter()
        .enumerate()
        .map(|(index, (role, content))| {
            if Some(index) == latest_user_index
                || (role == "system" && content.contains("## ConversationMemory"))
                || estimate_tokens(&content) <= MAX_HISTORY_MESSAGE_TOKENS
            {
                return (role, content);
            }
            (
                role,
                truncate_text_to_token_budget(
                    &content,
                    MAX_HISTORY_MESSAGE_TOKENS,
                    CONTEXT_TRUNCATION_MARKER,
                ),
            )
        })
        .collect()
}

/// Estimate token count using the same CJK-aware heuristic as the gateway guard.
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let chars = text.chars().count();
    let cjk = text
        .chars()
        .filter(|ch| {
            matches!(
                *ch as u32,
                0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3040..=0x30FF | 0xAC00..=0xD7AF
            )
        })
        .count();
    let non_cjk = chars.saturating_sub(cjk);
    cjk.saturating_add(non_cjk.div_ceil(4)).max(1)
}

/// Truncate text to an estimated token budget while preserving UTF-8 boundaries.
pub fn truncate_text_to_token_budget(text: &str, token_budget: usize, marker: &str) -> String {
    if estimate_tokens(text) <= token_budget {
        return text.to_string();
    }
    if token_budget == 0 {
        return String::new();
    }

    let marker_tokens = estimate_tokens(marker);
    let body_budget = token_budget.saturating_sub(marker_tokens).max(1);
    let mut out = String::new();
    let mut cjk = 0usize;
    let mut non_cjk = 0usize;
    for ch in text.chars() {
        if matches!(
            ch as u32,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3040..=0x30FF | 0xAC00..=0xD7AF
        ) {
            cjk += 1;
        } else {
            non_cjk += 1;
        }
        let estimated = cjk.saturating_add(non_cjk.div_ceil(4)).max(1);
        if estimated > body_budget {
            break;
        }
        out.push(ch);
    }
    out.push_str(marker);
    out
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
        evidence_source_rank(&a.source_type)
            .cmp(&evidence_source_rank(&b.source_type))
            .then(
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    let mut used = 0usize;
    for packet in packets.iter_mut() {
        let est = estimate_tokens(&packet.excerpt);
        if used + est > token_budget {
            let remaining = token_budget.saturating_sub(used);
            packet.excerpt = if evidence_source_rank(&packet.source_type) >= 3 || remaining <= 64 {
                format!("[已压缩] {}", packet.title)
            } else {
                truncate_text_to_token_budget(&packet.excerpt, remaining, CONTEXT_TRUNCATION_MARKER)
            };
            used = used.saturating_add(estimate_tokens(&packet.excerpt));
        } else {
            used += est;
        }
    }
}

fn evidence_source_rank(source_type: &crate::ai_runtime::SourceType) -> u8 {
    use crate::ai_runtime::SourceType;
    match source_type {
        SourceType::Note | SourceType::Anchor => 0,
        SourceType::Regulation => 1,
        SourceType::Template | SourceType::Session => 2,
        SourceType::Web => 3,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_slot: Option<CapabilitySlot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_family: Option<EndpointFamily>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_budget: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_budget: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_activation_plan: Option<SkillActivationPlanSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_policy: Option<AgentTaskPolicy>,
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

fn is_recoverable_checkpoint_status(status: &str) -> bool {
    matches!(
        status,
        "context_assembled" | "model_called" | "streaming" | "awaiting_tool_confirmation"
    )
}

/// Load checkpoint only for trace states that can legitimately resume.
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
            Ok((Some(json), status)) if is_recoverable_checkpoint_status(&status) => {
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
    fn estimate_tokens_counts_cjk_close_to_one_token_per_character() {
        assert!(estimate_tokens(&"汉".repeat(300)) >= 300);
        assert!(estimate_tokens(&"x".repeat(300)) <= 80);
    }

    #[test]
    fn compress_history_trims_single_long_recent_message() {
        let latest_user = "apple最新的手表是什么？".to_string();
        let history = vec![
            ("user".to_string(), "上一轮问题".to_string()),
            ("assistant".to_string(), "超长网页正文".repeat(10_000)),
            ("user".to_string(), latest_user.clone()),
        ];

        let out = compress_history_messages(&history);

        assert_eq!(out.last().unwrap().1, latest_user);
        assert!(out
            .iter()
            .all(|(_, content)| content.chars().count() < 8_000));
        assert!(out
            .iter()
            .any(|(_, content)| content.contains("已按上下文预算截断")));
    }

    #[test]
    fn load_checkpoint_rejects_non_recoverable_started_status() {
        let db = crate::storage::db::Database::open_in_memory().unwrap();
        let rid = "cp-started-stale";
        crate::ai_runtime::trace::TraceRecorder::start(
            &db,
            rid,
            crate::ai_runtime::AiScene::KnowledgeLookup,
        )
        .unwrap();
        let cp = sample_checkpoint(1);
        save_harness_checkpoint(&db, rid, &cp).unwrap();

        let loaded = load_harness_checkpoint(&db, rid).unwrap();

        assert!(
            loaded.is_none(),
            "started traces must not offer stale checkpoint recovery"
        );
    }

    #[test]
    fn load_checkpoint_allows_model_called_budget_pause_checkpoint() {
        let db = crate::storage::db::Database::open_in_memory().unwrap();
        let rid = "cp-model-called-budget";
        crate::ai_runtime::trace::TraceRecorder::start(
            &db,
            rid,
            crate::ai_runtime::AiScene::KnowledgeLookup,
        )
        .unwrap();
        crate::ai_runtime::trace::TraceRecorder::update_status(
            &db,
            rid,
            crate::ai_runtime::trace::TraceStatus::ModelCalled,
        )
        .unwrap();
        let cp = sample_checkpoint(2);
        save_harness_checkpoint(&db, rid, &cp).unwrap();

        let loaded = load_harness_checkpoint(&db, rid).unwrap();

        assert_eq!(loaded.unwrap().round, 2);
    }

    #[test]
    fn compress_history_preserves_conversation_memory_system_message() {
        let memory = (
            "system".to_string(),
            "## ConversationMemory\n目标: 完成 harness 修复并保留所有关键决策\n偏好: 小步提交\n决策: 不持久化正文".to_string(),
        );
        let mut history = vec![memory.clone()];
        history.extend((0..12).map(|i| ("user".to_string(), format!("message {i}"))));

        let out = compress_history_messages(&history);

        assert_eq!(out[0], memory);
        assert!(out[1].1.contains("[历史摘要]"));
        assert!(!out[1].1.contains("ConversationMemory"));
    }

    #[test]
    fn extract_thinking() {
        let (visible, think) = extract_thinking_blocks("答案<thinking>先检索法规</thinking>因此…");
        assert!(visible.contains("答案"));
        assert!(think.unwrap().contains("检索"));
    }

    fn sample_checkpoint(round: u32) -> HarnessCheckpoint {
        HarnessCheckpoint {
            meta: HarnessCheckpointMeta {
                scene: "knowledge_lookup".into(),
                session_id: 1,
                note_path: None,
                note_title: None,
                selection_excerpt: None,
                cold_start_packets: vec![],
                web_search_enabled: false,
                depth: 0,
                capability_slot: None,
                provider_id: None,
                model: None,
                endpoint_family: None,
                thinking: None,
                output_budget: None,
                input_budget: None,
                skill_activation_plan: None,
                task_policy: None,
            },
            round,
            messages: vec![],
            tool_calls: vec![],
            tool_results: vec![],
            evidence_packets: vec![],
            usage: crate::ai_runtime::model_gateway::TokenUsage::default(),
            usage_source: UsageSource::Provider,
            bonus_round_used: false,
        }
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
        let cp = sample_checkpoint(1);
        save_harness_checkpoint(&db, rid, &cp).unwrap();
        let loaded = load_harness_checkpoint(&db, rid).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().round, 1);
    }
}
