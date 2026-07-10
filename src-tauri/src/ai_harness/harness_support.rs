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

/// Extract reasoning tags for UI (stripped from visible content).
pub fn extract_thinking_blocks(content: &str) -> (String, Option<String>) {
    if !contains_reasoning_open_tag(content) {
        return (content.to_string(), None);
    }
    let mut thinking = String::new();
    let mut visible = String::new();
    let mut cursor = 0usize;
    while let Some(open) = find_next_reasoning_open(content, cursor) {
        visible.push_str(&content[cursor..open.start]);
        let body_start = open.start + open.open_len;
        if let Some(close_start) = find_ascii_case_insensitive(content, open.close_tag, body_start)
        {
            thinking.push_str(content[body_start..close_start].trim());
            thinking.push('\n');
            cursor = close_start + open.close_tag.len();
        } else {
            thinking.push_str(content[body_start..].trim());
            cursor = content.len();
            break;
        }
    }
    visible.push_str(&content[cursor..]);
    let thinking_opt = if thinking.trim().is_empty() {
        None
    } else {
        Some(thinking.trim().to_string())
    };
    (visible.trim().to_string(), thinking_opt)
}

/// Strip reasoning tags from visible output and optionally surface safe event metadata.
pub fn extract_thinking_blocks_for_event(
    content: &str,
    allow_thinking_event: bool,
) -> (String, Option<String>) {
    let (visible, thinking) = extract_thinking_blocks(content);
    if allow_thinking_event {
        (visible, thinking)
    } else {
        (visible, None)
    }
}

#[derive(Debug, Clone, Copy)]
struct ReasoningOpen {
    start: usize,
    open_len: usize,
    close_tag: &'static str,
}

fn contains_reasoning_open_tag(content: &str) -> bool {
    find_next_reasoning_open(content, 0).is_some()
}

fn find_next_reasoning_open(content: &str, from: usize) -> Option<ReasoningOpen> {
    const TAGS: [(&str, &str); 3] = [
        ("<thinking>", "</thinking>"),
        ("<think>", "</think>"),
        ("<reasoning>", "</reasoning>"),
    ];
    let mut best: Option<ReasoningOpen> = None;
    for (open, close) in TAGS {
        if let Some(start) = find_ascii_case_insensitive(content, open, from) {
            if best.map_or(true, |candidate| start < candidate.start) {
                best = Some(ReasoningOpen {
                    start,
                    open_len: open.len(),
                    close_tag: close,
                });
            }
        }
    }
    best
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || bytes.len() < needle.len() || from > bytes.len() - needle.len() {
        return None;
    }
    (from..=bytes.len() - needle.len())
        .find(|&idx| bytes[idx..idx + needle.len()].eq_ignore_ascii_case(needle))
}

/// Remove opening paragraphs that are model-internal planning rather than an answer.
pub fn sanitize_meta_analysis_prefix(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || !looks_like_meta_analysis_prefix(trimmed) {
        return trimmed.to_string();
    }

    let mut kept = Vec::new();
    let mut dropping = true;
    for paragraph in trimmed.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        if dropping && looks_like_meta_analysis_paragraph(paragraph) {
            continue;
        }
        dropping = false;
        kept.push(paragraph);
    }
    if kept.is_empty() {
        String::new()
    } else {
        kept.join("\n\n")
    }
}

fn looks_like_meta_analysis_prefix(text: &str) -> bool {
    looks_like_meta_analysis_paragraph(text.lines().next().unwrap_or(text))
}

fn looks_like_meta_analysis_paragraph(paragraph: &str) -> bool {
    let trimmed = paragraph.trim_start();
    if trimmed.is_empty() {
        return false;
    }

    // English patterns — use ASCII lowercase for case-insensitive matching.
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("the user ")
        || lower.starts_with("the user is ")
        || lower.starts_with("this is a ")
        || lower.starts_with("the current task ")
        || lower.starts_with("i should ")
        || lower.starts_with("i'll ")
        || lower.starts_with("the persona ")
        || lower.contains("current task focus")
        || lower.contains("persona is")
    {
        return true;
    }

    // Chinese patterns — Chinese characters are unaffected by ASCII lowercasing,
    // so we match against the original trimmed text.
    //
    // 用户意图描述 / user-intent restatements
    if trimmed.starts_with("用户的问题是")
        || trimmed.starts_with("用户想要")
        || trimmed.starts_with("用户询问")
        || trimmed.starts_with("用户希望")
        || trimmed.starts_with("用户的需求")
        || trimmed.starts_with("用户要求")
    {
        return true;
    }
    // 自我规划 / self-planning
    if trimmed.starts_with("我需要")
        || trimmed.starts_with("我应该")
        || trimmed.starts_with("我将")
        || trimmed.starts_with("让我")
        || trimmed.starts_with("我来")
        || trimmed.starts_with("我先")
        || trimmed.starts_with("首先我")
        || trimmed.starts_with("接下来我")
        || trimmed.starts_with("然后我")
    {
        return true;
    }
    // 任务理解 / task comprehension
    if trimmed.starts_with("当前任务")
        || trimmed.starts_with("任务重点")
        || trimmed.starts_with("根据系统提示")
    {
        return true;
    }
    // 自我确认 / self-acknowledgement
    if trimmed.starts_with("好的，")
        || trimmed.starts_with("明白了")
        || trimmed.starts_with("收到，")
        || trimmed.starts_with("了解，")
    {
        return true;
    }

    false
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
    #[serde(default)]
    pub context_scope: crate::ai_runtime::retrieval_scope::ContextScopeDto,
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

fn scrub_checkpoint_internal_reasoning(checkpoint: &mut HarnessCheckpoint) {
    for message in &mut checkpoint.messages {
        message.reasoning_content = None;
    }
}

/// Save harness checkpoint to `ai_traces`.
pub fn save_harness_checkpoint(
    db: &Database,
    request_id: &str,
    checkpoint: &HarnessCheckpoint,
) -> AppResult<()> {
    let mut checkpoint = checkpoint.clone();
    scrub_checkpoint_internal_reasoning(&mut checkpoint);
    let value = serde_json::to_value(&checkpoint)
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
                let mut cp: HarnessCheckpoint = match serde_json::from_str(&json) {
                    Ok(cp) => cp,
                    Err(_) => return Ok(None),
                };
                scrub_checkpoint_internal_reasoning(&mut cp);
                Ok(Some(cp))
            }
            Ok(_) => Ok(None),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
}

/// Heuristic check for answers that look like incomplete fragments,
/// document excerpts, or truncated output rather than complete responses.
///
/// This is intentionally conservative — it only flags obvious fragments
/// and should not interfere with normal short answers (greetings,
/// confirmations, etc.).
pub fn looks_like_incomplete_final_answer(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }

    // Very short content that looks like a raw document excerpt
    // rather than a conversational answer.
    if trimmed.chars().count() < 30 {
        // Legal clause number pattern: "第X条" or "刑法第X条"
        if trimmed.starts_with('《')
            || trimmed.contains("第") && trimmed.contains("条")
            || trimmed.contains("第") && trimmed.contains("款")
        {
            return true;
        }
        // Pure heading / title without body text
        if trimmed.starts_with('#') && !trimmed.contains('\n') {
            return true;
        }
        // Isolated blockquote without analysis
        if trimmed.starts_with('>') && !trimmed.contains('\n') {
            return true;
        }
        // Only contains citation references like "[1]" or "[法规]"
        // with no surrounding analysis text.
        let without_citations = trimmed.replace(&['[', ']'][..], "").trim().to_string();
        if without_citations
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_whitespace())
        {
            return true;
        }
    }

    // Moderately short content that lacks sentence-ending punctuation
    // is likely a truncated or mid-thought fragment.
    if trimmed.chars().count() < 50 {
        let last_char = trimmed.chars().last().unwrap_or(' ');
        let has_end_punct = matches!(last_char, '。' | '！' | '？' | '.' | '!' | '?' | '…');
        if !has_end_punct {
            return true;
        }
    }

    // Sentence appears truncated: ends with a comma or enumeration marker
    // but no proper sentence-ending punctuation.
    let ends_truncated = trimmed.ends_with(',')
        || trimmed.ends_with('，')
        || trimmed.ends_with(';')
        || trimmed.ends_with('；')
        || trimmed.ends_with("等等")
        || trimmed.ends_with("等");
    if ends_truncated {
        // Only flag if there's no sentence-ending punctuation anywhere
        // in the last 50 characters (allow "A, B, and C。" style endings).
        let tail: String = trimmed.chars().rev().take(50).collect();
        let has_end_punct = tail.contains('。')
            || tail.contains('！')
            || tail.contains('？')
            || tail.contains('.')
            || tail.contains('!')
            || tail.contains('?');
        if !has_end_punct {
            return true;
        }
    }

    false
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

    #[test]
    fn extract_think_and_reasoning_tags_case_insensitively() {
        let (visible, think) =
            extract_thinking_blocks("答<THINK>hidden</THINK>案<reasoning>plan</reasoning>");

        assert_eq!(visible, "答案");
        let think = think.unwrap();
        assert!(think.contains("hidden"));
        assert!(think.contains("plan"));
    }

    #[test]
    fn extract_unclosed_think_tag_does_not_leak() {
        let (visible, think) = extract_thinking_blocks("可见<think>内部推理未闭合");

        assert_eq!(visible, "可见");
        assert_eq!(think.as_deref(), Some("内部推理未闭合"));
    }

    #[test]
    fn suppresses_thinking_event_when_reasoning_is_off_but_still_cleans_visible_text() {
        let (visible, think) =
            extract_thinking_blocks_for_event("答复<think>hidden plan</think>", false);

        assert_eq!(visible, "答复");
        assert!(think.is_none());
    }

    #[test]
    fn keeps_thinking_event_when_reasoning_is_requested() {
        let (visible, think) =
            extract_thinking_blocks_for_event("答复<think>hidden plan</think>", true);

        assert_eq!(visible, "答复");
        assert_eq!(think.as_deref(), Some("hidden plan"));
    }

    #[test]
    fn sanitize_meta_analysis_removes_minimax_style_prefix() {
        let raw = "The user is greeting me with a simple \"你好?\".\n\nI should respond warmly.\n\n你好呀！\n\n我在。";

        let cleaned = sanitize_meta_analysis_prefix(raw);

        assert_eq!(cleaned, "你好呀！\n\n我在。");
    }

    #[test]
    fn sanitize_meta_analysis_preserves_normal_chinese_answer() {
        let raw = "我觉得这个问题可以分两步看。\n\n第一步是确认事实。";

        assert_eq!(sanitize_meta_analysis_prefix(raw), raw);
    }

    #[test]
    fn sanitize_meta_analysis_removes_chinese_self_talk_prefix() {
        // Chinese meta-analysis patterns that should be stripped.
        let cases = [
            (
                "用户的问题是《刑法》第三百八十五条的适用范围。\n\n让我来分析一下。\n\n该条款规定…",
                "该条款规定…",
            ),
            (
                "用户想要分析这个案例。\n\n我需要先查阅相关法规。\n\n根据《刑法》第385条…",
                "根据《刑法》第385条…",
            ),
            (
                "当前任务是进行纪法案例分析。\n\n首先我需要检索相关法律条文。\n\n分析如下：…",
                "分析如下：…",
            ),
            (
                "好的，我来帮您分析这个纪法案例。\n\n这涉及以下法律条款…",
                "这涉及以下法律条款…",
            ),
            (
                "明白了。用户的需求是案例法律分析。\n\n我先梳理关键事实。\n\n本案中…",
                "本案中…",
            ),
        ];

        for (raw, expected) in cases {
            let cleaned = sanitize_meta_analysis_prefix(raw);
            assert_eq!(
                cleaned, expected,
                "Failed to strip Chinese meta-analysis prefix.\nInput: {raw}\nExpected: {expected}\nGot: {cleaned}"
            );
        }
    }

    #[test]
    fn looks_incomplete_flags_legal_clause_fragment() {
        assert!(looks_like_incomplete_final_answer("《刑法》第三百八十五条"));
        assert!(looks_like_incomplete_final_answer("刑法第385条规定"));
        assert!(looks_like_incomplete_final_answer("  第三百八十五条  "));
    }

    #[test]
    fn looks_incomplete_flags_pure_heading() {
        assert!(looks_like_incomplete_final_answer("# 第三章 刑罚"));
    }

    #[test]
    fn looks_incomplete_flags_isolated_quote() {
        assert!(looks_like_incomplete_final_answer(
            "> 国家工作人员利用职务上的便利"
        ));
    }

    #[test]
    fn looks_incomplete_flags_truncated_sentence() {
        assert!(looks_like_incomplete_final_answer("根据上述分析，本案涉及"));
        assert!(looks_like_incomplete_final_answer(
            "需要考虑以下几个方面：第一，主体要件；第二，"
        ));
        assert!(looks_like_incomplete_final_answer(
            "相关规定包括刑法、监察法等等"
        ));
    }

    #[test]
    fn looks_incomplete_allows_normal_short_answers() {
        assert!(!looks_like_incomplete_final_answer("好的。"));
        assert!(!looks_like_incomplete_final_answer(
            "你好！请问有什么可以帮助你的？"
        ));
        assert!(!looks_like_incomplete_final_answer("收到。"));
    }

    #[test]
    fn looks_incomplete_allows_complete_answers() {
        assert!(!looks_like_incomplete_final_answer(
            "本案涉及贪污罪的认定。根据《刑法》第三百八十二条，贪污罪是指国家工作人员利用职务上的便利，侵吞、窃取、骗取或者以其他手段非法占有公共财物的行为。本案中，被告人的行为符合该罪的构成要件。"
        ));
        assert!(!looks_like_incomplete_final_answer(
            "根据相关法律规定，我建议从以下几个方面进行辩护：第一，证据不足；第二，程序违法。"
        ));
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
                context_scope: Default::default(),
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
