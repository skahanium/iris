//! Deliberation state and completion verification for complex agent tasks.
//!
//! This module keeps the phase-4 state small and inspectable: goal, outline,
//! assumptions, open questions, evidence gaps, and verification items. It does
//! not store raw prompts, full messages, or note bodies.

use serde::{Deserialize, Serialize};

use crate::ai_harness::harness::HarnessFinishReason;
use crate::ai_runtime::ContextPacket;
use crate::error::AppResult;
use crate::storage::db::Database;

const GOAL_LIMIT: usize = 240;

/// Inputs needed to initialize a deliberation state for one harness run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliberationInput {
    pub request_id: String,
    pub session_id: i64,
    pub user_goal: String,
    pub evidence_packet_count: usize,
    pub tool_result_count: usize,
    pub max_rounds: u32,
    pub token_budget: u32,
}

/// Status of a verification item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Pending,
    Passed,
    Failed,
}

/// One item in the completion checklist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationItem {
    pub id: String,
    pub description: String,
    pub status: VerificationStatus,
}

/// Persistent state for complex-task deliberation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliberationState {
    pub request_id: String,
    pub session_id: i64,
    pub current_goal: String,
    pub plan_outline: Vec<String>,
    pub assumptions: Vec<String>,
    pub open_questions: Vec<String>,
    pub evidence_gaps: Vec<String>,
    pub verification_items: Vec<VerificationItem>,
    pub status: String,
}

/// Result of running the completion verification checklist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationSummary {
    pub passed: bool,
    pub items: Vec<VerificationItem>,
}

/// User-facing severity for completion verification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationNoticeStatus {
    AnswerWithCaveat,
    PausedForRecovery,
}

/// Compact user-facing notice for verification gaps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationNotice {
    pub status: VerificationNoticeStatus,
    pub message: String,
    pub failed_items: Vec<String>,
}

impl DeliberationState {
    /// Create a bounded deliberation state from the current run context.
    pub fn from_input(input: DeliberationInput) -> Self {
        let current_goal = bounded(&input.user_goal);
        let mut evidence_gaps = Vec::new();
        if input.evidence_packet_count == 0 {
            evidence_gaps
                .push("缺少可引用证据，需要在最终回答前补足或明确说明无需外部证据".to_string());
        }

        Self {
            request_id: input.request_id,
            session_id: input.session_id,
            current_goal,
            plan_outline: vec![
                "明确用户目标与阶段验收边界".to_string(),
                "优先复用现有 runtime、测试和文档契约".to_string(),
                "在限定轮次与 token 预算内完成并保留恢复信息".to_string(),
            ],
            assumptions: vec![
                format!("max_rounds={}", input.max_rounds),
                format!("token_budget={}", input.token_budget),
                format!("tool_result_count={}", input.tool_result_count),
            ],
            open_questions: Vec::new(),
            evidence_gaps,
            verification_items: vec![
                VerificationItem {
                    id: "non_empty_answer".into(),
                    description: "最终回答非空".into(),
                    status: VerificationStatus::Pending,
                },
                VerificationItem {
                    id: "acceptance_covered".into(),
                    description: "阶段验收项已覆盖".into(),
                    status: VerificationStatus::Pending,
                },
                VerificationItem {
                    id: "evidence_accounted".into(),
                    description: "引用证据或明确说明无需外部证据".into(),
                    status: VerificationStatus::Pending,
                },
                VerificationItem {
                    id: "finish_reason_completed".into(),
                    description: "harness 以 completed 状态完成".into(),
                    status: VerificationStatus::Pending,
                },
            ],
            status: "running".into(),
        }
    }
}

/// Verify whether a final answer satisfies the deliberation checklist.
pub fn verify_completion(
    mut state: DeliberationState,
    final_answer: &str,
    evidence_packets: &[ContextPacket],
    finish_reason: HarnessFinishReason,
) -> VerificationSummary {
    let answer = final_answer.trim();
    for item in &mut state.verification_items {
        item.status = match item.id.as_str() {
            "non_empty_answer" if !answer.is_empty() => VerificationStatus::Passed,
            "acceptance_covered" if answer.contains("验收") || answer.contains("完成") => {
                VerificationStatus::Passed
            }
            "evidence_accounted" if has_citation(answer, evidence_packets) => {
                VerificationStatus::Passed
            }
            "finish_reason_completed" if finish_reason == HarnessFinishReason::Completed => {
                VerificationStatus::Passed
            }
            _ => VerificationStatus::Failed,
        };
    }
    let passed = state
        .verification_items
        .iter()
        .all(|item| item.status == VerificationStatus::Passed);
    VerificationSummary {
        passed,
        items: state.verification_items,
    }
}

/// Build a compact notice when verification did not fully pass.
pub fn verification_notice(
    summary: &VerificationSummary,
    finish_reason: HarnessFinishReason,
) -> Option<VerificationNotice> {
    if summary.passed {
        return None;
    }
    let failed_items = summary
        .items
        .iter()
        .filter(|item| item.status == VerificationStatus::Failed)
        .map(|item| item.description.clone())
        .collect::<Vec<_>>();
    if failed_items.is_empty() {
        return None;
    }
    let status = match finish_reason {
        HarnessFinishReason::BudgetExhausted | HarnessFinishReason::RoundLimit => {
            VerificationNoticeStatus::PausedForRecovery
        }
        _ => VerificationNoticeStatus::AnswerWithCaveat,
    };
    let message = match status {
        VerificationNoticeStatus::AnswerWithCaveat => {
            format!("未验证项：{}", failed_items.join("；"))
        }
        VerificationNoticeStatus::PausedForRecovery => {
            format!("任务已暂停，未验证项：{}", failed_items.join("；"))
        }
    };
    Some(VerificationNotice {
        status,
        message,
        failed_items,
    })
}

/// Append a short verification notice to a final answer without adding UI burden.
pub fn append_verification_notice(content: &str, notice: Option<&VerificationNotice>) -> String {
    let Some(notice) = notice else {
        return content.to_string();
    };
    if content.contains("未验证项：") {
        return content.to_string();
    }
    let trimmed = content.trim_end();
    if trimmed.is_empty() {
        return notice.message.clone();
    }
    format!("{trimmed}\n\n{}", notice.message)
}

/// Persist the current deliberation state and latest verification summary.
pub fn save_deliberation_state(
    db: &Database,
    state: &DeliberationState,
    summary: &VerificationSummary,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let plan = serde_json::to_string(&state.plan_outline)?;
    let assumptions = serde_json::to_string(&state.assumptions)?;
    let open_questions = serde_json::to_string(&state.open_questions)?;
    let gaps = serde_json::to_string(&state.evidence_gaps)?;
    let verification = serde_json::to_string(summary)?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO deliberation_states
             (request_id, session_id, current_goal, plan_outline_json, assumptions_json,
              open_questions_json, evidence_gaps_json, verification_json, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
             ON CONFLICT(request_id) DO UPDATE SET
                session_id = excluded.session_id,
                current_goal = excluded.current_goal,
                plan_outline_json = excluded.plan_outline_json,
                assumptions_json = excluded.assumptions_json,
                open_questions_json = excluded.open_questions_json,
                evidence_gaps_json = excluded.evidence_gaps_json,
                verification_json = excluded.verification_json,
                status = excluded.status,
                updated_at = excluded.updated_at",
            rusqlite::params![
                state.request_id,
                state.session_id,
                state.current_goal,
                plan,
                assumptions,
                open_questions,
                gaps,
                verification,
                if summary.passed { "verified" } else { "needs_attention" },
                now,
            ],
        )?;
        Ok(())
    })
}

/// Load the latest deliberation and verification state for a request.
pub fn load_deliberation_state(
    db: &Database,
    request_id: &str,
) -> AppResult<Option<(DeliberationState, VerificationSummary)>> {
    db.with_read_conn(|conn| {
        let result = conn.query_row(
            "SELECT request_id, session_id, current_goal, plan_outline_json,
                    assumptions_json, open_questions_json, evidence_gaps_json,
                    verification_json, status
             FROM deliberation_states
             WHERE request_id = ?1",
            [request_id],
            |row| {
                let plan_json: String = row.get(3)?;
                let assumptions_json: String = row.get(4)?;
                let open_questions_json: String = row.get(5)?;
                let gaps_json: String = row.get(6)?;
                let verification_json: String = row.get(7)?;
                let summary = serde_json::from_str::<VerificationSummary>(&verification_json)
                    .unwrap_or(VerificationSummary {
                        passed: false,
                        items: Vec::new(),
                    });
                Ok((
                    DeliberationState {
                        request_id: row.get(0)?,
                        session_id: row.get(1)?,
                        current_goal: row.get(2)?,
                        plan_outline: serde_json::from_str(&plan_json).unwrap_or_default(),
                        assumptions: serde_json::from_str(&assumptions_json).unwrap_or_default(),
                        open_questions: serde_json::from_str(&open_questions_json)
                            .unwrap_or_default(),
                        evidence_gaps: serde_json::from_str(&gaps_json).unwrap_or_default(),
                        verification_items: summary.items.clone(),
                        status: row.get(8)?,
                    },
                    summary,
                ))
            },
        );
        match result {
            Ok(state) => Ok(Some(state)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    })
}

fn has_citation(answer: &str, evidence_packets: &[ContextPacket]) -> bool {
    !evidence_packets.is_empty()
        && evidence_packets.iter().any(|packet| {
            !packet.citation_label.is_empty() && answer.contains(&packet.citation_label)
        })
}

fn bounded(text: &str) -> String {
    let trimmed = text.trim();
    let chars: String = trimmed.chars().take(GOAL_LIMIT).collect();
    if trimmed.chars().count() > GOAL_LIMIT {
        format!("{chars}...")
    } else if chars.is_empty() {
        "未记录目标".to_string()
    } else {
        chars
    }
}
