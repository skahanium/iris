//! Deterministic finalization gate for frozen current-fact Runs.
//!
//! A current external-fact Run may only complete when the model submits a
//! structured final answer whose source references can be resolved to this
//! Run's registered evidence. This module deliberately does not call an LLM
//! judge and never downgrades a missing protocol/evidence failure to a
//! guessed free-text answer.

use crate::ai_runtime::agent_evidence_repository::RegisteredEvidence;
use crate::ai_runtime::final_answer_submission::FinalAnswerSubmission;
use crate::ai_runtime::run_contract::FreshFactDomain;
use crate::ai_runtime::run_contract::FreshFactPolicy;

/// Stable reason a current-fact submission cannot complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CurrentFactFinalizationError {
    /// The model route cannot expose the required structured finalization
    /// protocol, so the Run must fail closed instead of guessing.
    UnsupportedProtocol,
    /// The available evidence cannot support the submitted current-fact
    /// claims after one bounded structured repair.
    InsufficientEvidence,
    /// The submission claims a source or entity that the current Run's
    /// evidence does not support.
    UnsupportedClaim,
}

/// Validate a current-fact final submission against the frozen policy and the
/// current Run's registered evidence.
///
/// This is a deterministic production gate, not a model-quality judge. It
/// enforces the minimum contract: structured protocol availability, resolvable
/// Run-local sources, and no source-group-only completion for strict current
/// facts.
pub(crate) fn validate_current_fact_submission(
    policy: &FreshFactPolicy,
    submission: &FinalAnswerSubmission,
    evidence: &[RegisteredEvidence],
) -> Result<(), CurrentFactFinalizationError> {
    if policy.schema_version != 1 {
        return Err(CurrentFactFinalizationError::UnsupportedProtocol);
    }
    if !is_current_fact_domain(policy.domain) {
        return Err(CurrentFactFinalizationError::UnsupportedProtocol);
    }

    if evidence.is_empty() {
        return Err(CurrentFactFinalizationError::InsufficientEvidence);
    }

    let evidence_by_reference = evidence
        .iter()
        .map(|registered| {
            (
                registered.reference.evidence_id.as_str(),
                registered.reference.display_label.as_str(),
            )
        })
        .collect::<Vec<_>>();

    let mut has_fallback_only = true;
    for registered in evidence {
        let reference = &registered.reference;
        if !is_source_group_fallback_evidence(reference) {
            has_fallback_only = false;
        }
    }

    if has_fallback_only {
        return Err(CurrentFactFinalizationError::UnsupportedClaim);
    }

    for block in &submission.blocks {
        for source in &block.sources {
            let resolved = evidence_by_reference
                .iter()
                .any(|(id, label)| source_matches_registered_evidence(source, id, label));
            if !resolved {
                return Err(CurrentFactFinalizationError::UnsupportedClaim);
            }
        }
        if block.sources.is_empty()
            && !crate::ai_runtime::final_answer_submission::is_source_free_structural_block(
                &block.markdown,
            )
        {
            return Err(CurrentFactFinalizationError::UnsupportedClaim);
        }
    }

    Ok(())
}

fn source_matches_registered_evidence(source: &str, id: &str, label: &str) -> bool {
    source == id
        || source == label
        || source
            .strip_prefix(['W', 'E', 'L'])
            .is_some_and(|numeric_id| numeric_id == id)
}

fn is_current_fact_domain(domain: FreshFactDomain) -> bool {
    matches!(
        domain,
        FreshFactDomain::Weather
            | FreshFactDomain::News
            | FreshFactDomain::Finance
            | FreshFactDomain::Entertainment
            | FreshFactDomain::Sports
            | FreshFactDomain::GenericWeb
    )
}

fn is_source_group_fallback_evidence(
    reference: &crate::ai_runtime::run_contract::EvidenceRef,
) -> bool {
    let haystack = format!(
        "{} {} {}",
        reference.evidence_id,
        reference.display_label,
        reference.title.as_deref().unwrap_or("")
    )
    .to_lowercase();
    [
        "source_group",
        "sourcegroup",
        "source-group",
        "fallback",
        "uncalibrated_route",
    ]
    .iter()
    .any(|marker| haystack.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::agent_evidence_repository::RegisteredEvidence;
    use crate::ai_runtime::run_contract::{EvidenceRef, EvidenceSourceKind, LocationRequirement};

    fn entertainment_policy() -> FreshFactPolicy {
        FreshFactPolicy {
            schema_version: 1,
            domain: FreshFactDomain::Entertainment,
            operation: None,
            window_start: Some("2026-07-19T08:00:00Z".to_string()),
            window_end: Some("2026-10-17T08:00:00Z".to_string()),
            location_requirement: LocationRequirement::City,
        }
    }

    fn evidence(id: &str, title: &str) -> RegisteredEvidence {
        RegisteredEvidence {
            evidence_id: id.parse().expect("numeric fixture id"),
            reference: EvidenceRef {
                evidence_id: id.to_string(),
                source_kind: EvidenceSourceKind::Web,
                title: Some(title.to_string()),
                display_label: format!("W{id}"),
                stale: false,
            },
        }
    }

    fn submission(markdown: &str, sources: &[&str]) -> FinalAnswerSubmission {
        FinalAnswerSubmission {
            blocks: vec![
                crate::ai_runtime::final_answer_submission::FinalAnswerBlock {
                    markdown: markdown.to_string(),
                    sources: sources.iter().map(|value| (*value).to_string()).collect(),
                },
            ],
        }
    }

    #[test]
    fn current_fact_finalization_accepts_provenance_prefixed_evidence_ids() {
        let policy = entertainment_policy();
        let evidence = [evidence("3", "上海 · 8月20日 · 电影A")];
        let structured = submission("电影A 8月20日上海上映", &["W3"]);

        validate_current_fact_submission(&policy, &structured, &evidence)
            .expect("the finalizer must accept the provenance reference used downstream");
    }

    #[test]
    fn strict_current_fact_rejects_unsupported_free_text() {
        let policy = entertainment_policy();
        let evidence = [evidence("1", "上海 · 8月20日 · 电影A")];
        let free_text = submission("最近值得看的电影是电影A。", &[]);

        let error = validate_current_fact_submission(&policy, &free_text, &evidence)
            .expect_err("free text cannot complete a strict current fact");
        assert_eq!(error, CurrentFactFinalizationError::UnsupportedClaim);
    }

    #[test]
    fn source_group_fallback_cannot_complete_strict_current_fact() {
        let policy = entertainment_policy();
        let fallback = RegisteredEvidence {
            evidence_id: 9,
            reference: EvidenceRef {
                evidence_id: "source_group".to_string(),
                source_kind: EvidenceSourceKind::Web,
                title: Some("SourceGroupFallback".to_string()),
                display_label: "source_group".to_string(),
                stale: false,
            },
        };
        let structured = submission("电影A 8月20日上海上映", &["source_group"]);

        let error = validate_current_fact_submission(&policy, &structured, &[fallback])
            .expect_err("source-group fallback cannot complete a strict current fact");
        assert_eq!(error, CurrentFactFinalizationError::UnsupportedClaim);
    }

    #[test]
    fn unsupported_finalization_protocol_never_falls_back_to_guessing() {
        let mut policy = entertainment_policy();
        policy.schema_version = 99;
        let evidence = [evidence("1", "上海 · 8月20日 · 电影A")];
        let structured = submission("电影A 8月20日上海上映", &["1"]);

        let error = validate_current_fact_submission(&policy, &structured, &evidence)
            .expect_err("unsupported protocol must fail closed");
        assert_eq!(error, CurrentFactFinalizationError::UnsupportedProtocol);
    }
}
