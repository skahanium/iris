//! Research state for evidence-driven industry analysis.
//!
//! This state summarizes sources, credibility, freshness, conflicts, gaps, and
//! conclusion boundaries. It stores evidence metadata, not raw notes or pages.

use serde::{Deserialize, Serialize};

use crate::ai_runtime::{ContextPacket, SourceType, TrustLevel};
use crate::error::AppResult;
use crate::storage::db::Database;

const TEXT_LIMIT: usize = 260;

/// Inputs used to derive a research state snapshot.
#[derive(Debug, Clone)]
pub struct ResearchStateInput {
    pub request_id: String,
    pub topic: String,
    pub questions: Vec<String>,
    pub evidence: Vec<ContextPacket>,
    pub global_gaps: Vec<String>,
    pub has_contradictions: bool,
    pub summary: String,
}

/// Normalized evidence item for research state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceItem {
    pub evidence_id: String,
    pub citation_label: String,
    pub source_type: String,
    pub title: String,
    pub credibility: String,
    pub freshness: String,
    pub score: f64,
}

/// A bounded preliminary conclusion and its evidence boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConclusionBoundary {
    pub statement: String,
    pub evidence_item_ids: Vec<String>,
    pub boundary: String,
    pub inference: bool,
}

/// Durable research state for industry analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResearchState {
    pub request_id: String,
    pub research_question: String,
    pub sub_questions: Vec<String>,
    pub sources: Vec<EvidenceItem>,
    pub credibility_summary: String,
    pub freshness_summary: String,
    pub conflicts: Vec<String>,
    pub counter_arguments: Vec<String>,
    pub evidence_gaps: Vec<String>,
    pub preliminary_conclusions: Vec<ConclusionBoundary>,
}

impl ResearchState {
    /// Build research state from workflow outputs.
    pub fn from_input(input: ResearchStateInput) -> Self {
        let sources = input
            .evidence
            .iter()
            .map(evidence_item_from_packet)
            .collect::<Vec<_>>();
        let high_credibility = sources
            .iter()
            .filter(|source| source.credibility == "high")
            .count();
        let needs_freshness_check = sources
            .iter()
            .filter(|source| source.freshness == "needs_check")
            .count();
        let evidence_ids = sources
            .iter()
            .map(|source| source.evidence_id.clone())
            .collect::<Vec<_>>();

        let conflicts = if input.has_contradictions {
            vec!["命题之间存在冲突证据，需要人工复核关键分歧".to_string()]
        } else {
            Vec::new()
        };
        let counter_arguments = if input.has_contradictions {
            vec!["保留反方解释：部分证据可能支持相反结论".to_string()]
        } else if input.global_gaps.is_empty() {
            Vec::new()
        } else {
            vec!["证据缺口可能削弱初步结论".to_string()]
        };

        Self {
            request_id: input.request_id,
            research_question: bounded(&input.topic),
            sub_questions: input.questions.into_iter().map(|q| bounded(&q)).collect(),
            credibility_summary: format!(
                "{} sources, {} high credibility",
                sources.len(),
                high_credibility
            ),
            freshness_summary: format!("{needs_freshness_check} sources need freshness check"),
            sources,
            conflicts,
            counter_arguments,
            evidence_gaps: input
                .global_gaps
                .into_iter()
                .map(|gap| bounded(&gap))
                .collect(),
            preliminary_conclusions: vec![ConclusionBoundary {
                statement: bounded(&input.summary),
                evidence_item_ids: evidence_ids.clone(),
                boundary: if evidence_ids.is_empty() {
                    "无直接证据，必须标为模型推断".to_string()
                } else {
                    "需验证商业化节奏、样本范围和证据新鲜度".to_string()
                },
                inference: evidence_ids.is_empty(),
            }],
        }
    }
}

/// Persist a research state snapshot.
pub fn save_research_state(db: &Database, state: &ResearchState) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO research_states
             (request_id, research_question, sub_questions_json, sources_json,
              credibility_summary, freshness_summary, conflicts_json,
              counter_arguments_json, evidence_gaps_json, preliminary_conclusions_json,
              created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
             ON CONFLICT(request_id) DO UPDATE SET
                research_question = excluded.research_question,
                sub_questions_json = excluded.sub_questions_json,
                sources_json = excluded.sources_json,
                credibility_summary = excluded.credibility_summary,
                freshness_summary = excluded.freshness_summary,
                conflicts_json = excluded.conflicts_json,
                counter_arguments_json = excluded.counter_arguments_json,
                evidence_gaps_json = excluded.evidence_gaps_json,
                preliminary_conclusions_json = excluded.preliminary_conclusions_json,
                updated_at = excluded.updated_at",
            rusqlite::params![
                state.request_id,
                state.research_question,
                serde_json::to_string(&state.sub_questions)?,
                serde_json::to_string(&state.sources)?,
                state.credibility_summary,
                state.freshness_summary,
                serde_json::to_string(&state.conflicts)?,
                serde_json::to_string(&state.counter_arguments)?,
                serde_json::to_string(&state.evidence_gaps)?,
                serde_json::to_string(&state.preliminary_conclusions)?,
                now,
            ],
        )?;
        Ok(())
    })
}

fn evidence_item_from_packet(packet: &ContextPacket) -> EvidenceItem {
    EvidenceItem {
        evidence_id: packet.id.clone(),
        citation_label: packet.citation_label.clone(),
        source_type: source_type_label(packet.source_type).to_string(),
        title: bounded(&packet.title),
        credibility: credibility_label(packet.trust_level).to_string(),
        freshness: freshness_label(packet).to_string(),
        score: packet.score,
    }
}

fn credibility_label(trust: TrustLevel) -> &'static str {
    match trust {
        TrustLevel::UserNote => "high",
        TrustLevel::DerivedCache => "medium",
        TrustLevel::ExternalWeb => "medium",
        TrustLevel::ModelGenerated => "low",
    }
}

fn freshness_label(packet: &ContextPacket) -> &'static str {
    if packet.stale || matches!(packet.source_type, SourceType::Web) {
        "needs_check"
    } else {
        "current"
    }
}

fn source_type_label(source_type: SourceType) -> &'static str {
    match source_type {
        SourceType::Note => "note",
        SourceType::Anchor => "anchor",
        SourceType::Regulation => "regulation",
        SourceType::Template => "template",
        SourceType::Session => "session",
        SourceType::Web => "web",
    }
}

fn bounded(text: &str) -> String {
    let trimmed = text.trim();
    let chars = trimmed.chars().take(TEXT_LIMIT).collect::<String>();
    if trimmed.chars().count() > TEXT_LIMIT {
        format!("{chars}...")
    } else {
        chars
    }
}
