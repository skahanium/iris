//! Fixture builders shared by the evaluation test modules.
//!
//! Extracted from `agent_capacity_eval_tests.rs` when that file exceeded its
//! pinned size budget. Both builders are used by more than one test module, so
//! they live here instead of being duplicated.

use super::agent_capacity_eval::{
    AnswerObservation, CaseManifest, CitationObservation, FactSupportObservation, ObservedSource,
    WebAnswerContamination,
};

pub(crate) fn manifest_fixture() -> CaseManifest {
    CaseManifest::parse(include_str!(
        "../../../docs/eval/fixtures/agent-answer-v1.json"
    ))
    .expect("versioned evaluation fixture must parse")
}

pub(crate) fn observation_for(case: &CaseManifest) -> AnswerObservation {
    AnswerObservation {
        case_id: case.id.clone(),
        sources: case
            .required_sources
            .iter()
            .map(|source| ObservedSource {
                id: source.id.clone(),
                kind: source.kind,
                authorization_scope_id: None,
            })
            .collect(),
        fact_supports: case
            .required_facts
            .iter()
            .filter_map(|fact| {
                fact.allowed_sources
                    .first()
                    .map(|source_id| FactSupportObservation {
                        fact_id: fact.id.clone(),
                        source_ids: vec![source_id.clone()],
                    })
            })
            .collect(),
        contradicted_fact_ids: Vec::new(),
        citations: case
            .required_facts
            .iter()
            .filter_map(|fact| {
                fact.allowed_sources
                    .first()
                    .map(|source_id| CitationObservation {
                        fact_id: fact.id.clone(),
                        source_id: source_id.clone(),
                    })
            })
            .collect(),
        tool_calls: Vec::new(),
        disclosures: case.disclosure_constraints.clone(),
        degraded: false,
        clarification_requested: false,
        web_answer_contamination: WebAnswerContamination::ConfirmedAbsent,
        safety_violations: Vec::new(),
    }
}
