//! Writing collaboration state for important documents.
//!
//! The state captures goals, style constraints, evidence links, draft version,
//! and patch-level revision rationale. It intentionally excludes raw document
//! bodies and selected text.

use serde::{Deserialize, Serialize};

use crate::ai_runtime::{ContextPacket, PatchProposal};
use crate::error::AppResult;
use crate::storage::db::Database;

const TEXT_LIMIT: usize = 240;

/// Inputs used to derive a writing state snapshot.
#[derive(Debug, Clone)]
pub struct WritingStateInput {
    pub request_id: String,
    pub target_path: String,
    pub base_content_hash: String,
    pub writing_goal: String,
    pub intent: String,
    pub evidence: Vec<ContextPacket>,
    pub patches: Vec<PatchProposal>,
}

/// One patch-level revision rationale.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WritingRevisionRecord {
    pub patch_id: String,
    pub scope: String,
    pub reason: String,
    pub risk: String,
    pub rollback: String,
    pub evidence_packet_ids: Vec<String>,
}

/// Durable collaboration state for a writing task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WritingState {
    pub request_id: String,
    pub target_path: String,
    pub document_goal: String,
    pub audience: String,
    pub genre: String,
    pub structure_outline: Vec<String>,
    pub key_arguments: Vec<String>,
    pub material_packet_ids: Vec<String>,
    pub citation_labels: Vec<String>,
    pub style_constraints: Vec<String>,
    pub revision_records: Vec<WritingRevisionRecord>,
    pub draft_version_hash: String,
}

impl WritingState {
    /// Build state from existing workflow inputs and generated patch proposals.
    pub fn from_input(input: WritingStateInput) -> Self {
        let material_packet_ids = input
            .evidence
            .iter()
            .map(|packet| packet.id.clone())
            .collect::<Vec<_>>();
        let citation_labels = input
            .evidence
            .iter()
            .filter(|packet| !packet.citation_label.is_empty())
            .map(|packet| packet.citation_label.clone())
            .collect::<Vec<_>>();
        let key_arguments = input
            .evidence
            .iter()
            .take(5)
            .map(|packet| bounded(&packet.title))
            .collect::<Vec<_>>();
        let revision_records = input
            .patches
            .iter()
            .map(|patch| WritingRevisionRecord {
                patch_id: patch.id.clone(),
                scope: format!("{}..{}", patch.range.start, patch.range.end),
                reason: bounded(&format!("{}: {}", input.intent, input.writing_goal)),
                risk: risk_label(patch.risk_level).to_string(),
                rollback: format!("恢复到 base_content_hash={}", input.base_content_hash),
                evidence_packet_ids: patch.evidence_packet_ids.clone(),
            })
            .collect::<Vec<_>>();

        Self {
            request_id: input.request_id,
            target_path: input.target_path,
            document_goal: bounded(&input.writing_goal),
            audience: extract_labeled(&input.writing_goal, &["受众:", "受众：", "audience:"]),
            genre: extract_labeled(&input.writing_goal, &["体裁:", "体裁：", "genre:"]),
            structure_outline: infer_outline(&input.writing_goal),
            key_arguments,
            material_packet_ids,
            citation_labels,
            style_constraints: infer_style_constraints(&input.writing_goal),
            revision_records,
            draft_version_hash: input.base_content_hash,
        }
    }
}

/// Persist a writing state snapshot.
pub fn save_writing_state(db: &Database, state: &WritingState) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO writing_states
             (request_id, target_path, draft_version_hash, document_goal, audience, genre,
              structure_outline_json, key_arguments_json, material_packet_ids_json,
              citation_labels_json, style_constraints_json, revision_records_json,
              created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)
             ON CONFLICT(request_id) DO UPDATE SET
                target_path = excluded.target_path,
                draft_version_hash = excluded.draft_version_hash,
                document_goal = excluded.document_goal,
                audience = excluded.audience,
                genre = excluded.genre,
                structure_outline_json = excluded.structure_outline_json,
                key_arguments_json = excluded.key_arguments_json,
                material_packet_ids_json = excluded.material_packet_ids_json,
                citation_labels_json = excluded.citation_labels_json,
                style_constraints_json = excluded.style_constraints_json,
                revision_records_json = excluded.revision_records_json,
                updated_at = excluded.updated_at",
            rusqlite::params![
                state.request_id,
                state.target_path,
                state.draft_version_hash,
                state.document_goal,
                state.audience,
                state.genre,
                serde_json::to_string(&state.structure_outline)?,
                serde_json::to_string(&state.key_arguments)?,
                serde_json::to_string(&state.material_packet_ids)?,
                serde_json::to_string(&state.citation_labels)?,
                serde_json::to_string(&state.style_constraints)?,
                serde_json::to_string(&state.revision_records)?,
                now,
            ],
        )?;
        Ok(())
    })
}

fn infer_outline(goal: &str) -> Vec<String> {
    if goal.contains("提纲")
        || goal.contains("结构")
        || goal.to_ascii_lowercase().contains("outline")
    {
        vec!["结构调整".to_string()]
    } else {
        Vec::new()
    }
}

fn infer_style_constraints(goal: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(style) = extract_labeled_opt(goal, &["风格:", "风格：", "style:"]) {
        out.push(style);
    }
    if goal.contains("证据") {
        out.push("证据驱动".to_string());
    }
    if goal.contains("克制") {
        out.push("克制表达".to_string());
    }
    out.sort();
    out.dedup();
    out
}

fn extract_labeled(text: &str, markers: &[&str]) -> String {
    extract_labeled_opt(text, markers).unwrap_or_default()
}

fn extract_labeled_opt(text: &str, markers: &[&str]) -> Option<String> {
    for marker in markers {
        if let Some(start) = text.find(marker) {
            let rest = text[start + marker.len()..].trim();
            let end = rest
                .char_indices()
                .find_map(|(idx, ch)| matches!(ch, '，' | ',' | '。' | ';' | '；').then_some(idx))
                .unwrap_or(rest.len());
            let value = bounded(&rest[..end]);
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn risk_label(risk: crate::ai_runtime::RiskLevel) -> &'static str {
    match risk {
        crate::ai_runtime::RiskLevel::Low => "low",
        crate::ai_runtime::RiskLevel::Medium => "medium",
        crate::ai_runtime::RiskLevel::High => "high",
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
