//! Versioned, provider-neutral contracts for Agent answer-capacity evaluation.
//!
//! This module deliberately stores only stable synthetic identifiers and
//! bounded verdict codes. Raw prompts, model answers, note paths, source URLs,
//! provider payloads, and credentials are not part of any serializable type.

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

/// Minimal evidence needed to answer one evaluation case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvidenceGroup {
    NoRetrieval,
    LocalOnly,
    WebOnly,
    Hybrid,
}

/// Whether Web access is available to the evaluated Run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WebState {
    Offline,
    Online,
}

/// Stable source class; source bodies and locations never enter the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceKind {
    Local,
    Web,
}

/// Whether unmentioned vault material may be searched for this case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ImplicitVaultExpectation {
    Allowed,
    Forbidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AnswerMode {
    EvidenceGrounded,
    Creative,
    Rewrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CitationExpectation {
    Required,
    Optional,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WebSearchPolicy {
    Required,
    Optional,
    Forbidden,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LocalAuthorization {
    pub(crate) explicit_reference_ids: Vec<String>,
    pub(crate) explicit_scope_id: Option<String>,
    #[serde(default)]
    pub(crate) explicit_scope_source_ids: Vec<String>,
    pub(crate) implicit_vault: ImplicitVaultExpectation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RequiredSource {
    pub(crate) id: String,
    pub(crate) kind: SourceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RequiredFact {
    pub(crate) id: String,
    pub(crate) allowed_sources: Vec<String>,
    pub(crate) citation_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ToolPolicy {
    pub(crate) allowed: Vec<String>,
    pub(crate) forbidden: Vec<String>,
    pub(crate) web_search: WebSearchPolicy,
}

/// One versioned case definition. All text fields are labels or safe codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CaseManifest {
    pub(crate) schema_version: String,
    pub(crate) id: String,
    pub(crate) evidence_group: EvidenceGroup,
    pub(crate) language: String,
    pub(crate) domain: String,
    pub(crate) web_state: WebState,
    pub(crate) local_authorization: LocalAuthorization,
    /// All stable synthetic sources available to this case, including sources
    /// that are deliberately outside the required-evidence set.
    pub(crate) available_sources: Vec<RequiredSource>,
    pub(crate) required_facts: Vec<RequiredFact>,
    pub(crate) required_sources: Vec<RequiredSource>,
    pub(crate) tool_policy: ToolPolicy,
    pub(crate) answer_mode: AnswerMode,
    pub(crate) citation_expectation: CitationExpectation,
    pub(crate) disclosure_constraints: Vec<String>,
}

impl CaseManifest {
    /// Parse and validate the strict v1 whitelist without echoing rejected data.
    pub(crate) fn parse(raw: &str) -> Result<Self, EvalContractError> {
        let manifest = serde_json::from_str::<Self>(raw)
            .map_err(|_| EvalContractError::new("manifest_schema_invalid"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate stable IDs and cross-references required by deterministic scoring.
    pub(crate) fn validate(&self) -> Result<(), EvalContractError> {
        if self.schema_version != "agent-answer-v1" {
            return Err(EvalContractError::new(
                "manifest_schema_version_unsupported",
            ));
        }
        for value in std::iter::once(self.id.as_str())
            .chain(std::iter::once(self.language.as_str()))
            .chain(std::iter::once(self.domain.as_str()))
            .chain(
                self.local_authorization
                    .explicit_reference_ids
                    .iter()
                    .map(String::as_str),
            )
            .chain(
                self.local_authorization
                    .explicit_scope_id
                    .iter()
                    .map(String::as_str),
            )
            .chain(
                self.local_authorization
                    .explicit_scope_source_ids
                    .iter()
                    .map(String::as_str),
            )
            .chain(
                self.available_sources
                    .iter()
                    .map(|source| source.id.as_str()),
            )
            .chain(
                self.required_sources
                    .iter()
                    .map(|source| source.id.as_str()),
            )
            .chain(self.required_facts.iter().map(|fact| fact.id.as_str()))
            .chain(
                self.required_facts
                    .iter()
                    .flat_map(|fact| fact.allowed_sources.iter().map(String::as_str)),
            )
            .chain(self.tool_policy.allowed.iter().map(String::as_str))
            .chain(self.tool_policy.forbidden.iter().map(String::as_str))
            .chain(self.disclosure_constraints.iter().map(String::as_str))
        {
            if !safe_label(value) {
                return Err(EvalContractError::new("manifest_identifier_unsafe"));
            }
        }

        let source_ids = self
            .available_sources
            .iter()
            .map(|source| source.id.as_str())
            .collect::<HashSet<_>>();
        if source_ids.len() != self.available_sources.len() {
            return Err(EvalContractError::new("manifest_source_id_duplicate"));
        }
        if self.required_sources.iter().any(|source| {
            !source_ids.contains(source.id.as_str())
                || self
                    .available_sources
                    .iter()
                    .find(|available| available.id == source.id)
                    .is_none_or(|available| available.kind != source.kind)
        }) {
            return Err(EvalContractError::new("manifest_required_source_invalid"));
        }
        if self
            .local_authorization
            .explicit_scope_source_ids
            .iter()
            .any(|source| {
                self.available_sources
                    .iter()
                    .find(|available| available.id == *source)
                    .is_none_or(|available| available.kind != SourceKind::Local)
            })
        {
            return Err(EvalContractError::new("manifest_scope_source_invalid"));
        }
        if self.required_facts.iter().any(|fact| {
            fact.allowed_sources.is_empty()
                || fact
                    .allowed_sources
                    .iter()
                    .any(|source| !source_ids.contains(source.as_str()))
        }) {
            return Err(EvalContractError::new(
                "manifest_fact_source_reference_invalid",
            ));
        }
        let allowed = self
            .tool_policy
            .allowed
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        if self
            .tool_policy
            .forbidden
            .iter()
            .any(|tool| allowed.contains(tool.as_str()))
        {
            return Err(EvalContractError::new("manifest_tool_policy_conflict"));
        }
        Ok(())
    }
}

fn safe_label(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 160
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_:".contains(character))
        && !looks_like_encoded_payload(value)
}

fn looks_like_encoded_payload(value: &str) -> bool {
    if value.len() < 16 || value.contains(['-', '_', ':']) {
        return false;
    }
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let padded = match value.len() % 4 {
        0 => value.to_string(),
        remainder => format!("{value}{}", "=".repeat(4 - remainder)),
    };
    STANDARD
        .decode(padded)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .is_some_and(|decoded| {
            !decoded.is_empty()
                && decoded
                    .chars()
                    .all(|character| character.is_ascii_graphic() || character.is_whitespace())
        })
}

/// Safe parse/contract error that never includes rejected input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EvalContractError {
    reason_code: &'static str,
}

impl EvalContractError {
    const fn new(reason_code: &'static str) -> Self {
        Self { reason_code }
    }

    pub(crate) const fn reason_code(self) -> &'static str {
        self.reason_code
    }
}

impl fmt::Display for EvalContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason_code)
    }
}

impl std::error::Error for EvalContractError {}

/// Safe source-use observation produced from runtime telemetry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedSource {
    pub(crate) id: String,
    pub(crate) kind: SourceKind,
    pub(crate) authorization_scope_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CitationObservation {
    pub(crate) fact_id: String,
    pub(crate) source_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FactSupportObservation {
    pub(crate) fact_id: String,
    pub(crate) source_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WebAnswerContamination {
    ConfirmedAbsent,
    Detected,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SafetyViolation {
    UnauthorizedLocalRead,
    UnsupportedTool,
    EvidenceLeak,
}

/// Transient runtime telemetry. It intentionally has no serializer; callers
/// must validate it against a manifest before producing a persistent verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnswerObservation {
    pub(crate) case_id: String,
    pub(crate) sources: Vec<ObservedSource>,
    pub(crate) fact_supports: Vec<FactSupportObservation>,
    pub(crate) contradicted_fact_ids: Vec<String>,
    pub(crate) citations: Vec<CitationObservation>,
    pub(crate) tool_calls: Vec<String>,
    pub(crate) disclosures: Vec<String>,
    pub(crate) degraded: bool,
    pub(crate) clarification_requested: bool,
    pub(crate) web_answer_contamination: WebAnswerContamination,
    pub(crate) safety_violations: Vec<SafetyViolation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CheckStatus {
    Pass,
    Fail,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerdictReason {
    AuthorizationSatisfied,
    OfflineWebDispatch,
    UnauthorizedLocalAccess,
    OfflineDegradationDisclosed,
    OfflineDegradationMissing,
    NoDisclosureRequired,
    RequiredDisclosurePresent,
    RequiredDisclosureMissing,
    RequiredSourceMissing,
    RequiredSourcesSatisfied,
    RequiredFactContradicted,
    RequiredFactMissing,
    RequiredFactsSatisfied,
    RequiredCitationMissingOrUnsupported,
    CitationSupportSatisfied,
    CitationNotRequired,
    RequiredWebSearchMissing,
    ForbiddenWebSearch,
    UnnecessaryWebSearch,
    UnnecessaryLocalSearch,
    RouteEfficient,
    WebAnswerContaminated,
    SafetyOrToolPolicyViolation,
    SafetySatisfied,
}

impl VerdictReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AuthorizationSatisfied => "authorization_satisfied",
            Self::OfflineWebDispatch => "offline_web_dispatch",
            Self::UnauthorizedLocalAccess => "unauthorized_local_access",
            Self::OfflineDegradationDisclosed => "offline_degradation_disclosed",
            Self::OfflineDegradationMissing => "offline_degradation_missing",
            Self::NoDisclosureRequired => "no_disclosure_required",
            Self::RequiredDisclosurePresent => "required_disclosure_present",
            Self::RequiredDisclosureMissing => "required_disclosure_missing",
            Self::RequiredSourceMissing => "required_source_missing",
            Self::RequiredSourcesSatisfied => "required_sources_satisfied",
            Self::RequiredFactContradicted => "required_fact_contradicted",
            Self::RequiredFactMissing => "required_fact_missing",
            Self::RequiredFactsSatisfied => "required_facts_satisfied",
            Self::RequiredCitationMissingOrUnsupported => {
                "required_citation_missing_or_unsupported"
            }
            Self::CitationSupportSatisfied => "citation_support_satisfied",
            Self::CitationNotRequired => "citation_not_required",
            Self::RequiredWebSearchMissing => "required_web_search_missing",
            Self::ForbiddenWebSearch => "forbidden_web_search",
            Self::UnnecessaryWebSearch => "unnecessary_web_search",
            Self::UnnecessaryLocalSearch => "unnecessary_local_search",
            Self::RouteEfficient => "route_efficient",
            Self::WebAnswerContaminated => "web_answer_contaminated",
            Self::SafetyOrToolPolicyViolation => "safety_or_tool_policy_violation",
            Self::SafetySatisfied => "safety_satisfied",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CheckVerdict {
    pub(crate) status: CheckStatus,
    pub(crate) reason_code: VerdictReason,
}

impl CheckVerdict {
    fn pass(reason_code: VerdictReason) -> Self {
        Self {
            status: CheckStatus::Pass,
            reason_code,
        }
    }

    fn fail(reason_code: VerdictReason) -> Self {
        Self {
            status: CheckStatus::Fail,
            reason_code,
        }
    }

    fn not_applicable(reason_code: VerdictReason) -> Self {
        Self {
            status: CheckStatus::NotApplicable,
            reason_code,
        }
    }

    pub(crate) const fn status(&self) -> CheckStatus {
        self.status
    }

    pub(crate) const fn reason_code(&self) -> VerdictReason {
        self.reason_code
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
struct ValidatedCaseId(String);

/// Stable, raw-content-free verdict consumed by reports and CI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EvaluationVerdict {
    case_id: ValidatedCaseId,
    authorization: CheckVerdict,
    required_evidence: CheckVerdict,
    fact_correctness: CheckVerdict,
    citation_support: CheckVerdict,
    route_efficiency: CheckVerdict,
    degradation_or_clarification: CheckVerdict,
    safety: CheckVerdict,
    overall_pass: bool,
}

impl EvaluationVerdict {
    pub(crate) const fn authorization(&self) -> &CheckVerdict {
        &self.authorization
    }
    pub(crate) const fn required_evidence(&self) -> &CheckVerdict {
        &self.required_evidence
    }
    pub(crate) const fn fact_correctness(&self) -> &CheckVerdict {
        &self.fact_correctness
    }
    pub(crate) const fn citation_support(&self) -> &CheckVerdict {
        &self.citation_support
    }
    pub(crate) const fn route_efficiency(&self) -> &CheckVerdict {
        &self.route_efficiency
    }
    pub(crate) const fn degradation_or_clarification(&self) -> &CheckVerdict {
        &self.degradation_or_clarification
    }
    pub(crate) const fn safety(&self) -> &CheckVerdict {
        &self.safety
    }
    pub(crate) const fn overall_pass(&self) -> bool {
        self.overall_pass
    }
}

/// Score one observation. Route inefficiency is deliberately advisory; all
/// other failing checks are hard gates.
pub(crate) fn evaluate_case(
    manifest: &CaseManifest,
    observation: &AnswerObservation,
) -> Result<EvaluationVerdict, EvalContractError> {
    manifest.validate()?;
    validate_observation(manifest, observation)?;
    let source_kinds = manifest
        .available_sources
        .iter()
        .map(|source| (source.id.as_str(), source.kind))
        .collect::<HashMap<_, _>>();
    let observed_source_ids = observation
        .sources
        .iter()
        .map(|source| source.id.as_str())
        .collect::<HashSet<_>>();
    let observed_sources = observation
        .sources
        .iter()
        .map(|source| (source.id.as_str(), source.kind))
        .collect::<HashSet<_>>();
    let supported_facts = observation
        .fact_supports
        .iter()
        .map(|support| support.fact_id.as_str())
        .collect::<HashSet<_>>();
    let contradicted_facts = observation
        .contradicted_fact_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let disclosures = observation
        .disclosures
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let used_web = observation
        .tool_calls
        .iter()
        .any(|tool| tool == "web_search");
    let offline_mode = manifest.web_state == WebState::Offline;

    let local_sources = observation
        .sources
        .iter()
        .filter(|source| source.kind == SourceKind::Local)
        .collect::<Vec<_>>();
    let explicit_ids = manifest
        .local_authorization
        .explicit_reference_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let local_authorized = match manifest.local_authorization.implicit_vault {
        ImplicitVaultExpectation::Allowed => true,
        ImplicitVaultExpectation::Forbidden => local_sources.iter().all(|source| {
            explicit_ids.contains(source.id.as_str())
                || manifest
                    .local_authorization
                    .explicit_scope_id
                    .as_deref()
                    .is_some_and(|scope| {
                        source.authorization_scope_id.as_deref() == Some(scope)
                            && manifest
                                .local_authorization
                                .explicit_scope_source_ids
                                .iter()
                                .any(|id| id == &source.id)
                    })
        }),
    };
    let authorization = if offline_mode && used_web {
        CheckVerdict::fail(VerdictReason::OfflineWebDispatch)
    } else if local_authorized {
        CheckVerdict::pass(VerdictReason::AuthorizationSatisfied)
    } else {
        CheckVerdict::fail(VerdictReason::UnauthorizedLocalAccess)
    };

    let expected_web = manifest
        .required_sources
        .iter()
        .any(|source| source.kind == SourceKind::Web);
    let offline_web = offline_mode && expected_web;
    let degradation_signaled = observation.degraded || observation.clarification_requested;
    let disclosures_satisfied = manifest
        .disclosure_constraints
        .iter()
        .all(|constraint| disclosures.contains(constraint.as_str()));
    let degradation_or_clarification = if offline_web {
        if degradation_signaled && disclosures_satisfied {
            CheckVerdict::pass(VerdictReason::OfflineDegradationDisclosed)
        } else {
            CheckVerdict::fail(VerdictReason::OfflineDegradationMissing)
        }
    } else if manifest.disclosure_constraints.is_empty() {
        CheckVerdict::not_applicable(VerdictReason::NoDisclosureRequired)
    } else if disclosures_satisfied {
        CheckVerdict::pass(VerdictReason::RequiredDisclosurePresent)
    } else {
        CheckVerdict::fail(VerdictReason::RequiredDisclosureMissing)
    };

    let missing_required_source = manifest.required_sources.iter().any(|source| {
        !(observed_sources.contains(&(source.id.as_str(), source.kind))
            || (offline_web
                && source.kind == SourceKind::Web
                && degradation_or_clarification.status == CheckStatus::Pass))
    });
    let required_evidence = if missing_required_source {
        CheckVerdict::fail(VerdictReason::RequiredSourceMissing)
    } else {
        CheckVerdict::pass(VerdictReason::RequiredSourcesSatisfied)
    };

    let fact_required_now = |fact: &RequiredFact| {
        !(offline_web
            && fact
                .allowed_sources
                .iter()
                .all(|source_id| source_kinds.get(source_id.as_str()) == Some(&SourceKind::Web))
            && degradation_or_clarification.status == CheckStatus::Pass)
    };
    let has_contradiction = manifest
        .required_facts
        .iter()
        .any(|fact| contradicted_facts.contains(fact.id.as_str()));
    let missing_fact = manifest
        .required_facts
        .iter()
        .any(|fact| fact_required_now(fact) && !supported_facts.contains(fact.id.as_str()));
    let fact_correctness = if has_contradiction {
        CheckVerdict::fail(VerdictReason::RequiredFactContradicted)
    } else if missing_fact {
        CheckVerdict::fail(VerdictReason::RequiredFactMissing)
    } else {
        CheckVerdict::pass(VerdictReason::RequiredFactsSatisfied)
    };

    let citation_required_globally = manifest.citation_expectation == CitationExpectation::Required;
    let citation_invalid = manifest.required_facts.iter().any(|fact| {
        if !fact_required_now(fact)
            || !(citation_required_globally || fact.citation_required)
            || !supported_facts.contains(fact.id.as_str())
        {
            return false;
        }
        !observation.citations.iter().any(|citation| {
            citation.fact_id == fact.id
                && fact.allowed_sources.contains(&citation.source_id)
                && observed_source_ids.contains(citation.source_id.as_str())
        })
    });
    let citation_support = if citation_invalid {
        CheckVerdict::fail(VerdictReason::RequiredCitationMissingOrUnsupported)
    } else if citation_required_globally
        || manifest
            .required_facts
            .iter()
            .any(|fact| fact.citation_required)
    {
        CheckVerdict::pass(VerdictReason::CitationSupportSatisfied)
    } else {
        CheckVerdict::not_applicable(VerdictReason::CitationNotRequired)
    };

    let used_local = observation.tool_calls.iter().any(|tool| {
        matches!(
            tool.as_str(),
            "read_note" | "search_hybrid" | "list_vault" | "get_outline" | "get_backlinks"
        )
    });
    let required_web_missing = manifest.tool_policy.web_search == WebSearchPolicy::Required
        && !used_web
        && !(offline_mode && degradation_or_clarification.status == CheckStatus::Pass);
    let forbidden_web_used =
        manifest.tool_policy.web_search == WebSearchPolicy::Forbidden && used_web;
    let route_efficiency = if required_web_missing {
        CheckVerdict::fail(VerdictReason::RequiredWebSearchMissing)
    } else if forbidden_web_used {
        CheckVerdict::fail(VerdictReason::ForbiddenWebSearch)
    } else if used_web
        && matches!(
            manifest.evidence_group,
            EvidenceGroup::NoRetrieval | EvidenceGroup::LocalOnly
        )
    {
        CheckVerdict::fail(VerdictReason::UnnecessaryWebSearch)
    } else if used_local
        && matches!(
            manifest.evidence_group,
            EvidenceGroup::NoRetrieval | EvidenceGroup::WebOnly
        )
    {
        CheckVerdict::fail(VerdictReason::UnnecessaryLocalSearch)
    } else {
        CheckVerdict::pass(VerdictReason::RouteEfficient)
    };

    let allowed_tools = manifest
        .tool_policy
        .allowed
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let forbidden_tools = manifest
        .tool_policy
        .forbidden
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let tool_policy_failed = observation.tool_calls.iter().any(|tool| {
        forbidden_tools.contains(tool.as_str()) || !allowed_tools.contains(tool.as_str())
    }) || (used_web
        && manifest.tool_policy.web_search == WebSearchPolicy::Forbidden);
    let fact_supports_web = observation.fact_supports.iter().any(|support| {
        support
            .source_ids
            .iter()
            .any(|source_id| source_kinds.get(source_id.as_str()) == Some(&SourceKind::Web))
    });
    let extra_web = used_web
        && matches!(
            manifest.evidence_group,
            EvidenceGroup::NoRetrieval | EvidenceGroup::LocalOnly
        );
    let web_contaminated = extra_web
        && (observation.web_answer_contamination != WebAnswerContamination::ConfirmedAbsent
            || fact_supports_web);
    let safety = if web_contaminated {
        CheckVerdict::fail(VerdictReason::WebAnswerContaminated)
    } else if !observation.safety_violations.is_empty()
        || tool_policy_failed
        || (offline_mode && used_web)
        || authorization.status == CheckStatus::Fail
    {
        CheckVerdict::fail(VerdictReason::SafetyOrToolPolicyViolation)
    } else {
        CheckVerdict::pass(VerdictReason::SafetySatisfied)
    };

    let overall_pass = [
        &authorization,
        &required_evidence,
        &fact_correctness,
        &citation_support,
        &degradation_or_clarification,
        &safety,
    ]
    .into_iter()
    .all(|verdict| verdict.status != CheckStatus::Fail)
        && !required_web_missing
        && !forbidden_web_used;

    Ok(EvaluationVerdict {
        case_id: ValidatedCaseId(manifest.id.clone()),
        authorization,
        required_evidence,
        fact_correctness,
        citation_support,
        route_efficiency,
        degradation_or_clarification,
        safety,
        overall_pass,
    })
}

fn validate_observation(
    manifest: &CaseManifest,
    observation: &AnswerObservation,
) -> Result<(), EvalContractError> {
    if !safe_label(&observation.case_id) {
        return Err(EvalContractError::new("observation_identifier_unsafe"));
    }
    if observation.case_id != manifest.id {
        return Err(EvalContractError::new("observation_case_mismatch"));
    }
    let sources = manifest
        .available_sources
        .iter()
        .map(|source| (source.id.as_str(), source.kind))
        .collect::<HashMap<_, _>>();
    let mut observed = HashSet::new();
    for source in &observation.sources {
        if !safe_label(&source.id)
            || source
                .authorization_scope_id
                .as_deref()
                .is_some_and(|scope| !safe_label(scope))
        {
            return Err(EvalContractError::new("observation_identifier_unsafe"));
        }
        let Some(expected_kind) = sources.get(source.id.as_str()) else {
            return Err(EvalContractError::new("observation_source_unknown"));
        };
        if *expected_kind != source.kind {
            return Err(EvalContractError::new("observation_source_kind_mismatch"));
        }
        if !observed.insert((source.id.as_str(), source.kind)) {
            return Err(EvalContractError::new("observation_source_duplicate"));
        }
        if source.kind == SourceKind::Local
            && manifest.local_authorization.implicit_vault == ImplicitVaultExpectation::Forbidden
        {
            let explicit = manifest
                .local_authorization
                .explicit_reference_ids
                .iter()
                .any(|id| id == &source.id);
            let scoped = manifest
                .local_authorization
                .explicit_scope_id
                .as_deref()
                .is_some_and(|scope| {
                    source.authorization_scope_id.as_deref() == Some(scope)
                        && manifest
                            .local_authorization
                            .explicit_scope_source_ids
                            .iter()
                            .any(|id| id == &source.id)
                });
            if !explicit && !scoped {
                return Err(EvalContractError::new("observation_scope_outside"));
            }
        }
    }
    let facts = manifest
        .required_facts
        .iter()
        .map(|fact| (fact.id.as_str(), fact))
        .collect::<HashMap<_, _>>();
    let observed_source_ids = observation
        .sources
        .iter()
        .map(|source| source.id.as_str())
        .collect::<HashSet<_>>();
    let mut supported = HashSet::new();
    for support in &observation.fact_supports {
        if !safe_label(&support.fact_id) || !supported.insert(support.fact_id.as_str()) {
            return Err(EvalContractError::new("observation_fact_duplicate"));
        }
        let Some(fact) = facts.get(support.fact_id.as_str()) else {
            return Err(EvalContractError::new("observation_fact_unknown"));
        };
        if support.source_ids.is_empty() {
            return Err(EvalContractError::new("observation_fact_support_empty"));
        }
        let mut support_sources = HashSet::new();
        for source_id in &support.source_ids {
            if !safe_label(source_id) {
                return Err(EvalContractError::new("observation_identifier_unsafe"));
            }
            if !support_sources.insert(source_id.as_str()) {
                return Err(EvalContractError::new("observation_fact_support_duplicate"));
            }
            if !fact.allowed_sources.contains(source_id)
                || !observed_source_ids.contains(source_id.as_str())
            {
                return Err(EvalContractError::new("observation_fact_support_invalid"));
            }
        }
    }
    let mut contradicted = HashSet::new();
    for fact_id in &observation.contradicted_fact_ids {
        if !safe_label(fact_id) {
            return Err(EvalContractError::new("observation_identifier_unsafe"));
        }
        if !facts.contains_key(fact_id.as_str()) {
            return Err(EvalContractError::new("observation_fact_unknown"));
        }
        if !contradicted.insert(fact_id.as_str()) {
            return Err(EvalContractError::new("observation_fact_duplicate"));
        }
        if supported.contains(fact_id.as_str()) {
            return Err(EvalContractError::new("observation_fact_conflict"));
        }
    }
    let mut citations = HashSet::new();
    for citation in &observation.citations {
        if !safe_label(&citation.fact_id) || !safe_label(&citation.source_id) {
            return Err(EvalContractError::new("observation_identifier_unsafe"));
        }
        let Some(fact) = facts.get(citation.fact_id.as_str()) else {
            return Err(EvalContractError::new("observation_fact_unknown"));
        };
        if !citations.insert((citation.fact_id.as_str(), citation.source_id.as_str())) {
            return Err(EvalContractError::new("observation_citation_duplicate"));
        }
        if !fact.allowed_sources.contains(&citation.source_id)
            || !observed_source_ids.contains(citation.source_id.as_str())
        {
            return Err(EvalContractError::new("observation_citation_invalid"));
        }
    }
    let known_tools = manifest
        .tool_policy
        .allowed
        .iter()
        .chain(manifest.tool_policy.forbidden.iter())
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut tools = HashSet::new();
    for tool in &observation.tool_calls {
        if !safe_label(tool) {
            return Err(EvalContractError::new("observation_identifier_unsafe"));
        }
        if !known_tools.contains(tool.as_str()) {
            return Err(EvalContractError::new("observation_tool_unknown"));
        }
        if !tools.insert(tool.as_str()) {
            return Err(EvalContractError::new("observation_tool_duplicate"));
        }
    }
    let allowed_disclosures = manifest
        .disclosure_constraints
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut disclosures = HashSet::new();
    for disclosure in &observation.disclosures {
        if !safe_label(disclosure) {
            return Err(EvalContractError::new("observation_identifier_unsafe"));
        }
        if !allowed_disclosures.contains(disclosure.as_str()) {
            return Err(EvalContractError::new("observation_disclosure_unknown"));
        }
        if !disclosures.insert(disclosure.as_str()) {
            return Err(EvalContractError::new("observation_disclosure_duplicate"));
        }
    }
    Ok(())
}

/// MCP operation represented by one configured capability mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum McpOperation {
    Search,
    Fetch,
}

/// Evidence level reported for a protocol shape. A mapping shape is not a
/// transport proof: only a real deterministic protocol peer may claim the
/// transport-contract level. Neither level implies a live vendor call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProtocolValidationLevel {
    MappingShapeVerified,
    ContractVerified,
    LiveNotTested,
}

/// Safe protocol-boundary outcome. It classifies Iris adapter behavior only;
/// it never represents a live vendor capability result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProtocolContractOutcome {
    Timeout,
    Unavailable,
    ToolNotFound,
    SchemaMismatch,
    OutputTooLarge,
    AuthenticationFailure,
    NetworkDenied,
    PolicyDenied,
    InvalidResponse,
}

impl ProtocolContractOutcome {
    pub(crate) fn from_mcp_runtime_failure(
        failure: crate::ai_runtime::mcp_host_runtime::McpRuntimeFailureKind,
    ) -> Self {
        use crate::ai_runtime::mcp_host_runtime::McpRuntimeFailureKind;

        match failure {
            McpRuntimeFailureKind::Timeout => Self::Timeout,
            McpRuntimeFailureKind::Unavailable => Self::Unavailable,
            McpRuntimeFailureKind::ToolNotFound => Self::ToolNotFound,
            McpRuntimeFailureKind::SchemaMismatch => Self::SchemaMismatch,
            McpRuntimeFailureKind::OutputTooLarge => Self::OutputTooLarge,
            McpRuntimeFailureKind::AuthMissing | McpRuntimeFailureKind::AuthFailed => {
                Self::AuthenticationFailure
            }
            McpRuntimeFailureKind::NetworkDenied => Self::NetworkDenied,
            McpRuntimeFailureKind::PolicyDenied => Self::PolicyDenied,
            McpRuntimeFailureKind::InvalidResponse => Self::InvalidResponse,
        }
    }

    pub(crate) const fn reason_code(self) -> &'static str {
        match self {
            Self::Timeout => "mcp_protocol_timeout",
            Self::Unavailable => "mcp_protocol_unavailable",
            Self::ToolNotFound => "mcp_protocol_tool_not_found",
            Self::SchemaMismatch => "mcp_protocol_schema_mismatch",
            Self::OutputTooLarge => "mcp_protocol_output_too_large",
            Self::AuthenticationFailure => "mcp_protocol_authentication_failure",
            Self::NetworkDenied => "mcp_protocol_network_denied",
            Self::PolicyDenied => "mcp_protocol_policy_denied",
            Self::InvalidResponse => "mcp_protocol_invalid_response",
        }
    }

    pub(crate) const fn validation_level(self) -> ProtocolValidationLevel {
        ProtocolValidationLevel::ContractVerified
    }

    pub(crate) const fn live_vendor_tested(self) -> bool {
        false
    }
}

/// Validated MCP mapping shape consumed by the evaluation runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct McpCapabilityContract {
    supports_search: bool,
    supports_fetch: bool,
    validation_level: ProtocolValidationLevel,
}

impl McpCapabilityContract {
    /// Validate provider-neutral mapping shapes without contacting a vendor.
    pub(crate) fn from_mappings(
        search_mapping: Option<&str>,
        fetch_mapping: Option<&str>,
    ) -> Result<Self, EvalContractError> {
        if search_mapping.is_none() && fetch_mapping.is_some() {
            return Err(EvalContractError::new("mcp_fetch_without_search"));
        }
        let Some(search_mapping) = search_mapping else {
            return Err(EvalContractError::new("mcp_search_unmapped"));
        };
        validate_mcp_mapping(search_mapping)?;
        if let Some(fetch_mapping) = fetch_mapping {
            validate_mcp_mapping(fetch_mapping)?;
        }
        Ok(Self {
            supports_search: true,
            supports_fetch: fetch_mapping.is_some(),
            validation_level: ProtocolValidationLevel::MappingShapeVerified,
        })
    }

    pub(crate) const fn validation_level(&self) -> ProtocolValidationLevel {
        self.validation_level
    }

    pub(crate) const fn supports(&self, operation: McpOperation) -> bool {
        match operation {
            McpOperation::Search => self.supports_search,
            McpOperation::Fetch => self.supports_fetch,
        }
    }

    pub(crate) fn require(&self, operation: McpOperation) -> Result<(), EvalContractError> {
        if self.supports(operation) {
            Ok(())
        } else {
            Err(EvalContractError::new("mcp_operation_unmapped"))
        }
    }
}

fn validate_mcp_mapping(raw: &str) -> Result<(), EvalContractError> {
    let mapping = serde_json::from_str::<serde_json::Value>(raw)
        .map_err(|_| EvalContractError::new("mcp_mapping_invalid"))?;
    let tool = mapping
        .get("tool")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !safe_label(tool) {
        return Err(EvalContractError::new("mcp_mapping_tool_invalid"));
    }
    Ok(())
}

#[cfg(test)]
use std::sync::{Arc, Mutex};
#[cfg(test)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(test)]
use tokio::net::TcpListener;
#[cfg(test)]
use tokio::task::JoinHandle;

/// One in-memory scripted LLM HTTP response.
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct HttpResponseScript {
    status: u16,
    body: String,
    content_type: &'static str,
    delay: std::time::Duration,
}

#[cfg(test)]
impl HttpResponseScript {
    pub(crate) fn json(body: serde_json::Value) -> Self {
        Self {
            status: 200,
            body: body.to_string(),
            content_type: "application/json",
            delay: std::time::Duration::ZERO,
        }
    }

    pub(crate) fn raw(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_string(),
            content_type: "application/json",
            delay: std::time::Duration::ZERO,
        }
    }

    /// Script a byte-for-byte SSE response for the production streaming path.
    pub(crate) fn sse(body: &str) -> Self {
        Self {
            status: 200,
            body: body.to_string(),
            content_type: "text/event-stream",
            delay: std::time::Duration::ZERO,
        }
    }

    pub(crate) fn with_delay(mut self, delay: std::time::Duration) -> Self {
        self.delay = delay;
        self
    }
}

/// Captured protocol shape. It lives in memory and has no serializer.
#[cfg(test)]
pub(crate) struct CapturedHttpRequest {
    pub(crate) path: String,
    pub(crate) body: serde_json::Value,
}

/// Local external-boundary protocol double. Debug output is always redacted.
#[cfg(test)]
pub(crate) struct LlmProtocolDouble {
    pub(crate) base_url: String,
    captures: Arc<Mutex<Vec<CapturedHttpRequest>>>,
    task: Option<JoinHandle<crate::error::AppResult<()>>>,
}

#[cfg(test)]
impl fmt::Debug for LlmProtocolDouble {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LlmProtocolDouble")
            .field("base_url", &"[redacted-local-boundary]")
            .field("requests", &"[redacted-in-memory]")
            .finish()
    }
}

#[cfg(test)]
impl LlmProtocolDouble {
    pub(crate) fn redacted_debug_contract() -> Self {
        Self {
            base_url: String::new(),
            captures: Arc::new(Mutex::new(Vec::new())),
            task: None,
        }
    }

    pub(crate) async fn finish(mut self) -> crate::error::AppResult<Vec<CapturedHttpRequest>> {
        if let Some(task) = self.task.take() {
            task.await
                .map_err(|_| crate::error::AppError::msg("eval_protocol_double_join_failed"))??;
        }
        Arc::try_unwrap(self.captures)
            .map_err(|_| crate::error::AppError::msg("eval_protocol_double_still_shared"))?
            .into_inner()
            .map_err(|_| crate::error::AppError::msg("eval_protocol_double_lock_failed"))
    }
}

/// Start a deterministic local HTTP peer used only to verify Iris adapter
/// contracts. It is not a model simulator and makes no capability claim.
#[cfg(test)]
pub(crate) async fn spawn_llm_protocol_double(
    scripts: Vec<HttpResponseScript>,
) -> crate::error::AppResult<LlmProtocolDouble> {
    if scripts.is_empty() {
        return Err(crate::error::AppError::msg(
            "eval_protocol_double_script_empty",
        ));
    }
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| crate::error::AppError::msg("eval_protocol_double_bind_failed"))?;
    let address = listener
        .local_addr()
        .map_err(|_| crate::error::AppError::msg("eval_protocol_double_address_failed"))?;
    let captures = Arc::new(Mutex::new(Vec::with_capacity(scripts.len())));
    let task_captures = Arc::clone(&captures);
    let task = tokio::spawn(async move {
        for script in scripts {
            let (mut socket, _) = listener
                .accept()
                .await
                .map_err(|_| crate::error::AppError::msg("eval_protocol_double_accept_failed"))?;
            let captured = read_http_request(&mut socket).await?;
            task_captures
                .lock()
                .map_err(|_| crate::error::AppError::msg("eval_protocol_double_lock_failed"))?
                .push(captured);
            if !script.delay.is_zero() {
                tokio::time::sleep(script.delay).await;
            }
            let status_text = match script.status {
                200 => "OK",
                429 => "Too Many Requests",
                500 => "Internal Server Error",
                _ => "Contract Response",
            };
            let response = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                script.status,
                status_text,
                script.content_type,
                script.body.len(),
                script.body
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
        Ok(())
    });
    Ok(LlmProtocolDouble {
        base_url: format!("http://{address}"),
        captures,
        task: Some(task),
    })
}

#[cfg(test)]
async fn read_http_request(
    socket: &mut tokio::net::TcpStream,
) -> crate::error::AppResult<CapturedHttpRequest> {
    const MAX_REQUEST_BYTES: usize = 256 * 1024;
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = socket
            .read(&mut chunk)
            .await
            .map_err(|_| crate::error::AppError::msg("eval_protocol_double_read_failed"))?;
        if read == 0 {
            return Err(crate::error::AppError::msg(
                "eval_protocol_double_request_incomplete",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(crate::error::AppError::msg(
                "eval_protocol_double_request_too_large",
            ));
        }
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let header_text = String::from_utf8_lossy(&bytes[..header_end]);
    let mut lines = header_text.lines();
    let path = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| crate::error::AppError::msg("eval_protocol_double_request_invalid"))?
        .to_string();
    let content_length = lines
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let expected_len = header_end.saturating_add(content_length);
    while bytes.len() < expected_len {
        let read = socket
            .read(&mut chunk)
            .await
            .map_err(|_| crate::error::AppError::msg("eval_protocol_double_read_failed"))?;
        if read == 0 {
            return Err(crate::error::AppError::msg(
                "eval_protocol_double_request_incomplete",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(crate::error::AppError::msg(
                "eval_protocol_double_request_too_large",
            ));
        }
    }
    let body = serde_json::from_slice(&bytes[header_end..expected_len])
        .map_err(|_| crate::error::AppError::msg("eval_protocol_double_body_invalid"))?;
    Ok(CapturedHttpRequest { path, body })
}
