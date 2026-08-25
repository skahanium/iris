//! Deterministic finalization gate for frozen current-fact Runs.
//!
//! A current external-fact Run may only complete when the model submits a
//! structured final answer backed by usable current-Run evidence. Source
//! reference syntax and ownership are validated exclusively by
//! [`crate::ai_runtime::provenance::ProvenancePolicy`]; this module must not
//! create a second identifier interpreter.

use crate::ai_runtime::provenance::ProvenancePolicy;
use crate::ai_runtime::run_contract::FreshFactDomain;
use crate::ai_runtime::run_contract::FreshFactPolicy;

/// Stable reason a current-fact submission cannot complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CurrentFactFinalizationError {
    /// The model route cannot expose the required structured finalization
    /// protocol, so the Run must fail closed instead of guessing.
    UnsupportedProtocol,
    /// The current Run has no usable Web or external-tool evidence.
    InsufficientEvidence,
}

/// Validate the frozen current-fact policy against the current Run's unified
/// provenance allow-list.
///
/// This is a deterministic production gate, not a model-quality judge. It
/// enforces only current-fact policy availability and evidence sufficiency.
/// The provenance validator separately owns reference syntax, Run ownership,
/// and per-block source coverage.
pub(crate) fn validate_current_fact_policy(
    policy: &FreshFactPolicy,
    provenance: &ProvenancePolicy,
) -> Result<(), CurrentFactFinalizationError> {
    if policy.schema_version != 1 {
        return Err(CurrentFactFinalizationError::UnsupportedProtocol);
    }
    if !is_current_fact_domain(policy.domain) {
        return Err(CurrentFactFinalizationError::UnsupportedProtocol);
    }

    if provenance.current_run_web_evidence_ids.is_empty()
        && provenance.current_run_external_evidence_ids.is_empty()
    {
        return Err(CurrentFactFinalizationError::InsufficientEvidence);
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    use crate::ai_runtime::run_contract::LocationRequirement;

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

    fn provenance(web: &[i64], external: &[i64]) -> ProvenancePolicy {
        ProvenancePolicy {
            current_user_available: true,
            conversation_history_available: false,
            runtime_fact_available: false,
            authorized_material_count: 0,
            current_run_local_evidence_ids: BTreeSet::new(),
            current_run_web_evidence_ids: web.iter().copied().collect(),
            current_run_external_evidence_ids: external.iter().copied().collect(),
            strict_current_evidence: true,
        }
    }

    #[test]
    fn current_fact_policy_accepts_run_local_web_evidence() {
        let policy = entertainment_policy();
        let provenance = provenance(&[1], &[]);

        validate_current_fact_policy(&policy, &provenance)
            .expect("the current-fact gate consumes the unified Run-local evidence policy");
    }

    #[test]
    fn strict_current_fact_requires_current_run_web_or_external_evidence() {
        let policy = entertainment_policy();
        let provenance = provenance(&[], &[]);

        let error = validate_current_fact_policy(&policy, &provenance)
            .expect_err("current facts cannot complete without current evidence");
        assert_eq!(error, CurrentFactFinalizationError::InsufficientEvidence);
    }

    #[test]
    fn structured_external_evidence_is_sufficient_for_current_fact_policy() {
        let policy = entertainment_policy();
        let provenance = provenance(&[], &[1001]);

        validate_current_fact_policy(&policy, &provenance)
            .expect("a normalized current-Run provider record is sufficient evidence");
    }

    #[test]
    fn unsupported_finalization_protocol_never_falls_back_to_guessing() {
        let mut policy = entertainment_policy();
        policy.schema_version = 99;
        let provenance = provenance(&[1], &[]);

        let error = validate_current_fact_policy(&policy, &provenance)
            .expect_err("unsupported protocol must fail closed");
        assert_eq!(error, CurrentFactFinalizationError::UnsupportedProtocol);
    }
}
