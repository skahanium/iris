//! Conversation memory summaries for long-running AI sessions.
//!
//! The memory layer stores bounded, traceable summaries of older turns. It keeps
//! sequence ranges and content hashes so later phases can reason about what was
//! summarized without storing raw prompt checkpoints.

use serde::{Deserialize, Serialize};

use crate::cas::hash::content_hash_str;
use crate::error::AppResult;
use crate::storage::db::Database;

const DEFAULT_MINIMUM_MESSAGES: usize = 7;
const DEFAULT_RECENT_MESSAGE_LIMIT: usize = 24;
const DETERMINISTIC_SUMMARY_LIMIT: usize = 220;
const MODEL_SUMMARY_FIELD_LIMIT: usize = 500;
const MODEL_SUMMARY_TOTAL_LIMIT: usize = 1_500;
const MODEL_COMPACTION_INPUT_LIMIT: usize = 12_000;
const MODEL_COMPACTION_MARKER: &str = "[模型压缩] ";

/// Policy knobs for deciding when and how much dialogue to summarize.
#[derive(Debug, Clone, Copy)]
pub struct ConversationMemoryPolicy {
    pub minimum_messages: usize,
    pub recent_message_limit: usize,
}

impl Default for ConversationMemoryPolicy {
    fn default() -> Self {
        Self {
            minimum_messages: DEFAULT_MINIMUM_MESSAGES,
            recent_message_limit: DEFAULT_RECENT_MESSAGE_LIMIT,
        }
    }
}

/// Durable summary of older turns in a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationMemory {
    pub id: i64,
    pub session_id: i64,
    pub seq_start: i64,
    pub seq_end: i64,
    pub content_hash: String,
    pub goal_summary: String,
    pub preference_summary: String,
    pub decision_summary: String,
    pub open_threads_summary: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
struct MemoryMessage {
    seq: i64,
    role: String,
    content: String,
    content_hash: Option<String>,
}

/// One bounded, no-tool compression request for a long-running conversation.
/// It is deliberately internal to the normal Run path: the request carries no
/// provider state and the resulting text is persisted only through the same
/// four durable summary fields used by the deterministic fallback.
#[derive(Debug, Clone)]
pub(crate) struct ConversationMemoryCompactionRequest {
    session_id: i64,
    seq_start: i64,
    seq_end: i64,
    content_hash: String,
    prompt: String,
    fallback: MemoryDraft,
}

impl ConversationMemoryCompactionRequest {
    pub(crate) fn prompt(&self) -> &str {
        &self.prompt
    }
}

impl ConversationMemory {
    /// Refresh the latest summary for a session when the dialogue is long enough.
    pub fn refresh_for_session(
        db: &Database,
        session_id: i64,
        policy: ConversationMemoryPolicy,
    ) -> AppResult<Option<Self>> {
        let messages = load_messages(db, session_id)?;
        let recent_limit = policy.recent_message_limit.max(1);
        let minimum = policy.minimum_messages.max(recent_limit + 1);
        if messages.len() < minimum {
            clear_for_session(db, session_id)?;
            return Ok(None);
        }

        let recent_limit = recent_limit.min(messages.len().saturating_sub(1));
        let summary_end_index = messages.len().saturating_sub(recent_limit + 1);
        let summarized = &messages[..=summary_end_index];
        let seq_start = summarized.first().map(|msg| msg.seq).unwrap_or(1);
        let seq_end = summarized.last().map(|msg| msg.seq).unwrap_or(seq_start);
        let memory = draft_for_messages(session_id, seq_start, seq_end, summarized);
        if let Some(existing) = Self::latest_for_session(db, session_id)? {
            if existing.seq_start == memory.seq_start
                && existing.seq_end == memory.seq_end
                && existing.content_hash == memory.content_hash
                && is_model_compacted(&existing)
            {
                return Ok(Some(existing));
            }
        }
        upsert_memory(db, memory)?;
        Self::latest_for_session(db, session_id)
    }

    /// Load the latest memory summary for a session.
    pub fn latest_for_session(db: &Database, session_id: i64) -> AppResult<Option<Self>> {
        db.with_read_conn(|conn| {
            let result = conn.query_row(
                "SELECT id, session_id, seq_start, seq_end, content_hash,
                        goal_summary, preference_summary, decision_summary,
                        open_threads_summary, created_at, updated_at
                 FROM conversation_summaries
                 WHERE session_id = ?1",
                [session_id],
                |row| {
                    Ok(Self {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        seq_start: row.get(2)?,
                        seq_end: row.get(3)?,
                        content_hash: row.get(4)?,
                        goal_summary: row.get(5)?,
                        preference_summary: row.get(6)?,
                        decision_summary: row.get(7)?,
                        open_threads_summary: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                    })
                },
            );
            match result {
                Ok(memory) => Ok(Some(memory)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(err.into()),
            }
        })
    }

    /// Load a summary only after revalidating its exact covered committed-message range.
    pub fn validated_for_session(db: &Database, session_id: i64) -> AppResult<Option<Self>> {
        let Some(memory) = Self::latest_for_session(db, session_id)? else {
            return Ok(None);
        };
        let messages = load_messages(db, session_id)?;
        let covered = messages
            .iter()
            .filter(|message| message.seq >= memory.seq_start && message.seq <= memory.seq_end)
            .cloned()
            .collect::<Vec<_>>();
        let valid = covered
            .first()
            .is_some_and(|message| message.seq == memory.seq_start)
            && covered
                .last()
                .is_some_and(|message| message.seq == memory.seq_end)
            && summarized_content_hash(&covered) == memory.content_hash;
        if valid {
            Ok(Some(memory))
        } else {
            Self::refresh_for_session(db, session_id, Default::default())
        }
    }

    /// Render a safe system prompt fragment for this memory summary.
    pub fn to_prompt_fragment(&self) -> String {
        format!(
            "## ConversationMemory\n\
             seq={}..{} content_hash={}\n\
             目标: {}\n\
             偏好: {}\n\
             决策: {}\n\
             待处理事项: {}",
            self.seq_start,
            self.seq_end,
            self.content_hash,
            display_summary(&self.goal_summary),
            display_summary(&self.preference_summary),
            display_summary(&self.decision_summary),
            display_summary(&self.open_threads_summary)
        )
    }

    /// Build one model compression request only when the current durable
    /// summary still comes from the deterministic fallback. A successfully
    /// compressed range is marked in the existing fields, so later Runs do not
    /// spend a hidden extra model turn until the covered range actually moves.
    pub(crate) fn pending_model_compaction(
        db: &Database,
        session_id: i64,
    ) -> AppResult<Option<ConversationMemoryCompactionRequest>> {
        let Some(memory) = Self::validated_for_session(db, session_id)? else {
            return Ok(None);
        };
        if is_model_compacted(&memory) {
            return Ok(None);
        }
        let covered = load_messages(db, session_id)?
            .into_iter()
            .filter(|message| message.seq >= memory.seq_start && message.seq <= memory.seq_end)
            .collect::<Vec<_>>();
        if covered.is_empty() || summarized_content_hash(&covered) != memory.content_hash {
            return Ok(None);
        }
        let fallback = draft_for_messages(session_id, memory.seq_start, memory.seq_end, &covered);
        let prompt = model_compaction_prompt(&memory, &covered);
        Ok(Some(ConversationMemoryCompactionRequest {
            session_id,
            seq_start: memory.seq_start,
            seq_end: memory.seq_end,
            content_hash: memory.content_hash,
            prompt,
            fallback,
        }))
    }

    /// Persist a bounded model compaction when it still matches the exact
    /// committed-message range. Invalid output and provider failure safely use
    /// the deterministic fallback and never block the user's active Run.
    pub(crate) fn apply_model_compaction(
        db: &Database,
        request: &ConversationMemoryCompactionRequest,
        output: Option<&str>,
    ) -> AppResult<()> {
        let Some(current) = Self::latest_for_session(db, request.session_id)? else {
            return Ok(());
        };
        if current.seq_start != request.seq_start
            || current.seq_end != request.seq_end
            || current.content_hash != request.content_hash
        {
            return Ok(());
        }
        let draft = parse_model_compaction(output.unwrap_or_default())
            .map(|summary| MemoryDraft {
                session_id: request.session_id,
                seq_start: request.seq_start,
                seq_end: request.seq_end,
                content_hash: request.content_hash.clone(),
                goal_summary: mark_model_compaction(summary.goal_summary),
                preference_summary: mark_model_compaction(summary.preference_summary),
                decision_summary: mark_model_compaction(summary.decision_summary),
                open_threads_summary: mark_model_compaction(summary.open_threads_summary),
            })
            .unwrap_or_else(|| request.fallback.clone());
        upsert_memory(db, draft)
    }
}

/// Build memory plus the latest recent turns for prompt assembly.
pub fn build_memory_prompt_messages(
    db: &Database,
    session_id: i64,
    recent_limit: usize,
) -> AppResult<Vec<(String, String)>> {
    let mut out = Vec::new();
    if let Some(memory) = ConversationMemory::validated_for_session(db, session_id)? {
        out.push(("system".to_string(), memory.to_prompt_fragment()));
    }
    let recent =
        crate::ai_runtime::normal_session_repository::NormalSessionRepository::recent_messages(
            db,
            session_id,
            recent_limit as u32,
        )?;
    out.extend(
        recent
            .into_iter()
            .filter(|msg| msg.role == "user" || msg.role == "assistant")
            .map(|msg| (msg.role, msg.content)),
    );
    Ok(out)
}

/// Build only the memory system fragment for Run history augmentation.
pub fn build_memory_system_message(
    db: &Database,
    session_id: i64,
) -> AppResult<Option<(String, String)>> {
    Ok(ConversationMemory::validated_for_session(db, session_id)?
        .map(|memory| ("system".to_string(), memory.to_prompt_fragment())))
}

#[derive(Debug, Clone)]
struct MemoryDraft {
    session_id: i64,
    seq_start: i64,
    seq_end: i64,
    content_hash: String,
    goal_summary: String,
    preference_summary: String,
    decision_summary: String,
    open_threads_summary: String,
}

#[derive(Debug, Deserialize)]
struct ModelCompactionSummary {
    goal_summary: String,
    preference_summary: String,
    decision_summary: String,
    open_threads_summary: String,
}

fn draft_for_messages(
    session_id: i64,
    seq_start: i64,
    seq_end: i64,
    messages: &[MemoryMessage],
) -> MemoryDraft {
    MemoryDraft {
        session_id,
        seq_start,
        seq_end,
        content_hash: summarized_content_hash(messages),
        goal_summary: extract_summary(
            messages,
            &["goal:", "Goal:", "目标：", "目标:"],
            &[
                "goal",
                "want",
                "need",
                "plan",
                "\u{60f3}",
                "\u{5e0c}\u{671b}",
                "\u{9700}\u{8981}",
            ],
            "goal",
            SummaryFallback::Goal,
        ),
        preference_summary: extract_summary(
            messages,
            &[
                "prefer:",
                "Preference:",
                "偏好：",
                "偏好:",
                "约束：",
                "约束:",
                "更正：",
                "更正:",
                "correction:",
            ],
            &[
                "prefer",
                "style",
                "avoid",
                "like",
                "\u{504f}\u{597d}",
                "\u{559c}\u{6b22}",
                "\u{4e0d}\u{8981}",
                "\u{66f4}\u{6b63}",
                "\u{64a4}\u{56de}",
                "constraint",
                "correction",
            ],
            "preference",
            SummaryFallback::Optional,
        ),
        decision_summary: extract_summary(
            messages,
            &["decision:", "Decision:", "已完成：", "已完成:"],
            &[
                "decision",
                "decided",
                "choose",
                "use ",
                "confirmed",
                "\u{51b3}\u{5b9a}",
                "\u{9009}\u{62e9}",
                "\u{91c7}\u{7528}",
            ],
            "decision",
            SummaryFallback::Optional,
        ),
        open_threads_summary: extract_summary(
            messages,
            &["open:", "待处理：", "待处理:", "下一步：", "下一步:"],
            &[
                "open",
                "todo",
                "next",
                "follow up",
                "question",
                "\u{5f85}\u{529e}",
                "\u{4e0b}\u{4e00}\u{6b65}",
                "\u{95ee}\u{9898}",
            ],
            "open",
            SummaryFallback::Optional,
        ),
    }
}

fn model_compaction_prompt(memory: &ConversationMemory, messages: &[MemoryMessage]) -> String {
    let prior = format!(
        "当前目标：{}\n最新约束与更正：{}\n已确认结果：{}\n未解决事项：{}",
        display_summary(&memory.goal_summary),
        display_summary(&memory.preference_summary),
        display_summary(&memory.decision_summary),
        display_summary(&memory.open_threads_summary),
    );
    let mut remaining = MODEL_COMPACTION_INPUT_LIMIT;
    let mut transcript = Vec::new();
    for message in messages.iter().rev() {
        if remaining == 0 {
            break;
        }
        let cleaned = redact_sensitive(&message.content);
        let excerpt = truncate_chars(&cleaned, remaining);
        remaining = remaining.saturating_sub(excerpt.chars().count());
        transcript.push(format!("{}: {}", message.role, excerpt));
    }
    transcript.reverse();
    format!(
        "将下列即将移出最近对话窗口的已提交消息压缩为 JSON 对象。\n\
         只输出 goal_summary、preference_summary、decision_summary、open_threads_summary 四个字符串字段。\n\
         保留最新用户纠正，区分已确认的工具/Host 结果与 assistant 的未核实说法；不要编造、不要保留来源 URL、不要输出解释。\n\
         四字段总计不超过 1500 个字符，每字段不超过 500 个字符。\n\n\
         旧摘要：\n{prior}\n\n待压缩消息：\n{}",
        transcript.join("\n")
    )
}

fn parse_model_compaction(raw: &str) -> Option<ModelCompactionSummary> {
    let trimmed = raw.trim();
    let trimmed = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .trim()
        .strip_suffix("```")
        .unwrap_or(trimmed)
        .trim();
    let parsed = serde_json::from_str::<ModelCompactionSummary>(trimmed).ok()?;
    let normalized = normalize_model_summary(parsed);
    (!normalized.goal_summary.is_empty()).then_some(normalized)
}

fn normalize_model_summary(summary: ModelCompactionSummary) -> ModelCompactionSummary {
    let mut remaining = MODEL_SUMMARY_TOTAL_LIMIT;
    let mut take = |value: String| {
        let bounded = truncate_chars(
            &redact_sensitive(&value),
            MODEL_SUMMARY_FIELD_LIMIT.min(remaining),
        );
        remaining = remaining.saturating_sub(bounded.chars().count());
        if bounded.is_empty() {
            not_recorded()
        } else {
            bounded
        }
    };
    ModelCompactionSummary {
        goal_summary: take(summary.goal_summary),
        preference_summary: take(summary.preference_summary),
        decision_summary: take(summary.decision_summary),
        open_threads_summary: take(summary.open_threads_summary),
    }
}

fn mark_model_compaction(summary: String) -> String {
    format!("{MODEL_COMPACTION_MARKER}{summary}")
}

fn is_model_compacted(memory: &ConversationMemory) -> bool {
    memory.goal_summary.starts_with(MODEL_COMPACTION_MARKER)
}

fn display_summary(summary: &str) -> &str {
    summary
        .strip_prefix(MODEL_COMPACTION_MARKER)
        .unwrap_or(summary)
}

fn load_messages(db: &Database, session_id: i64) -> AppResult<Vec<MemoryMessage>> {
    // Memory is prompt data, so it must use the exact same committed-turn
    // projection as recent Run history. Reading session_messages directly
    // would resurrect failed or still-active user turns into later prompts.
    crate::ai_runtime::normal_session_repository::NormalSessionRepository::recent_messages(
        db,
        session_id,
        u32::MAX,
    )
    .map(|messages| {
        messages
            .into_iter()
            .map(|message| MemoryMessage {
                seq: message.seq,
                role: message.role,
                content: message.content,
                content_hash: None,
            })
            .collect()
    })
}

fn summarized_content_hash(messages: &[MemoryMessage]) -> String {
    let hash_input = messages
        .iter()
        .map(|message| {
            let content_hash = message
                .content_hash
                .clone()
                .unwrap_or_else(|| content_hash_str(&message.content));
            format!("{}:{content_hash}", message.seq)
        })
        .collect::<Vec<_>>()
        .join("|");
    content_hash_str(&format!("count={};{hash_input}", messages.len()))
}

fn upsert_memory(db: &Database, draft: MemoryDraft) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO conversation_summaries
             (session_id, seq_start, seq_end, content_hash, goal_summary,
              preference_summary, decision_summary, open_threads_summary, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
             ON CONFLICT(session_id) DO UPDATE SET
                seq_start = excluded.seq_start,
                seq_end = excluded.seq_end,
                content_hash = excluded.content_hash,
                goal_summary = excluded.goal_summary,
                preference_summary = excluded.preference_summary,
                decision_summary = excluded.decision_summary,
                open_threads_summary = excluded.open_threads_summary,
                updated_at = excluded.updated_at",
            rusqlite::params![
                draft.session_id,
                draft.seq_start,
                draft.seq_end,
                draft.content_hash,
                draft.goal_summary,
                draft.preference_summary,
                draft.decision_summary,
                draft.open_threads_summary,
                now,
            ],
        )?;
        Ok(())
    })
}

fn clear_for_session(db: &Database, session_id: i64) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM conversation_summaries WHERE session_id = ?1",
            [session_id],
        )?;
        Ok(())
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SummaryFallback {
    Goal,
    Optional,
}

fn extract_summary(
    messages: &[MemoryMessage],
    markers: &[&str],
    hints: &[&str],
    fallback_label: &str,
    fallback: SummaryFallback,
) -> String {
    for message in messages
        .iter()
        .rev()
        .filter(|message| message.role == "user")
    {
        for marker in markers {
            if let Some(summary) = extract_after_marker(&message.content, marker) {
                return bounded_summary(&summary);
            }
        }
    }
    if let Some(summary) = extract_by_hints(messages, hints, fallback_label) {
        return summary;
    }
    match fallback {
        SummaryFallback::Goal => fallback_goal_summary(messages, fallback_label),
        SummaryFallback::Optional => not_recorded(),
    }
}

fn extract_by_hints(
    messages: &[MemoryMessage],
    hints: &[&str],
    fallback_label: &str,
) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|msg| {
            msg.role == "user"
                && !msg.content.trim().is_empty()
                && contains_any_hint(&msg.content, hints)
        })
        .map(|msg| bounded_summary(&format!("{fallback_label}: {}", msg.content.trim())))
}

fn contains_any_hint(content: &str, hints: &[&str]) -> bool {
    let lower = content.to_ascii_lowercase();
    hints.iter().any(|hint| {
        let hint_lower = hint.to_ascii_lowercase();
        lower.contains(&hint_lower) || content.contains(hint)
    })
}

fn fallback_goal_summary(messages: &[MemoryMessage], fallback_label: &str) -> String {
    let last = messages
        .iter()
        .rev()
        .find(|msg| msg.role == "user" && !msg.content.trim().is_empty())
        .map(|msg| msg.content.trim());
    match last {
        Some(last) => bounded_summary(&format!("{fallback_label}: {last}")),
        None => not_recorded(),
    }
}

fn not_recorded() -> String {
    "\u{672a}\u{8bb0}\u{5f55}".to_string()
}

fn extract_after_marker(content: &str, marker: &str) -> Option<String> {
    let start = content.find(marker)? + marker.len();
    let rest = content[start..].trim();
    if rest.is_empty() {
        return None;
    }
    let end = rest
        .char_indices()
        .find_map(|(idx, ch)| matches!(ch, '。' | '\n' | '\r').then_some(idx))
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn bounded_summary(text: &str) -> String {
    let safe = redact_sensitive(text.trim());
    let chars: String = safe.chars().take(DETERMINISTIC_SUMMARY_LIMIT).collect();
    if safe.chars().count() > DETERMINISTIC_SUMMARY_LIMIT {
        format!("{chars}...")
    } else if chars.is_empty() {
        "未记录".to_string()
    } else {
        chars
    }
}

fn truncate_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn redact_sensitive(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if lower.contains("api key")
        || lower.contains("apikey")
        || lower.contains("token")
        || lower.contains("password")
        || lower.contains("secret")
    {
        "[已省略敏感内容]".to_string()
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod memory_extraction_tests {
    use super::{
        extract_summary, ConversationMemory, ConversationMemoryPolicy, MemoryMessage,
        SummaryFallback, MODEL_COMPACTION_MARKER,
    };
    use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
    use crate::storage::db::Database;

    fn msg(seq: i64, role: &str, content: &str) -> MemoryMessage {
        MemoryMessage {
            seq,
            role: role.to_string(),
            content: content.to_string(),
            content_hash: None,
        }
    }

    #[test]
    fn natural_language_hints_populate_distinct_memory_fields() {
        let messages = vec![
            msg(1, "user", "I want a careful harness repair plan."),
            msg(2, "user", "Please avoid placeholder fixes."),
            msg(3, "user", "decision: Keep the privacy boundary."),
            msg(4, "user", "Next check frontend recovery."),
        ];

        let goal = extract_summary(
            &messages,
            &["goal:"],
            &["want"],
            "goal",
            SummaryFallback::Goal,
        );
        let preference = extract_summary(
            &messages,
            &["prefer:"],
            &["avoid"],
            "preference",
            SummaryFallback::Optional,
        );
        let decision = extract_summary(
            &messages,
            &["decision:"],
            &["decided"],
            "decision",
            SummaryFallback::Optional,
        );
        let open = extract_summary(
            &messages,
            &["open:"],
            &["next"],
            "open",
            SummaryFallback::Optional,
        );

        assert!(goal.contains("careful harness"));
        assert!(preference.contains("placeholder"));
        assert!(decision.contains("privacy boundary"));
        assert!(open.contains("frontend recovery"));
    }

    #[test]
    fn optional_memory_fields_do_not_duplicate_first_user_message_without_evidence() {
        let messages = vec![msg(1, "user", "General chat without preference markers.")];

        let preference = extract_summary(
            &messages,
            &["prefer:"],
            &["nonexistent-hint"],
            "preference",
            SummaryFallback::Optional,
        );

        assert_eq!(preference, "\u{672a}\u{8bb0}\u{5f55}");
    }

    #[test]
    fn first_user_message_is_not_permanent_goal() {
        let messages = vec![
            msg(1, "user", "What is the weather in Paris?"),
            msg(2, "assistant", "I don't know yet."),
            msg(3, "user", "请总结这份资料"),
        ];

        let goal = extract_summary(&messages, &[], &[], "goal", SummaryFallback::Goal);

        assert!(
            !goal.contains("What is the weather in Paris?"),
            "the first user message must not become the permanent goal of later unrelated requests"
        );
    }

    #[test]
    fn latest_explicit_correction_is_preserved_as_a_constraint() {
        let messages = vec![
            msg(1, "user", "目标：准备代号甲的摘要。"),
            msg(2, "assistant", "收到。"),
            msg(3, "user", "更正：代号应为乙，撤回甲；保持简洁中文。"),
        ];

        let preference = extract_summary(
            &messages,
            &["更正：", "更正:"],
            &["更正", "撤回"],
            "preference",
            SummaryFallback::Optional,
        );

        assert!(preference.contains("代号应为乙"));
        assert!(preference.contains("撤回甲"));
    }

    #[test]
    fn refresh_keeps_summary_and_recent_window_disjoint_at_twenty_five_messages() {
        let db = Database::open_in_memory().expect("database");
        let session = NormalSessionRepository::create(&db).expect("session");
        db.with_conn(|conn| {
            for seq in 1..=25 {
                conn.execute(
                    "INSERT INTO session_messages
                     (session_id, seq, role, content, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        session.session_id,
                        seq,
                        if seq % 2 == 0 { "assistant" } else { "user" },
                        format!("message-{seq}"),
                        format!("2026-07-27T00:00:0{seq}Z"),
                    ],
                )?;
            }
            Ok(())
        })
        .expect("seed conversation");

        let memory =
            ConversationMemory::refresh_for_session(&db, session.session_id, Default::default())
                .expect("refresh")
                .expect("twenty-five messages require memory");
        let recent = NormalSessionRepository::recent_messages(&db, session.session_id, 24)
            .expect("recent history");

        assert_eq!((memory.seq_start, memory.seq_end), (1, 1));
        assert_eq!(recent.first().expect("recent").seq, 2);
        assert!(memory.seq_end < recent.first().expect("recent").seq);
    }

    #[test]
    fn model_compaction_replaces_keyword_fallback_once_and_preserves_prompt_bounds() {
        let db = Database::open_in_memory().expect("database");
        let session = NormalSessionRepository::create(&db).expect("session");
        db.with_conn(|conn| {
            for seq in 1..=27_i64 {
                let content = match seq {
                    1 => "目标：准备甲项目的说明。".to_string(),
                    3 => "更正：目标改为乙项目，保持中文并撤回甲。".to_string(),
                    _ => format!("message-{seq}"),
                };
                conn.execute(
                    "INSERT INTO session_messages
                     (session_id, seq, role, content, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        session.session_id,
                        seq,
                        if seq % 2 == 0 { "assistant" } else { "user" },
                        content,
                        format!("2026-09-02T00:00:{seq:02}Z"),
                    ],
                )?;
            }
            Ok(())
        })
        .expect("seed conversation");
        ConversationMemory::refresh_for_session(&db, session.session_id, Default::default())
            .expect("fallback summary");
        let request = ConversationMemory::pending_model_compaction(&db, session.session_id)
            .expect("pending request")
            .expect("deterministic fallback needs one model compaction");
        assert!(request.prompt().contains("撤回甲"));
        ConversationMemory::apply_model_compaction(
            &db,
            &request,
            Some(
                r#"{"goal_summary":"乙项目说明","preference_summary":"中文；撤回甲","decision_summary":"未记录","open_threads_summary":"完成说明"}"#,
            ),
        )
        .expect("persist model compaction");

        let memory = ConversationMemory::latest_for_session(&db, session.session_id)
            .expect("read memory")
            .expect("memory");
        assert!(memory.goal_summary.starts_with(MODEL_COMPACTION_MARKER));
        assert!(memory.to_prompt_fragment().contains("乙项目说明"));
        assert!(!memory
            .to_prompt_fragment()
            .contains(MODEL_COMPACTION_MARKER));
        assert!(
            ConversationMemory::pending_model_compaction(&db, session.session_id)
                .expect("read pending")
                .is_none()
        );
        let refreshed =
            ConversationMemory::refresh_for_session(&db, session.session_id, Default::default())
                .expect("refresh")
                .expect("memory");
        assert!(refreshed.goal_summary.starts_with(MODEL_COMPACTION_MARKER));
    }

    #[test]
    fn invalid_model_compaction_output_falls_back_without_blocking_memory_refresh() {
        let db = Database::open_in_memory().expect("database");
        let session = NormalSessionRepository::create(&db).expect("session");
        db.with_conn(|conn| {
            for seq in 1..=25_i64 {
                conn.execute(
                    "INSERT INTO session_messages
                     (session_id, seq, role, content, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        session.session_id,
                        seq,
                        if seq % 2 == 0 { "assistant" } else { "user" },
                        format!("message-{seq}"),
                        format!("2026-09-02T00:01:{seq:02}Z"),
                    ],
                )?;
            }
            Ok(())
        })
        .expect("seed conversation");
        ConversationMemory::refresh_for_session(&db, session.session_id, Default::default())
            .expect("fallback summary");
        let request = ConversationMemory::pending_model_compaction(&db, session.session_id)
            .expect("pending")
            .expect("request");
        ConversationMemory::apply_model_compaction(&db, &request, Some("not json"))
            .expect("fallback persists");
        let memory = ConversationMemory::latest_for_session(&db, session.session_id)
            .expect("read")
            .expect("memory");
        assert!(!memory.goal_summary.starts_with(MODEL_COMPACTION_MARKER));
    }

    #[test]
    fn summary_invalidates_when_covered_messages_change() {
        let db = Database::open_in_memory().expect("database");
        let session = NormalSessionRepository::create(&db).expect("session");
        db.with_conn(|conn| {
            for seq in 1..=5_i64 {
                conn.execute(
                    "INSERT INTO session_messages
                     (session_id, seq, role, content, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        session.session_id,
                        seq,
                        if seq % 2 == 0 { "assistant" } else { "user" },
                        format!("covered-message-{seq}"),
                        format!("2026-07-27T00:00:0{seq}Z"),
                    ],
                )?;
            }
            Ok(())
        })
        .expect("seed conversation");

        let policy = ConversationMemoryPolicy {
            minimum_messages: 3,
            recent_message_limit: 1,
        };
        let before = ConversationMemory::refresh_for_session(&db, session.session_id, policy)
            .expect("refresh before")
            .expect("memory exists");
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE session_messages SET content = ?1
                 WHERE session_id = ?2 AND seq = ?3",
                rusqlite::params!["covered-message-changed", session.session_id, 2],
            )?;
            Ok(())
        })
        .expect("change a covered message");

        let after = ConversationMemory::refresh_for_session(&db, session.session_id, policy)
            .expect("refresh after")
            .expect("memory still exists");
        assert_ne!(
            before.content_hash, after.content_hash,
            "a changed covered message must invalidate the old summary hash"
        );
    }

    #[test]
    fn stale_summary_is_revalidated_before_context_assembly() {
        let db = Database::open_in_memory().expect("database");
        let session = NormalSessionRepository::create(&db).expect("session");
        db.with_conn(|conn| {
            for seq in 1..=25_i64 {
                conn.execute(
                    "INSERT INTO session_messages
                     (session_id, seq, role, content, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        session.session_id,
                        seq,
                        if seq % 2 == 0 { "assistant" } else { "user" },
                        format!("context-message-{seq}"),
                        format!("2026-08-18T00:00:{seq:02}Z"),
                    ],
                )?;
            }
            Ok(())
        })
        .expect("seed conversation");
        let before =
            ConversationMemory::refresh_for_session(&db, session.session_id, Default::default())
                .expect("initial refresh")
                .expect("summary");
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE session_messages SET content = 'changed covered message'
                 WHERE session_id = ?1 AND seq = 1",
                [session.session_id],
            )?;
            Ok(())
        })
        .expect("mutate covered message");

        let after = ConversationMemory::validated_for_session(&db, session.session_id)
            .expect("validated read")
            .expect("refreshed summary");
        assert_ne!(after.content_hash, before.content_hash);
    }

    #[test]
    fn messages_after_summary_range_do_not_invalidate_existing_summary() {
        let db = Database::open_in_memory().expect("database");
        let session = NormalSessionRepository::create(&db).expect("session");
        db.with_conn(|conn| {
            for seq in 1..=25_i64 {
                conn.execute(
                    "INSERT INTO session_messages
                     (session_id, seq, role, content, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        session.session_id,
                        seq,
                        if seq % 2 == 0 { "assistant" } else { "user" },
                        format!("message-{seq}"),
                        format!("2026-08-18T00:00:{seq:02}Z"),
                    ],
                )?;
            }
            Ok(())
        })
        .expect("seed conversation");
        let before =
            ConversationMemory::refresh_for_session(&db, session.session_id, Default::default())
                .expect("refresh")
                .expect("summary");
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO session_messages
                 (session_id, seq, role, content, created_at)
                 VALUES (?1, 26, 'assistant', 'new recent message', ?2)",
                rusqlite::params![session.session_id, "2026-08-18T00:00:26Z"],
            )?;
            Ok(())
        })
        .expect("append recent message");

        let after = ConversationMemory::validated_for_session(&db, session.session_id)
            .expect("validated read")
            .expect("summary");
        assert_eq!(after.content_hash, before.content_hash);
        assert_eq!(
            (after.seq_start, after.seq_end),
            (before.seq_start, before.seq_end)
        );
    }
}
