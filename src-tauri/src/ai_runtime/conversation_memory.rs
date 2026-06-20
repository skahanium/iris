//! Conversation memory summaries for long-running AI sessions.
//!
//! The memory layer stores bounded, traceable summaries of older turns. It keeps
//! sequence ranges and content hashes so later phases can reason about what was
//! summarized without storing raw prompt checkpoints.

use serde::{Deserialize, Serialize};

use crate::cas::hash::content_hash_str;
use crate::error::AppResult;
use crate::storage::db::Database;

const DEFAULT_MINIMUM_MESSAGES: usize = 20;
const DEFAULT_RECENT_MESSAGE_LIMIT: usize = 4;
const SUMMARY_LIMIT: usize = 220;

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

impl ConversationMemory {
    /// Refresh the latest summary for a session when the dialogue is long enough.
    pub fn refresh_for_session(
        db: &Database,
        session_id: i64,
        policy: ConversationMemoryPolicy,
    ) -> AppResult<Option<Self>> {
        let messages = load_messages(db, session_id)?;
        let minimum = policy.minimum_messages.max(1);
        if messages.len() < minimum {
            return Ok(None);
        }

        let recent_limit = policy
            .recent_message_limit
            .min(messages.len().saturating_sub(1));
        let summary_end_index = messages.len().saturating_sub(recent_limit + 1);
        let summarized = &messages[..=summary_end_index];
        let seq_start = summarized.first().map(|msg| msg.seq).unwrap_or(1);
        let seq_end = summarized.last().map(|msg| msg.seq).unwrap_or(seq_start);
        let hash_input = summarized
            .iter()
            .map(|msg| {
                msg.content_hash
                    .clone()
                    .unwrap_or_else(|| content_hash_str(&msg.content))
            })
            .collect::<Vec<_>>()
            .join("|");

        let memory = MemoryDraft {
            session_id,
            seq_start,
            seq_end,
            content_hash: content_hash_str(&hash_input),
            goal_summary: extract_summary(
                summarized,
                &["目标:", "目标：", "goal:", "Goal:"],
                "目标",
            ),
            preference_summary: extract_summary(
                summarized,
                &["偏好:", "偏好：", "prefer:", "Preference:"],
                "偏好",
            ),
            decision_summary: extract_summary(
                summarized,
                &["决定:", "决定：", "decision:", "Decision:"],
                "决策",
            ),
            open_threads_summary: extract_summary(
                summarized,
                &["开放问题:", "开放问题：", "待办:", "待办：", "open:"],
                "待处理事项",
            ),
        };
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
            self.goal_summary,
            self.preference_summary,
            self.decision_summary,
            self.open_threads_summary
        )
    }
}

/// Build memory plus the latest recent turns for prompt assembly.
pub fn build_memory_prompt_messages(
    db: &Database,
    session_id: i64,
    recent_limit: usize,
) -> AppResult<Vec<(String, String)>> {
    let mut out = Vec::new();
    if let Some(memory) = ConversationMemory::latest_for_session(db, session_id)? {
        out.push(("system".to_string(), memory.to_prompt_fragment()));
    }
    let recent = crate::ai_runtime::session::SessionManager::recent_messages(
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

/// Build only the memory system fragment for harness history augmentation.
pub fn build_memory_system_message(
    db: &Database,
    session_id: i64,
) -> AppResult<Option<(String, String)>> {
    Ok(ConversationMemory::latest_for_session(db, session_id)?
        .map(|memory| ("system".to_string(), memory.to_prompt_fragment())))
}

#[derive(Debug)]
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

fn load_messages(db: &Database, session_id: i64) -> AppResult<Vec<MemoryMessage>> {
    db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT seq, role, content, content_hash
             FROM session_messages
             WHERE session_id = ?1 AND role IN ('user', 'assistant')
             ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map([session_id], |row| {
            Ok(MemoryMessage {
                seq: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                content_hash: row.get(3)?,
            })
        })?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    })
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

fn extract_summary(messages: &[MemoryMessage], markers: &[&str], fallback_label: &str) -> String {
    for message in messages {
        for marker in markers {
            if let Some(summary) = extract_after_marker(&message.content, marker) {
                return bounded_summary(&summary);
            }
        }
    }
    let fallback = messages
        .iter()
        .find(|msg| msg.role == "user" && !msg.content.trim().is_empty())
        .map(|msg| msg.content.as_str())
        .unwrap_or("未记录");
    bounded_summary(&format!("{fallback_label}: {fallback}"))
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
    let chars: String = safe.chars().take(SUMMARY_LIMIT).collect();
    if safe.chars().count() > SUMMARY_LIMIT {
        format!("{chars}...")
    } else if chars.is_empty() {
        "未记录".to_string()
    } else {
        chars
    }
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
