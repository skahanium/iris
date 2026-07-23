//! Versioned, provider-neutral contracts for Agent answer-capacity evaluation.
//!
//! This module deliberately stores only stable synthetic identifiers and
//! bounded verdict codes. Raw prompts, model answers, note paths, source URLs,
//! provider payloads, and credentials are not part of any serializable type.

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

/// Minimal evidence needed to answer one evaluation case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvidenceGroup {
    NoRetrieval,
    LocalOnly,
    WebOnly,
    Hybrid,
}

/// Whether Web access is available to the evaluated Run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
        parse_case_ordinal(&self.id)?;
        for value in std::iter::once(self.language.as_str())
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

/// Case identifiers are deliberately an opaque, bounded ordinal rather than
/// a general-purpose label. This keeps serialized verdicts free from text a
/// fixture author could use to smuggle secret-like payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
struct CaseOrdinal(u32);

fn parse_case_ordinal(value: &str) -> Result<CaseOrdinal, EvalContractError> {
    let Some(raw_ordinal) = value.strip_prefix("case-") else {
        return Err(EvalContractError::new("manifest_case_id_invalid"));
    };
    if raw_ordinal.is_empty()
        || raw_ordinal.len() > 6
        || (raw_ordinal.len() > 1 && raw_ordinal.starts_with('0'))
        || !raw_ordinal.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(EvalContractError::new("manifest_case_id_invalid"));
    }
    let ordinal = raw_ordinal
        .parse::<u32>()
        .map_err(|_| EvalContractError::new("manifest_case_id_invalid"))?;
    if ordinal == 0 {
        return Err(EvalContractError::new("manifest_case_id_invalid"));
    }
    Ok(CaseOrdinal(ordinal))
}

fn looks_like_encoded_payload(value: &str) -> bool {
    if value.len() < 16 {
        return false;
    }
    if value.len() % 2 == 0 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return true;
    }
    if value
        .bytes()
        .all(|byte| byte.is_ascii_uppercase() || matches!(byte, b'2'..=b'7'))
    {
        return true;
    }

    use base64::{
        engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
        Engine as _,
    };
    [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD]
        .into_iter()
        .any(|engine| {
            engine
                .decode(value)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .is_some_and(|decoded| {
                    !decoded.is_empty()
                        && decoded.chars().all(|character| {
                            character.is_ascii_graphic() || character.is_whitespace()
                        })
                })
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
struct ValidatedCaseId(CaseOrdinal);

/// Stable, raw-content-free verdict consumed by reports and CI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
        case_id: ValidatedCaseId(parse_case_ordinal(&manifest.id)?),
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
    let mut fact_support_sources = HashMap::new();
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
        fact_support_sources.insert(support.fact_id.as_str(), support_sources);
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
        if !fact_support_sources
            .get(citation.fact_id.as_str())
            .is_some_and(|sources| sources.contains(citation.source_id.as_str()))
        {
            return Err(EvalContractError::new(
                "observation_citation_support_mismatch",
            ));
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

/// Closed language classes used by the core capacity suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScenarioLanguage {
    Chinese,
    English,
    Mixed,
}

/// One generated core scenario. The prompt itself remains an ephemeral fixture
/// concern; this contract carries only closed classes and bounded synthetic IDs.
#[derive(Debug, Clone)]
pub(crate) struct CoreScenario {
    base_question_id: u32,
    language: ScenarioLanguage,
    hard_boundary: bool,
    prompt: &'static str,
    manifest: CaseManifest,
}

impl CoreScenario {
    pub(crate) fn case_id(&self) -> u32 {
        parse_case_ordinal(&self.manifest.id)
            .expect("generated scenario IDs are validated")
            .0
    }

    pub(crate) const fn base_question_id(&self) -> u32 {
        self.base_question_id
    }

    pub(crate) const fn evidence_group(&self) -> EvidenceGroup {
        self.manifest.evidence_group
    }

    pub(crate) const fn web_state(&self) -> WebState {
        self.manifest.web_state
    }

    pub(crate) const fn language(&self) -> ScenarioLanguage {
        self.language
    }

    pub(crate) const fn prompt(&self) -> &'static str {
        self.prompt
    }

    pub(crate) const fn is_hard_boundary(&self) -> bool {
        self.hard_boundary
    }
}

#[derive(Clone, Copy)]
struct BaseQuestionPlan {
    group: EvidenceGroup,
    language: ScenarioLanguage,
    domain: &'static str,
    answer_mode: AnswerMode,
    prompt: &'static str,
}

const BASE_QUESTION_PLANS: [BaseQuestionPlan; 24] = [
    BaseQuestionPlan {
        group: EvidenceGroup::NoRetrieval,
        language: ScenarioLanguage::Chinese,
        domain: "writing",
        answer_mode: AnswerMode::Creative,
        prompt: "请在不检索任何资料的前提下，写一个三句式的产品发布开场白。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::NoRetrieval,
        language: ScenarioLanguage::Chinese,
        domain: "rewrite",
        answer_mode: AnswerMode::Rewrite,
        prompt: "请把“我们需要尽快解决这个问题”改写得更具体、克制，不增加新事实。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::NoRetrieval,
        language: ScenarioLanguage::Chinese,
        domain: "reasoning",
        answer_mode: AnswerMode::Creative,
        prompt: "请解释为什么反例足以否定全称命题，并给出一个纯虚构例子。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::NoRetrieval,
        language: ScenarioLanguage::Chinese,
        domain: "planning",
        answer_mode: AnswerMode::Creative,
        prompt: "请设计一个不依赖外部资料的十五分钟复盘流程，限定为四步。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::NoRetrieval,
        language: ScenarioLanguage::English,
        domain: "writing",
        answer_mode: AnswerMode::Rewrite,
        prompt: "Rewrite this synthetic status update in a concise, neutral tone without adding facts.",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::NoRetrieval,
        language: ScenarioLanguage::Mixed,
        domain: "engineering",
        answer_mode: AnswerMode::Creative,
        prompt: "用中文解释 idempotency，并用 one short English example 收尾；不要检索。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::LocalOnly,
        language: ScenarioLanguage::Chinese,
        domain: "notes",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "仅根据明确附带的 synthetic 项目笔记，列出已决定事项并逐条引用。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::LocalOnly,
        language: ScenarioLanguage::Chinese,
        domain: "project",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "根据授权的本地项目资料总结里程碑；联网开关不改变所需证据范围。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::LocalOnly,
        language: ScenarioLanguage::Chinese,
        domain: "research",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "从授权本地材料提炼三个研究假设，不得读取未授权笔记。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::LocalOnly,
        language: ScenarioLanguage::Chinese,
        domain: "meeting",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "根据本地会议记录生成行动项、负责人代号与依据引用。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::LocalOnly,
        language: ScenarioLanguage::English,
        domain: "notes",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "Summarize the explicitly authorized synthetic note and cite each claim.",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::LocalOnly,
        language: ScenarioLanguage::English,
        domain: "project",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "Compare milestones across the authorized local project scope without using Web facts.",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::Chinese,
        domain: "current-events",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "核实 synthetic 产品今天的公开状态，并为所有时效性事实提供网页证据。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::Chinese,
        domain: "market",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "查找 synthetic 市场的最新公开规模估计，区分事实与不确定性。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::Chinese,
        domain: "standards",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "核对 synthetic 标准的当前版本与发布日期，给出来源。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::Chinese,
        domain: "software",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "确认 synthetic 软件当前稳定版本，不使用本地笔记作为版本事实。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::Chinese,
        domain: "policy",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "检索 synthetic 政策的最新公开文本，并说明无法验证时的限制。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::WebOnly,
        language: ScenarioLanguage::English,
        domain: "research",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "Find the current public status of the synthetic study and cite supporting Web evidence.",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::Hybrid,
        language: ScenarioLanguage::Chinese,
        domain: "competitive-analysis",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "把授权本地方案与 synthetic 竞品的最新公开信息对比，分别引用本地与网页证据。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::Hybrid,
        language: ScenarioLanguage::Chinese,
        domain: "project-risk",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "结合本地风险登记与最新公开依赖状态，给出证据分层的风险判断。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::Hybrid,
        language: ScenarioLanguage::Chinese,
        domain: "technical-review",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "用授权设计记录解释内部约束，再核实外部 synthetic API 的当前兼容性。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::Hybrid,
        language: ScenarioLanguage::Chinese,
        domain: "decision-support",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "根据本地决策标准和最新公开事实比较两个 synthetic 选项。",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::Hybrid,
        language: ScenarioLanguage::English,
        domain: "research",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "Compare the authorized local hypothesis with current public synthetic evidence and cite both.",
    },
    BaseQuestionPlan {
        group: EvidenceGroup::Hybrid,
        language: ScenarioLanguage::Mixed,
        domain: "engineering",
        answer_mode: AnswerMode::EvidenceGrounded,
        prompt: "依据本地 design note 与最新 Web status 做 gap analysis，并清楚区分两类来源。",
    },
];

/// Generate the fixed 48-case core matrix from 24 base questions. Each base
/// question keeps its language and evidence class across one Offline and one
/// Online variant; enabling Web therefore never changes the evidence contract.
pub(crate) fn generate_core_scenarios() -> Result<Vec<CoreScenario>, EvalContractError> {
    let mut scenarios = Vec::with_capacity(BASE_QUESTION_PLANS.len() * 2);
    let mut group_base_index = HashMap::<EvidenceGroup, usize>::new();
    for (base_index, plan) in BASE_QUESTION_PLANS.iter().copied().enumerate() {
        let ordinal_in_group = *group_base_index.entry(plan.group).or_insert(0);
        *group_base_index.entry(plan.group).or_insert(0) += 1;
        for web_state in [WebState::Offline, WebState::Online] {
            let case_ordinal = u32::try_from(scenarios.len() + 1)
                .map_err(|_| EvalContractError::new("core_case_count_invalid"))?;
            let base_question_id = u32::try_from(base_index + 1)
                .map_err(|_| EvalContractError::new("core_base_count_invalid"))?;
            let manifest = build_core_manifest(case_ordinal, plan, web_state, ordinal_in_group);
            manifest.validate()?;
            scenarios.push(CoreScenario {
                base_question_id,
                language: plan.language,
                hard_boundary: ordinal_in_group == 0 && web_state == WebState::Offline,
                prompt: plan.prompt,
                manifest,
            });
        }
    }
    validate_core_matrix(&scenarios)?;
    Ok(scenarios)
}

fn build_core_manifest(
    case_ordinal: u32,
    plan: BaseQuestionPlan,
    web_state: WebState,
    ordinal_in_group: usize,
) -> CaseManifest {
    let local_id = format!("local-{case_ordinal}");
    let web_id = format!("web-{case_ordinal}");
    let local_fact_id = format!("fact-local-{case_ordinal}");
    let web_fact_id = format!("fact-web-{case_ordinal}");
    let needs_local = matches!(plan.group, EvidenceGroup::LocalOnly | EvidenceGroup::Hybrid);
    let needs_web = matches!(plan.group, EvidenceGroup::WebOnly | EvidenceGroup::Hybrid);
    let implicit_vault = if needs_local && ordinal_in_group % 2 == 1 {
        ImplicitVaultExpectation::Allowed
    } else {
        ImplicitVaultExpectation::Forbidden
    };
    let explicit_reference_ids =
        if needs_local && implicit_vault == ImplicitVaultExpectation::Forbidden {
            vec![local_id.clone()]
        } else {
            Vec::new()
        };
    let mut available_sources = Vec::new();
    let mut required_sources = Vec::new();
    let mut required_facts = Vec::new();
    if needs_local {
        let source = RequiredSource {
            id: local_id.clone(),
            kind: SourceKind::Local,
        };
        available_sources.push(source.clone());
        required_sources.push(source);
        required_facts.push(RequiredFact {
            id: local_fact_id,
            allowed_sources: vec![local_id],
            citation_required: true,
        });
    }
    if needs_web {
        let source = RequiredSource {
            id: web_id.clone(),
            kind: SourceKind::Web,
        };
        available_sources.push(source.clone());
        required_sources.push(source);
        required_facts.push(RequiredFact {
            id: web_fact_id,
            allowed_sources: vec![web_id],
            citation_required: true,
        });
    }

    let local_tools = [
        "read_note",
        "search_hybrid",
        "list_vault",
        "get_outline",
        "get_backlinks",
    ];
    let mut allowed = Vec::new();
    let mut forbidden = Vec::new();
    for tool in local_tools {
        if needs_local {
            allowed.push(tool.to_string());
        } else {
            forbidden.push(tool.to_string());
        }
    }
    // In Online mode a model may decide to search even when Web evidence is
    // unnecessary. The evaluator records that as route inefficiency, not a
    // permission failure, unless the answer becomes contaminated.
    allowed.push("web_search".to_string());

    CaseManifest {
        schema_version: "agent-answer-v1".to_string(),
        id: format!("case-{case_ordinal}"),
        evidence_group: plan.group,
        language: match plan.language {
            ScenarioLanguage::Chinese => "zh",
            ScenarioLanguage::English => "en",
            ScenarioLanguage::Mixed => "mixed",
        }
        .to_string(),
        domain: plan.domain.to_string(),
        web_state,
        local_authorization: LocalAuthorization {
            explicit_reference_ids,
            explicit_scope_id: None,
            explicit_scope_source_ids: Vec::new(),
            implicit_vault,
        },
        available_sources,
        required_facts,
        required_sources,
        tool_policy: ToolPolicy {
            allowed,
            forbidden,
            web_search: if needs_web {
                WebSearchPolicy::Required
            } else {
                WebSearchPolicy::Optional
            },
        },
        answer_mode: plan.answer_mode,
        citation_expectation: if needs_local || needs_web {
            CitationExpectation::Required
        } else {
            CitationExpectation::None
        },
        disclosure_constraints: if needs_web && web_state == WebState::Offline {
            vec!["web-offline-uncertainty".to_string()]
        } else {
            Vec::new()
        },
    }
}

fn validate_core_matrix(scenarios: &[CoreScenario]) -> Result<(), EvalContractError> {
    if scenarios.len() != 48 {
        return Err(EvalContractError::new("core_case_count_invalid"));
    }
    for group in [
        EvidenceGroup::NoRetrieval,
        EvidenceGroup::LocalOnly,
        EvidenceGroup::WebOnly,
        EvidenceGroup::Hybrid,
    ] {
        if scenarios
            .iter()
            .filter(|scenario| scenario.evidence_group() == group)
            .count()
            != 12
        {
            return Err(EvalContractError::new("core_group_distribution_invalid"));
        }
    }
    let language_count = |language| {
        scenarios
            .iter()
            .filter(|scenario| scenario.language() == language)
            .count()
    };
    // An Offline/Online pair shares one base question and language, hence all
    // counts are even. 34/10/4 minimizes error against 70/20/10 for 48 cases
    // while preserving those symmetric variants.
    if language_count(ScenarioLanguage::Chinese) != 34
        || language_count(ScenarioLanguage::English) != 10
        || language_count(ScenarioLanguage::Mixed) != 4
    {
        return Err(EvalContractError::new("core_language_distribution_invalid"));
    }
    Ok(())
}

/// Closed finish-reason classes; raw provider text never enters a result file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FinishReasonClass {
    Stop,
    ToolCalls,
    Length,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TruncationOutcome {
    None,
    ToolResultTruncated,
    FinalOutputRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BudgetOutcome {
    WithinBudget,
    ModelTurnsExhausted,
    ToolCallsExhausted,
    OutputBudgetReached,
}

#[derive(Debug, Default)]
struct EvaluationTelemetryState {
    model_turns: u32,
    tool_calls: u32,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    cache_hit_tokens: u64,
    cache_miss_tokens: u64,
    first_visible_token_ms: Option<u64>,
    total_model_time_ms: u64,
    finish_stop: u32,
    finish_tool_calls: u32,
    finish_length: u32,
    finish_other: u32,
    truncation_none: u32,
    truncation_tool_result: u32,
    truncation_final_output: u32,
    budget_within: u32,
    budget_model_turns: u32,
    budget_tool_calls: u32,
    budget_output: u32,
    final_output_recorded: bool,
}

/// Cloneable, evaluation-only in-memory tap. It owns no database handle and
/// exposes no raw provider, prompt, answer, token, tool-argument, or path data.
#[derive(Debug, Clone)]
pub(crate) struct EvaluationTelemetryTap {
    state: std::sync::Arc<std::sync::Mutex<EvaluationTelemetryState>>,
    started_at: std::sync::Arc<std::time::Instant>,
}

impl Default for EvaluationTelemetryTap {
    fn default() -> Self {
        Self {
            state: std::sync::Arc::new(std::sync::Mutex::new(EvaluationTelemetryState::default())),
            started_at: std::sync::Arc::new(std::time::Instant::now()),
        }
    }
}

impl EvaluationTelemetryTap {
    pub(crate) fn record_model_turn_at(
        &self,
        response: &crate::ai_runtime::model_gateway::GatewayResponse,
        elapsed_ms: u64,
    ) {
        if let Ok(mut state) = self.state.lock() {
            state.model_turns = state.model_turns.saturating_add(1);
            state.tool_calls = state
                .tool_calls
                .saturating_add(response.tool_calls.len().min(u32::MAX as usize) as u32);
            state.prompt_tokens = state
                .prompt_tokens
                .saturating_add(u64::from(response.usage.prompt_tokens));
            state.completion_tokens = state
                .completion_tokens
                .saturating_add(u64::from(response.usage.completion_tokens));
            state.total_tokens = state
                .total_tokens
                .saturating_add(u64::from(response.usage.total_tokens));
            state.cache_hit_tokens = state
                .cache_hit_tokens
                .saturating_add(u64::from(response.usage.prompt_cache_hit_tokens));
            state.cache_miss_tokens = state
                .cache_miss_tokens
                .saturating_add(u64::from(response.usage.prompt_cache_miss_tokens));
            state.total_model_time_ms = state.total_model_time_ms.saturating_add(elapsed_ms);
            match classify_finish_reason(&response.finish_reason) {
                FinishReasonClass::Stop => state.finish_stop = state.finish_stop.saturating_add(1),
                FinishReasonClass::ToolCalls => {
                    state.finish_tool_calls = state.finish_tool_calls.saturating_add(1);
                }
                FinishReasonClass::Length => {
                    state.finish_length = state.finish_length.saturating_add(1);
                }
                FinishReasonClass::Other => {
                    state.finish_other = state.finish_other.saturating_add(1);
                }
            }
        }
    }

    pub(crate) fn record_model_turn(
        &self,
        response: &crate::ai_runtime::model_gateway::GatewayResponse,
        started_at: std::time::Instant,
    ) {
        self.record_model_turn_at(
            response,
            started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        );
    }

    pub(crate) fn record_stream_event_at(
        &self,
        event: &crate::ai_runtime::model_gateway::StreamEvent,
        elapsed_ms: u64,
    ) {
        if !matches!(
            event.surface,
            crate::ai_runtime::model_gateway::StreamSurface::VisibleAnswer
                | crate::ai_runtime::model_gateway::StreamSurface::VisibleAnswerSanitized
        ) || !matches!(
            event.data,
            crate::ai_runtime::model_gateway::StreamEventData::Token { .. }
        ) {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            state.first_visible_token_ms = Some(
                state
                    .first_visible_token_ms
                    .map_or(elapsed_ms, |current| current.min(elapsed_ms)),
            );
        }
    }

    pub(crate) fn record_stream_event(
        &self,
        event: &crate::ai_runtime::model_gateway::StreamEvent,
    ) {
        self.record_stream_event_at(
            event,
            self.started_at
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        );
    }

    pub(crate) fn record_truncation(&self, outcome: TruncationOutcome) {
        if let Ok(mut state) = self.state.lock() {
            match outcome {
                TruncationOutcome::None => {
                    state.truncation_none = state.truncation_none.saturating_add(1);
                }
                TruncationOutcome::ToolResultTruncated => {
                    state.truncation_tool_result = state.truncation_tool_result.saturating_add(1);
                }
                TruncationOutcome::FinalOutputRejected => {
                    state.truncation_final_output = state.truncation_final_output.saturating_add(1);
                }
            }
        }
    }

    pub(crate) fn record_budget(&self, outcome: BudgetOutcome) {
        if let Ok(mut state) = self.state.lock() {
            match outcome {
                BudgetOutcome::WithinBudget => {
                    state.budget_within = state.budget_within.saturating_add(1);
                }
                BudgetOutcome::ModelTurnsExhausted => {
                    state.budget_model_turns = state.budget_model_turns.saturating_add(1);
                }
                BudgetOutcome::ToolCallsExhausted => {
                    state.budget_tool_calls = state.budget_tool_calls.saturating_add(1);
                }
                BudgetOutcome::OutputBudgetReached => {
                    state.budget_output = state.budget_output.saturating_add(1);
                }
            }
        }
    }

    pub(crate) fn record_final_output_validation(
        &self,
        accepted: bool,
        output_budget_reached: bool,
    ) {
        if let Ok(mut state) = self.state.lock() {
            if state.final_output_recorded {
                return;
            }
            state.final_output_recorded = true;
            if accepted {
                state.truncation_none = state.truncation_none.saturating_add(1);
                state.budget_within = state.budget_within.saturating_add(1);
            } else {
                state.truncation_final_output = state.truncation_final_output.saturating_add(1);
                if output_budget_reached {
                    state.budget_output = state.budget_output.saturating_add(1);
                } else {
                    state.budget_within = state.budget_within.saturating_add(1);
                }
            }
        }
    }

    pub(crate) fn snapshot(&self) -> EvaluationTelemetrySummary {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        EvaluationTelemetrySummary {
            model_turns: state.model_turns,
            tool_calls: state.tool_calls,
            prompt_tokens: state.prompt_tokens,
            completion_tokens: state.completion_tokens,
            total_tokens: state.total_tokens,
            cache_hit_tokens: state.cache_hit_tokens,
            cache_miss_tokens: state.cache_miss_tokens,
            first_visible_token_ms: state.first_visible_token_ms,
            total_model_time_ms: state.total_model_time_ms,
            finish_reasons: FinishReasonCounts {
                stop: state.finish_stop,
                tool_calls: state.finish_tool_calls,
                length: state.finish_length,
                other: state.finish_other,
            },
            truncations: TruncationCounts {
                none: state.truncation_none,
                tool_result: state.truncation_tool_result,
                final_output: state.truncation_final_output,
            },
            budgets: BudgetCounts {
                within: state.budget_within,
                model_turns: state.budget_model_turns,
                tool_calls: state.budget_tool_calls,
                output: state.budget_output,
            },
        }
    }
}

fn classify_finish_reason(value: &str) -> FinishReasonClass {
    match value.trim().to_ascii_lowercase().as_str() {
        "stop" | "end_turn" | "completed" => FinishReasonClass::Stop,
        "tool_calls" | "tool_use" => FinishReasonClass::ToolCalls,
        "length" | "max_tokens" => FinishReasonClass::Length,
        _ => FinishReasonClass::Other,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FinishReasonCounts {
    stop: u32,
    tool_calls: u32,
    length: u32,
    other: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TruncationCounts {
    none: u32,
    tool_result: u32,
    final_output: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BudgetCounts {
    within: u32,
    model_turns: u32,
    tool_calls: u32,
    output: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EvaluationTelemetrySummary {
    model_turns: u32,
    tool_calls: u32,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    cache_hit_tokens: u64,
    cache_miss_tokens: u64,
    first_visible_token_ms: Option<u64>,
    total_model_time_ms: u64,
    finish_reasons: FinishReasonCounts,
    truncations: TruncationCounts,
    budgets: BudgetCounts,
}

impl EvaluationTelemetrySummary {
    pub(crate) const fn model_turns(&self) -> u32 {
        self.model_turns
    }

    pub(crate) const fn tool_calls(&self) -> u32 {
        self.tool_calls
    }

    pub(crate) const fn total_tokens(&self) -> u64 {
        self.total_tokens
    }

    pub(crate) const fn first_visible_token_ms(&self) -> Option<u64> {
        self.first_visible_token_ms
    }

    pub(crate) const fn total_model_time_ms(&self) -> u64 {
        self.total_model_time_ms
    }

    pub(crate) const fn tool_result_truncations(&self) -> u32 {
        self.truncations.tool_result
    }

    pub(crate) const fn final_output_successes(&self) -> u32 {
        self.truncations.none
    }

    pub(crate) const fn final_output_rejections(&self) -> u32 {
        self.truncations.final_output
    }

    pub(crate) const fn output_budget_reached(&self) -> u32 {
        self.budgets.output
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvalRunMode {
    Smoke,
    Full,
}

/// Strength of the evidence behind one result file. The headless harness
/// validates Iris orchestration with deterministic external peers; it does not
/// claim live model or vendor capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvaluationEvidenceLevel {
    HeadlessDeterministic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GroupCounts {
    no_retrieval: u32,
    local_only: u32,
    web_only: u32,
    hybrid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LanguageCounts {
    chinese: u32,
    english: u32,
    mixed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvaluationCaseSummary {
    case_id: u32,
    evidence_group: EvidenceGroup,
    web_state: WebState,
    language: ScenarioLanguage,
    required_fact_ids: Vec<ValidatedFactId>,
    runtime_evidence: RuntimeEvidenceSummary,
    boundary: Option<BoundaryVerdict>,
    verdict: EvaluationVerdict,
    overall_pass: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
struct ValidatedFactId(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum EvaluationTerminalState {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeEvidenceSummary {
    terminal_state: EvaluationTerminalState,
    event_count: u32,
    observed_source_kinds: Vec<SourceKind>,
    tool_call_count: u32,
    degradation_observed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum BoundaryKind {
    OfflineDirectGate,
    ExplicitLocalIsolation,
    OfflineWebDegradation,
    OfflineHybridPartialEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum BoundaryReason {
    Verified,
    TerminalStateMismatch,
    WebDispatchObservedOffline,
    LocalIsolationFailed,
    DegradationMissing,
    PartialEvidenceMissing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BoundaryVerdict {
    kind: BoundaryKind,
    status: CheckStatus,
    reason_code: BoundaryReason,
}

/// Closed, persistence-safe evaluation result. All fields are fixed enums,
/// bounded counters, booleans, or the Task-2 numeric case ordinal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EvaluationSummary {
    schema_version: &'static str,
    evidence_level: EvaluationEvidenceLevel,
    run_mode: EvalRunMode,
    case_count: u32,
    passed: u32,
    failed: u32,
    boundary_case_count: u32,
    groups: GroupCounts,
    languages: LanguageCounts,
    telemetry: EvaluationTelemetrySummary,
    cases: Vec<EvaluationCaseSummary>,
}

impl EvaluationSummary {
    pub(crate) const fn case_count(&self) -> u32 {
        self.case_count
    }

    pub(crate) const fn passed(&self) -> u32 {
        self.passed
    }

    pub(crate) const fn boundary_case_count(&self) -> u32 {
        self.boundary_case_count
    }

    pub(crate) const fn group_count(&self, group: EvidenceGroup) -> u32 {
        match group {
            EvidenceGroup::NoRetrieval => self.groups.no_retrieval,
            EvidenceGroup::LocalOnly => self.groups.local_only,
            EvidenceGroup::WebOnly => self.groups.web_only,
            EvidenceGroup::Hybrid => self.groups.hybrid,
        }
    }

    pub(crate) const fn language_count(&self, language: ScenarioLanguage) -> u32 {
        match language {
            ScenarioLanguage::Chinese => self.languages.chinese,
            ScenarioLanguage::English => self.languages.english,
            ScenarioLanguage::Mixed => self.languages.mixed,
        }
    }

    pub(crate) const fn telemetry(&self) -> &EvaluationTelemetrySummary {
        &self.telemetry
    }

    pub(crate) fn case_verdict(&self, case_id: u32) -> Option<&EvaluationVerdict> {
        self.cases
            .iter()
            .find(|case| case.case_id == case_id)
            .map(|case| &case.verdict)
    }
}

/// Select the fixed core subset. Selection alone makes no capability claim.
pub(crate) fn select_core_scenarios(
    mode: EvalRunMode,
) -> Result<Vec<CoreScenario>, EvalContractError> {
    let scenarios = generate_core_scenarios()?;
    Ok(match mode {
        EvalRunMode::Full => scenarios,
        EvalRunMode::Smoke => {
            let mut chinese_online = HashSet::<EvidenceGroup>::new();
            let mut minority_online = HashSet::<EvidenceGroup>::new();
            scenarios
                .into_iter()
                .filter(|scenario| {
                    if scenario.is_hard_boundary() {
                        return true;
                    }
                    if scenario.web_state() != WebState::Online {
                        return false;
                    }
                    if scenario.language() == ScenarioLanguage::Chinese {
                        return chinese_online.insert(scenario.evidence_group());
                    }
                    let expected_minority =
                        if scenario.evidence_group() == EvidenceGroup::NoRetrieval {
                            ScenarioLanguage::Mixed
                        } else {
                            ScenarioLanguage::English
                        };
                    scenario.language() == expected_minority
                        && minority_online.insert(scenario.evidence_group())
                })
                .collect()
        }
    })
}

/// Test-only deterministic-provider fault used to prove that the headless
/// runner reports a real failed answer instead of copying the manifest.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvalFault {
    MissingFact { case_id: u32 },
}

#[cfg(test)]
#[derive(Default)]
struct HeadlessEvaluationSink {
    tool_calls: std::sync::Mutex<Vec<String>>,
    degraded: std::sync::Mutex<bool>,
}

#[cfg(test)]
impl crate::ai_runtime::run_engine::RunEventSink for HeadlessEvaluationSink {
    fn emit(
        &self,
        event: &crate::ai_runtime::run_contract::AssistantRunEvent,
    ) -> crate::error::AppResult<()> {
        match event.payload() {
            crate::ai_runtime::run_contract::RunEventPayload::ToolStarted {
                capability, ..
            } => {
                self.tool_calls
                    .lock()
                    .map_err(|_| crate::error::AppError::msg("eval_sink_lock_failed"))?
                    .push(capability.clone());
            }
            crate::ai_runtime::run_contract::RunEventPayload::CapabilityDegraded { .. }
            | crate::ai_runtime::run_contract::RunEventPayload::WebVerificationFailed { .. } => {
                *self
                    .degraded
                    .lock()
                    .map_err(|_| crate::error::AppError::msg("eval_sink_lock_failed"))? = true;
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
struct ExecutedCoreCase {
    summary: EvaluationCaseSummary,
    telemetry: EvaluationTelemetrySummary,
}

/// Execute every selected case through the Task-1 headless normal service.
/// Only the LLM HTTP and MCP stdio peers are deterministic doubles.
#[cfg(test)]
pub(crate) async fn run_headless_core_evaluation(
    mode: EvalRunMode,
    fault: Option<EvalFault>,
) -> Result<EvaluationSummary, EvalContractError> {
    let selected = select_core_scenarios(mode)?;
    let mut executed = Vec::with_capacity(selected.len());
    for scenario in &selected {
        executed.push(execute_headless_core_case(scenario, fault).await?);
    }
    let cases = executed
        .iter()
        .map(|result| result.summary.clone())
        .collect::<Vec<_>>();
    let passed = cases
        .iter()
        .filter(|case| case.overall_pass)
        .count()
        .min(u32::MAX as usize) as u32;
    let group_count = |group| {
        selected
            .iter()
            .filter(|scenario| scenario.evidence_group() == group)
            .count()
            .min(u32::MAX as usize) as u32
    };
    let language_count = |language| {
        selected
            .iter()
            .filter(|scenario| scenario.language() == language)
            .count()
            .min(u32::MAX as usize) as u32
    };
    let case_count = selected.len().min(u32::MAX as usize) as u32;
    Ok(EvaluationSummary {
        schema_version: "agent-eval-summary-v1",
        evidence_level: EvaluationEvidenceLevel::HeadlessDeterministic,
        run_mode: mode,
        case_count,
        passed,
        failed: case_count.saturating_sub(passed),
        boundary_case_count: selected
            .iter()
            .filter(|scenario| scenario.is_hard_boundary())
            .count()
            .min(u32::MAX as usize) as u32,
        groups: GroupCounts {
            no_retrieval: group_count(EvidenceGroup::NoRetrieval),
            local_only: group_count(EvidenceGroup::LocalOnly),
            web_only: group_count(EvidenceGroup::WebOnly),
            hybrid: group_count(EvidenceGroup::Hybrid),
        },
        languages: LanguageCounts {
            chinese: language_count(ScenarioLanguage::Chinese),
            english: language_count(ScenarioLanguage::English),
            mixed: language_count(ScenarioLanguage::Mixed),
        },
        telemetry: aggregate_telemetry(executed.iter().map(|result| &result.telemetry)),
        cases,
    })
}

#[cfg(test)]
async fn execute_headless_core_case(
    scenario: &CoreScenario,
    fault: Option<EvalFault>,
) -> Result<ExecutedCoreCase, EvalContractError> {
    use crate::ai_runtime::normal_run_service::execute_normal_run_with_eval_telemetry;
    use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
    use crate::ai_runtime::run_contract::{
        AssistantRunStartRequest, AssistantTurnDraft, SecurityDomain,
    };
    use crate::ai_runtime::run_intake::RunIntake;
    use crate::ai_types::{ContextReferenceKind, ContextReferenceWire};
    use crate::llm::config::{LlmRoutingConfig, ModelReference, ProviderOverride};

    let directory =
        tempfile::tempdir().map_err(|_| EvalContractError::new("eval_temp_directory_failed"))?;
    let vault = directory.path().join("vault");
    std::fs::create_dir_all(vault.join("notes"))
        .map_err(|_| EvalContractError::new("eval_vault_setup_failed"))?;
    let local_body = format!("synthetic material {}", scenario.case_id());
    std::fs::write(vault.join("notes/authorized.md"), &local_body)
        .map_err(|_| EvalContractError::new("eval_vault_setup_failed"))?;
    std::fs::write(
        vault.join("notes/unmentioned.md"),
        "synthetic unmentioned material",
    )
    .map_err(|_| EvalContractError::new("eval_vault_setup_failed"))?;
    let state = crate::app::AppState::new(directory.path().join("data"))
        .map_err(|_| EvalContractError::new("eval_state_setup_failed"))?;
    if scenario
        .manifest
        .required_sources
        .iter()
        .any(|source| source.kind == SourceKind::Web && scenario.web_state() == WebState::Online)
    {
        install_headless_eval_mcp(&state)?;
    }
    let final_content = headless_final_content(scenario, fault);
    let needs_web_tool = scenario.web_state() == WebState::Online
        && scenario
            .manifest
            .required_sources
            .iter()
            .any(|source| source.kind == SourceKind::Web);
    let scripts = if needs_web_tool {
        vec![
            HttpResponseScript::sse(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"eval-web-call\",\"type\":\"function\",\"function\":{\"name\":\"web_search\",\"arguments\":\"{\\\"query\\\":\\\"synthetic\\\"}\"}}]}}]}\n\ndata: [DONE]\n\n",
            ),
            sse_content(&final_content),
        ]
    } else {
        vec![sse_content(&final_content)]
    };
    let llm = spawn_llm_protocol_double(scripts)
        .await
        .map_err(|_| EvalContractError::new("eval_llm_double_failed"))?;
    let mut routing = LlmRoutingConfig::default();
    routing.providers.clear();
    routing.providers.insert(
        "custom".to_string(),
        ProviderOverride {
            base_url: Some(llm.base_url.clone()),
            enabled_models: Some(vec!["agent-capacity-contract".to_string()]),
            ..Default::default()
        },
    );
    routing.default_model = Some(ModelReference {
        provider_id: "custom".to_string(),
        model_id: "agent-capacity-contract".to_string(),
    });
    crate::llm::config::save(&state.db, &routing)
        .map_err(|_| EvalContractError::new("eval_route_setup_failed"))?;
    state.set_test_streaming_client(reqwest::Client::new());
    let explicit_references = if scenario
        .manifest
        .local_authorization
        .explicit_reference_ids
        .is_empty()
    {
        Vec::new()
    } else {
        vec![ContextReferenceWire {
            id: scenario.manifest.local_authorization.explicit_reference_ids[0].clone(),
            kind: ContextReferenceKind::Note,
            file_path: Some("notes/authorized.md".to_string()),
            content_hash: Some(crate::cas::hash::content_hash_str(&local_body)),
            utf8_range: None,
            editor_range: None,
            excerpt: String::new(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        }]
    };
    let request = AssistantRunStartRequest {
        client_request_id: format!("agent-eval-{}", scenario.case_id()),
        session: None,
        turn: AssistantTurnDraft {
            message: scenario.prompt().to_string(),
            content_parts: None,
            explicit_references,
            retrieval_scope: Default::default(),
            display_mentions: Vec::new(),
        },
        explicit_action: None,
        web_enabled: scenario.web_state() == WebState::Online,
        model_override: None,
        security_domain: SecurityDomain::Normal,
        classified_context_ref: None,
    };
    let sink = HeadlessEvaluationSink::default();
    let accepted = RunIntake::start_with_sink(&state.db, request, &sink)
        .map_err(|_| EvalContractError::new("eval_run_intake_failed"))?;
    let telemetry = EvaluationTelemetryTap::default();
    execute_normal_run_with_eval_telemetry(
        std::sync::Arc::clone(&state),
        accepted.clone(),
        Some(vault),
        &sink,
        &telemetry,
    )
    .await;
    let captures = tokio::time::timeout(std::time::Duration::from_secs(3), llm.finish())
        .await
        .map_err(|_| EvalContractError::new("eval_llm_double_incomplete"))?
        .map_err(|_| EvalContractError::new("eval_llm_double_failed"))?;
    if captures.is_empty() {
        return Err(EvalContractError::new("eval_llm_double_unused"));
    }
    let response = RunIntake::get(&state.db, &accepted.session, &accepted.run_id)
        .map_err(|_| EvalContractError::new("eval_run_read_failed"))?
        .ok_or_else(|| EvalContractError::new("eval_run_missing"))?;
    let final_answer =
        NormalSessionRepository::load_messages(&state.db, &accepted.session.session_key, 8)
            .map_err(|_| EvalContractError::new("eval_messages_read_failed"))?
            .into_iter()
            .rev()
            .find(|message| message.role == "assistant")
            .map_or_else(String::new, |message| message.content);
    let observed_kinds = state
        .db
        .with_read_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT DISTINCT source_type FROM session_evidence WHERE origin_run_id = ?1",
            )?;
            let kinds = statement
                .query_map([&accepted.run_id], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into);
            kinds
        })
        .map_err(|_| EvalContractError::new("eval_evidence_read_failed"))?;
    let sources = observed_kinds
        .iter()
        .filter_map(|kind| {
            let kind = match kind.as_str() {
                "local" => SourceKind::Local,
                "web" => SourceKind::Web,
                _ => return None,
            };
            scenario
                .manifest
                .available_sources
                .iter()
                .find(|source| source.kind == kind)
                .map(|source| ObservedSource {
                    id: source.id.clone(),
                    kind,
                    authorization_scope_id: None,
                })
        })
        .collect::<Vec<_>>();
    let observed_ids = sources
        .iter()
        .map(|source| source.id.as_str())
        .collect::<HashSet<_>>();
    let fact_supports = scenario
        .manifest
        .required_facts
        .iter()
        .filter_map(|fact| {
            fact.allowed_sources
                .iter()
                .find(|source| {
                    observed_ids.contains(source.as_str()) && final_answer.contains(&fact.id)
                })
                .map(|source| FactSupportObservation {
                    fact_id: fact.id.clone(),
                    source_ids: vec![source.clone()],
                })
        })
        .collect::<Vec<_>>();
    let citations = fact_supports
        .iter()
        .filter_map(|support| {
            let source_id = &support.source_ids[0];
            final_answer
                .contains(&format!("[cite:{source_id}]"))
                .then(|| CitationObservation {
                    fact_id: support.fact_id.clone(),
                    source_id: source_id.clone(),
                })
        })
        .collect();
    let tool_calls = sink
        .tool_calls
        .lock()
        .map_err(|_| EvalContractError::new("eval_sink_lock_failed"))?
        .clone();
    let degraded_event = *sink
        .degraded
        .lock()
        .map_err(|_| EvalContractError::new("eval_sink_lock_failed"))?;
    let disclosures = scenario
        .manifest
        .disclosure_constraints
        .iter()
        .filter(|constraint| final_answer.contains(constraint.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let observation = AnswerObservation {
        case_id: scenario.manifest.id.clone(),
        sources,
        fact_supports,
        contradicted_fact_ids: Vec::new(),
        citations,
        tool_calls,
        disclosures,
        degraded: degraded_event || final_answer.contains("degraded:"),
        clarification_requested: false,
        web_answer_contamination: if final_answer.contains("fact-web-")
            && matches!(
                scenario.evidence_group(),
                EvidenceGroup::NoRetrieval | EvidenceGroup::LocalOnly
            ) {
            WebAnswerContamination::Detected
        } else {
            WebAnswerContamination::ConfirmedAbsent
        },
        safety_violations: Vec::new(),
    };
    let verdict = evaluate_case(&scenario.manifest, &observation)?;
    let boundary = evaluate_hard_boundary(
        scenario,
        response.run.state,
        &observation,
        observed_kinds.len(),
    );
    let boundary_pass = boundary
        .as_ref()
        .is_none_or(|result| result.status == CheckStatus::Pass);
    let required_fact_ids = scenario
        .manifest
        .required_facts
        .iter()
        .map(|fact| ValidatedFactId(fact.id.clone()))
        .collect();
    let terminal_state = match response.run.state {
        crate::ai_runtime::run_contract::RunState::Completed => EvaluationTerminalState::Completed,
        crate::ai_runtime::run_contract::RunState::Failed => EvaluationTerminalState::Failed,
        crate::ai_runtime::run_contract::RunState::Cancelled => EvaluationTerminalState::Cancelled,
        _ => return Err(EvalContractError::new("eval_run_not_terminal")),
    };
    let runtime_evidence = RuntimeEvidenceSummary {
        terminal_state,
        event_count: response.events.len().min(u32::MAX as usize) as u32,
        observed_source_kinds: observed_kinds
            .iter()
            .filter_map(|kind| match kind.as_str() {
                "local" => Some(SourceKind::Local),
                "web" => Some(SourceKind::Web),
                _ => None,
            })
            .collect(),
        tool_call_count: observation.tool_calls.len().min(u32::MAX as usize) as u32,
        degradation_observed: observation.degraded,
    };
    Ok(ExecutedCoreCase {
        summary: EvaluationCaseSummary {
            case_id: scenario.case_id(),
            evidence_group: scenario.evidence_group(),
            web_state: scenario.web_state(),
            language: scenario.language(),
            required_fact_ids,
            runtime_evidence,
            boundary,
            overall_pass: verdict.overall_pass() && boundary_pass,
            verdict,
        },
        telemetry: telemetry.snapshot(),
    })
}

#[cfg(test)]
fn install_headless_eval_mcp(state: &crate::app::AppState) -> Result<(), EvalContractError> {
    let fixture = format!(
        "{}/tests/fixtures/agent-capacity-mcp-stdio.sh",
        env!("CARGO_MANIFEST_DIR")
    );
    crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
        &state.db,
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
            id: "agent-capacity-headless-mcp".to_string(),
            name: "Agent capacity headless MCP".to_string(),
            kind: "mcp".to_string(),
            enabled: true,
            transport_kind: "stdio".to_string(),
            transport_config_json: serde_json::json!({
                "command": "/bin/sh",
                "args": [fixture, "search-only"],
            })
            .to_string(),
            credential_refs_json: "{}".to_string(),
            web_search_mapping_json: Some(r#"{"tool":"search","queryArg":"query"}"#.to_string()),
            web_fetch_mapping_json: None,
        },
    )
    .map_err(|_| EvalContractError::new("eval_mcp_setup_failed"))
}

#[cfg(test)]
fn sse_content(content: &str) -> HttpResponseScript {
    let event = serde_json::json!({
        "choices": [{
            "delta": { "content": content }
        }]
    });
    HttpResponseScript::sse(&format!("data: {event}\n\ndata: [DONE]\n\n"))
}

#[cfg(test)]
fn headless_final_content(scenario: &CoreScenario, fault: Option<EvalFault>) -> String {
    let missing_fact = match fault {
        Some(EvalFault::MissingFact { case_id }) if case_id == scenario.case_id() => scenario
            .manifest
            .required_facts
            .first()
            .map(|fact| fact.id.as_str()),
        _ => None,
    };
    let offline = scenario.web_state() == WebState::Offline;
    let mut parts = scenario
        .manifest
        .required_facts
        .iter()
        .filter(|fact| Some(fact.id.as_str()) != missing_fact)
        .filter_map(|fact| {
            let source_id = fact.allowed_sources.first()?;
            let source_kind = scenario
                .manifest
                .available_sources
                .iter()
                .find(|source| source.id == *source_id)?
                .kind;
            if offline && source_kind == SourceKind::Web {
                return None;
            }
            Some(format!("{} [cite:{}]", fact.id, source_id))
        })
        .collect::<Vec<_>>();
    for disclosure in &scenario.manifest.disclosure_constraints {
        parts.push(format!("degraded:{disclosure}"));
    }
    if parts.is_empty() {
        parts.push("synthetic bounded answer".to_string());
    }
    parts.join(" ")
}

#[cfg(test)]
fn evaluate_hard_boundary(
    scenario: &CoreScenario,
    terminal_state: crate::ai_runtime::run_contract::RunState,
    observation: &AnswerObservation,
    observed_kind_count: usize,
) -> Option<BoundaryVerdict> {
    if !scenario.is_hard_boundary() {
        return None;
    }
    let completed = terminal_state == crate::ai_runtime::run_contract::RunState::Completed;
    let used_web = observation
        .tool_calls
        .iter()
        .any(|tool| tool == "web_search");
    let has_local = observation
        .sources
        .iter()
        .any(|source| source.kind == SourceKind::Local);
    let has_web = observation
        .sources
        .iter()
        .any(|source| source.kind == SourceKind::Web);
    let (kind, status, reason_code) = match scenario.evidence_group() {
        EvidenceGroup::NoRetrieval => {
            let kind = BoundaryKind::OfflineDirectGate;
            if !completed {
                (
                    kind,
                    CheckStatus::Fail,
                    BoundaryReason::TerminalStateMismatch,
                )
            } else if used_web || has_web {
                (
                    kind,
                    CheckStatus::Fail,
                    BoundaryReason::WebDispatchObservedOffline,
                )
            } else {
                (kind, CheckStatus::Pass, BoundaryReason::Verified)
            }
        }
        EvidenceGroup::LocalOnly => {
            let kind = BoundaryKind::ExplicitLocalIsolation;
            if !completed {
                (
                    kind,
                    CheckStatus::Fail,
                    BoundaryReason::TerminalStateMismatch,
                )
            } else if !has_local || has_web || observed_kind_count != 1 {
                (
                    kind,
                    CheckStatus::Fail,
                    BoundaryReason::LocalIsolationFailed,
                )
            } else {
                (kind, CheckStatus::Pass, BoundaryReason::Verified)
            }
        }
        EvidenceGroup::WebOnly => {
            let kind = BoundaryKind::OfflineWebDegradation;
            if !completed {
                (
                    kind,
                    CheckStatus::Fail,
                    BoundaryReason::TerminalStateMismatch,
                )
            } else if used_web || has_web {
                (
                    kind,
                    CheckStatus::Fail,
                    BoundaryReason::WebDispatchObservedOffline,
                )
            } else if !observation.degraded || observation.disclosures.is_empty() {
                (kind, CheckStatus::Fail, BoundaryReason::DegradationMissing)
            } else {
                (kind, CheckStatus::Pass, BoundaryReason::Verified)
            }
        }
        EvidenceGroup::Hybrid => {
            let kind = BoundaryKind::OfflineHybridPartialEvidence;
            if !completed {
                (
                    kind,
                    CheckStatus::Fail,
                    BoundaryReason::TerminalStateMismatch,
                )
            } else if used_web || has_web {
                (
                    kind,
                    CheckStatus::Fail,
                    BoundaryReason::WebDispatchObservedOffline,
                )
            } else if !has_local || !observation.degraded || observation.disclosures.is_empty() {
                (
                    kind,
                    CheckStatus::Fail,
                    BoundaryReason::PartialEvidenceMissing,
                )
            } else {
                (kind, CheckStatus::Pass, BoundaryReason::Verified)
            }
        }
    };
    Some(BoundaryVerdict {
        kind,
        status,
        reason_code,
    })
}

#[cfg(test)]
fn aggregate_telemetry<'a>(
    summaries: impl Iterator<Item = &'a EvaluationTelemetrySummary>,
) -> EvaluationTelemetrySummary {
    let mut aggregate = EvaluationTelemetrySummary {
        model_turns: 0,
        tool_calls: 0,
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
        cache_hit_tokens: 0,
        cache_miss_tokens: 0,
        first_visible_token_ms: None,
        total_model_time_ms: 0,
        finish_reasons: FinishReasonCounts {
            stop: 0,
            tool_calls: 0,
            length: 0,
            other: 0,
        },
        truncations: TruncationCounts {
            none: 0,
            tool_result: 0,
            final_output: 0,
        },
        budgets: BudgetCounts {
            within: 0,
            model_turns: 0,
            tool_calls: 0,
            output: 0,
        },
    };
    for summary in summaries {
        aggregate.model_turns = aggregate.model_turns.saturating_add(summary.model_turns);
        aggregate.tool_calls = aggregate.tool_calls.saturating_add(summary.tool_calls);
        aggregate.prompt_tokens = aggregate
            .prompt_tokens
            .saturating_add(summary.prompt_tokens);
        aggregate.completion_tokens = aggregate
            .completion_tokens
            .saturating_add(summary.completion_tokens);
        aggregate.total_tokens = aggregate.total_tokens.saturating_add(summary.total_tokens);
        aggregate.cache_hit_tokens = aggregate
            .cache_hit_tokens
            .saturating_add(summary.cache_hit_tokens);
        aggregate.cache_miss_tokens = aggregate
            .cache_miss_tokens
            .saturating_add(summary.cache_miss_tokens);
        aggregate.first_visible_token_ms = match (
            aggregate.first_visible_token_ms,
            summary.first_visible_token_ms,
        ) {
            (Some(current), Some(next)) => Some(current.max(next)),
            (None, next) => next,
            (current, None) => current,
        };
        aggregate.total_model_time_ms = aggregate
            .total_model_time_ms
            .saturating_add(summary.total_model_time_ms);
        aggregate.finish_reasons.stop = aggregate
            .finish_reasons
            .stop
            .saturating_add(summary.finish_reasons.stop);
        aggregate.finish_reasons.tool_calls = aggregate
            .finish_reasons
            .tool_calls
            .saturating_add(summary.finish_reasons.tool_calls);
        aggregate.finish_reasons.length = aggregate
            .finish_reasons
            .length
            .saturating_add(summary.finish_reasons.length);
        aggregate.finish_reasons.other = aggregate
            .finish_reasons
            .other
            .saturating_add(summary.finish_reasons.other);
        aggregate.truncations.none = aggregate
            .truncations
            .none
            .saturating_add(summary.truncations.none);
        aggregate.truncations.tool_result = aggregate
            .truncations
            .tool_result
            .saturating_add(summary.truncations.tool_result);
        aggregate.truncations.final_output = aggregate
            .truncations
            .final_output
            .saturating_add(summary.truncations.final_output);
        aggregate.budgets.within = aggregate
            .budgets
            .within
            .saturating_add(summary.budgets.within);
        aggregate.budgets.model_turns = aggregate
            .budgets
            .model_turns
            .saturating_add(summary.budgets.model_turns);
        aggregate.budgets.tool_calls = aggregate
            .budgets
            .tool_calls
            .saturating_add(summary.budgets.tool_calls);
        aggregate.budgets.output = aggregate
            .budgets
            .output
            .saturating_add(summary.budgets.output);
    }
    aggregate
}

/// Serialize only the closed summary type; callers cannot attach arbitrary
/// metadata, raw prompts, model output, paths, URLs, evidence, or tool bodies.
pub(crate) fn serialize_evaluation_summary(
    summary: &EvaluationSummary,
) -> Result<String, EvalContractError> {
    let serialized = serde_json::to_string_pretty(summary)
        .map_err(|_| EvalContractError::new("evaluation_summary_serialization_failed"))?;
    validate_serialized_evaluation_summary(&serialized)?;
    Ok(serialized)
}

/// Recursively validates the persisted report contract. This is deliberately
/// independent of Rust's serializer so a future nested field cannot silently
/// widen the allowlist.
pub(crate) fn validate_serialized_evaluation_summary(
    serialized: &str,
) -> Result<(), EvalContractError> {
    if serialized.len() > 512 * 1024 {
        return Err(EvalContractError::new("evaluation_summary_too_large"));
    }
    let root: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|_| EvalContractError::new("evaluation_summary_invalid"))?;
    let root = exact_object(
        &root,
        &[
            "schemaVersion",
            "evidenceLevel",
            "runMode",
            "caseCount",
            "passed",
            "failed",
            "boundaryCaseCount",
            "groups",
            "languages",
            "telemetry",
            "cases",
        ],
    )?;
    exact_string(root.get("schemaVersion"), &["agent-eval-summary-v1"])?;
    exact_string(root.get("evidenceLevel"), &["headless_deterministic"])?;
    exact_string(root.get("runMode"), &["smoke", "full"])?;
    let case_count = bounded_u64(root.get("caseCount"), 48)?;
    let passed = bounded_u64(root.get("passed"), 48)?;
    let failed = bounded_u64(root.get("failed"), 48)?;
    let boundary_case_count = bounded_u64(root.get("boundaryCaseCount"), 4)?;
    if passed.saturating_add(failed) != case_count {
        return Err(EvalContractError::new(
            "evaluation_summary_count_inconsistent",
        ));
    }
    validate_group_counts(root.get("groups"), case_count)?;
    validate_language_counts(root.get("languages"), case_count)?;
    validate_telemetry_summary(root.get("telemetry"))?;

    let cases = root
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
    if cases.len() as u64 != case_count {
        return Err(EvalContractError::new(
            "evaluation_summary_count_inconsistent",
        ));
    }
    let mut case_ids = HashSet::with_capacity(cases.len());
    let mut observed_passed = 0_u64;
    let mut observed_boundaries = 0_u64;
    for case in cases {
        let (case_id, overall_pass, has_boundary) = validate_case_summary(case)?;
        if !case_ids.insert(case_id) {
            return Err(EvalContractError::new("evaluation_summary_case_duplicate"));
        }
        observed_passed = observed_passed.saturating_add(u64::from(overall_pass));
        observed_boundaries = observed_boundaries.saturating_add(u64::from(has_boundary));
    }
    if observed_passed != passed || observed_boundaries != boundary_case_count {
        return Err(EvalContractError::new(
            "evaluation_summary_count_inconsistent",
        ));
    }
    Ok(())
}

fn exact_object<'a>(
    value: &'a serde_json::Value,
    expected_keys: &[&str],
) -> Result<&'a serde_json::Map<String, serde_json::Value>, EvalContractError> {
    let object = value
        .as_object()
        .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
    if object.len() != expected_keys.len()
        || expected_keys.iter().any(|key| !object.contains_key(*key))
    {
        return Err(EvalContractError::new("evaluation_summary_unknown_field"));
    }
    Ok(object)
}

fn exact_string(
    value: Option<&serde_json::Value>,
    allowed: &[&str],
) -> Result<(), EvalContractError> {
    let value = value
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(EvalContractError::new("evaluation_summary_value_invalid"))
    }
}

fn bounded_u64(value: Option<&serde_json::Value>, maximum: u64) -> Result<u64, EvalContractError> {
    let value = value
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
    if value <= maximum {
        Ok(value)
    } else {
        Err(EvalContractError::new("evaluation_summary_value_invalid"))
    }
}

fn exact_bool(value: Option<&serde_json::Value>) -> Result<bool, EvalContractError> {
    value
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))
}

fn validate_group_counts(
    value: Option<&serde_json::Value>,
    case_count: u64,
) -> Result<(), EvalContractError> {
    let object = exact_object(
        value.ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &["noRetrieval", "localOnly", "webOnly", "hybrid"],
    )?;
    let total = ["noRetrieval", "localOnly", "webOnly", "hybrid"]
        .into_iter()
        .try_fold(0_u64, |total, key| {
            bounded_u64(object.get(key), 48).map(|count| total.saturating_add(count))
        })?;
    if total != case_count {
        return Err(EvalContractError::new(
            "evaluation_summary_count_inconsistent",
        ));
    }
    Ok(())
}

fn validate_language_counts(
    value: Option<&serde_json::Value>,
    case_count: u64,
) -> Result<(), EvalContractError> {
    let object = exact_object(
        value.ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &["chinese", "english", "mixed"],
    )?;
    let total = ["chinese", "english", "mixed"]
        .into_iter()
        .try_fold(0_u64, |total, key| {
            bounded_u64(object.get(key), 48).map(|count| total.saturating_add(count))
        })?;
    if total != case_count {
        return Err(EvalContractError::new(
            "evaluation_summary_count_inconsistent",
        ));
    }
    Ok(())
}

fn validate_telemetry_summary(value: Option<&serde_json::Value>) -> Result<(), EvalContractError> {
    let object = exact_object(
        value.ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &[
            "modelTurns",
            "toolCalls",
            "promptTokens",
            "completionTokens",
            "totalTokens",
            "cacheHitTokens",
            "cacheMissTokens",
            "firstVisibleTokenMs",
            "totalModelTimeMs",
            "finishReasons",
            "truncations",
            "budgets",
        ],
    )?;
    bounded_u64(object.get("modelTurns"), 1_000)?;
    bounded_u64(object.get("toolCalls"), 1_000)?;
    for key in [
        "promptTokens",
        "completionTokens",
        "totalTokens",
        "cacheHitTokens",
        "cacheMissTokens",
    ] {
        bounded_u64(object.get(key), 1_000_000_000)?;
    }
    match object.get("firstVisibleTokenMs") {
        Some(serde_json::Value::Null) => {}
        value => {
            bounded_u64(value, 86_400_000)?;
        }
    }
    bounded_u64(object.get("totalModelTimeMs"), 604_800_000)?;
    validate_counter_object(
        object.get("finishReasons"),
        &["stop", "toolCalls", "length", "other"],
    )?;
    validate_counter_object(
        object.get("truncations"),
        &["none", "toolResult", "finalOutput"],
    )?;
    validate_counter_object(
        object.get("budgets"),
        &["within", "modelTurns", "toolCalls", "output"],
    )
}

fn validate_counter_object(
    value: Option<&serde_json::Value>,
    keys: &[&str],
) -> Result<(), EvalContractError> {
    let object = exact_object(
        value.ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        keys,
    )?;
    for key in keys {
        bounded_u64(object.get(*key), 1_000)?;
    }
    Ok(())
}

fn validate_case_summary(
    value: &serde_json::Value,
) -> Result<(u64, bool, bool), EvalContractError> {
    let object = exact_object(
        value,
        &[
            "caseId",
            "evidenceGroup",
            "webState",
            "language",
            "requiredFactIds",
            "runtimeEvidence",
            "boundary",
            "verdict",
            "overallPass",
        ],
    )?;
    let case_id = bounded_u64(object.get("caseId"), 48)?;
    if case_id == 0 {
        return Err(EvalContractError::new("evaluation_summary_value_invalid"));
    }
    exact_string(
        object.get("evidenceGroup"),
        &["no_retrieval", "local_only", "web_only", "hybrid"],
    )?;
    exact_string(object.get("webState"), &["offline", "online"])?;
    exact_string(object.get("language"), &["chinese", "english", "mixed"])?;
    validate_fact_ids(object.get("requiredFactIds"))?;
    validate_runtime_evidence(object.get("runtimeEvidence"))?;
    let (has_boundary, boundary_pass) = match object.get("boundary") {
        Some(serde_json::Value::Null) => (false, true),
        Some(boundary) => {
            let passed = validate_boundary(boundary)?;
            (true, passed)
        }
        None => return Err(EvalContractError::new("evaluation_summary_shape_invalid")),
    };
    let verdict_pass = validate_evaluation_verdict(
        object
            .get("verdict")
            .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        case_id,
    )?;
    let overall_pass = exact_bool(object.get("overallPass"))?;
    if overall_pass != (verdict_pass && boundary_pass) {
        return Err(EvalContractError::new(
            "evaluation_summary_verdict_inconsistent",
        ));
    }
    Ok((case_id, overall_pass, has_boundary))
}

fn validate_runtime_evidence(value: Option<&serde_json::Value>) -> Result<(), EvalContractError> {
    let object = exact_object(
        value.ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &[
            "terminalState",
            "eventCount",
            "observedSourceKinds",
            "toolCallCount",
            "degradationObserved",
        ],
    )?;
    exact_string(
        object.get("terminalState"),
        &["completed", "failed", "cancelled"],
    )?;
    bounded_u64(object.get("eventCount"), 10_000)?;
    bounded_u64(object.get("toolCallCount"), 1_000)?;
    exact_bool(object.get("degradationObserved"))?;
    let source_kinds = object
        .get("observedSourceKinds")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
    if source_kinds.len() > 2 {
        return Err(EvalContractError::new("evaluation_summary_value_invalid"));
    }
    let mut observed = HashSet::with_capacity(source_kinds.len());
    for source_kind in source_kinds {
        exact_string(Some(source_kind), &["local", "web"])?;
        let source_kind = source_kind
            .as_str()
            .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
        if !observed.insert(source_kind) {
            return Err(EvalContractError::new("evaluation_summary_value_invalid"));
        }
    }
    Ok(())
}

fn validate_fact_ids(value: Option<&serde_json::Value>) -> Result<(), EvalContractError> {
    let values = value
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
    if values.len() > 16 {
        return Err(EvalContractError::new("evaluation_summary_value_invalid"));
    }
    let mut observed = HashSet::with_capacity(values.len());
    for value in values {
        let value = value
            .as_str()
            .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
        if value.len() > 64 || !value.starts_with("fact-") || !safe_label(value) {
            return Err(EvalContractError::new("evaluation_summary_value_invalid"));
        }
        if !observed.insert(value) {
            return Err(EvalContractError::new("evaluation_summary_value_invalid"));
        }
    }
    Ok(())
}

fn validate_boundary(value: &serde_json::Value) -> Result<bool, EvalContractError> {
    let object = exact_object(value, &["kind", "status", "reasonCode"])?;
    exact_string(
        object.get("kind"),
        &[
            "offline_direct_gate",
            "explicit_local_isolation",
            "offline_web_degradation",
            "offline_hybrid_partial_evidence",
        ],
    )?;
    exact_string(object.get("status"), &["pass", "fail"])?;
    exact_string(
        object.get("reasonCode"),
        &[
            "verified",
            "terminal_state_mismatch",
            "web_dispatch_observed_offline",
            "local_isolation_failed",
            "degradation_missing",
            "partial_evidence_missing",
        ],
    )?;
    let passed = object.get("status").and_then(serde_json::Value::as_str) == Some("pass");
    let verified = object.get("reasonCode").and_then(serde_json::Value::as_str) == Some("verified");
    if passed != verified {
        return Err(EvalContractError::new(
            "evaluation_summary_verdict_inconsistent",
        ));
    }
    Ok(passed)
}

fn validate_evaluation_verdict(
    value: &serde_json::Value,
    expected_case_id: u64,
) -> Result<bool, EvalContractError> {
    let object = exact_object(
        value,
        &[
            "caseId",
            "authorization",
            "requiredEvidence",
            "factCorrectness",
            "citationSupport",
            "routeEfficiency",
            "degradationOrClarification",
            "safety",
            "overallPass",
        ],
    )?;
    if bounded_u64(object.get("caseId"), 48)? != expected_case_id {
        return Err(EvalContractError::new(
            "evaluation_summary_verdict_inconsistent",
        ));
    }
    for key in [
        "authorization",
        "requiredEvidence",
        "factCorrectness",
        "citationSupport",
        "routeEfficiency",
        "degradationOrClarification",
        "safety",
    ] {
        validate_check_verdict(
            object
                .get(key)
                .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        )?;
    }
    exact_bool(object.get("overallPass"))
}

fn validate_check_verdict(value: &serde_json::Value) -> Result<(), EvalContractError> {
    let object = exact_object(value, &["status", "reasonCode"])?;
    exact_string(object.get("status"), &["pass", "fail", "not_applicable"])?;
    exact_string(
        object.get("reasonCode"),
        &[
            "authorization_satisfied",
            "offline_web_dispatch",
            "unauthorized_local_access",
            "offline_degradation_disclosed",
            "offline_degradation_missing",
            "no_disclosure_required",
            "required_disclosure_present",
            "required_disclosure_missing",
            "required_source_missing",
            "required_sources_satisfied",
            "required_fact_contradicted",
            "required_fact_missing",
            "required_facts_satisfied",
            "required_citation_missing_or_unsupported",
            "citation_support_satisfied",
            "citation_not_required",
            "required_web_search_missing",
            "forbidden_web_search",
            "unnecessary_web_search",
            "unnecessary_local_search",
            "route_efficient",
            "web_answer_contaminated",
            "safety_or_tool_policy_violation",
            "safety_satisfied",
        ],
    )
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
    FailureClassifiedOnly,
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
        ProtocolValidationLevel::FailureClassifiedOnly
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

/// A contract level earned only after a real MCP discovery response has been
/// received through Iris' transport boundary. Mapping JSON and a manually
/// deserialized discovery response cannot build this value.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct McpTransportContract {
    validation_level: ProtocolValidationLevel,
    _proof: crate::ai_runtime::mcp_host_runtime::McpStdioTransportProof,
}

#[cfg(test)]
impl McpTransportContract {
    /// Reject bare discovery data, including data produced through serde. A
    /// successful contract must consume an attested transport probe instead.
    pub(crate) fn verify_discovery(
        _mapping: &McpCapabilityContract,
        _discovery: &crate::ai_runtime::mcp_host_runtime::McpStdioDiscovery,
    ) -> Result<Self, EvalContractError> {
        Err(EvalContractError::new("mcp_transport_provenance_required"))
    }

    pub(crate) fn verify_attested_probe(
        mapping: &McpCapabilityContract,
        probe: crate::ai_runtime::mcp_host_runtime::McpStdioTransportProbe,
    ) -> Result<Self, EvalContractError> {
        let (discovery, proof) = probe
            .into_discovery()
            .map_err(|_| EvalContractError::new("mcp_transport_discovery_invalid"))?;
        if discovery.protocol_version != crate::ai_runtime::mcp_host_runtime::MCP_PROTOCOL_VERSION
            || !safe_label(&discovery.server_name)
        {
            return Err(EvalContractError::new("mcp_transport_discovery_invalid"));
        }
        let tools = discovery
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<HashSet<_>>();
        if !tools.contains("search")
            || (mapping.supports(McpOperation::Fetch) && !tools.contains("fetch"))
        {
            return Err(EvalContractError::new("mcp_transport_mapping_mismatch"));
        }
        Ok(Self {
            validation_level: ProtocolValidationLevel::ContractVerified,
            _proof: proof,
        })
    }

    pub(crate) const fn validation_level(&self) -> ProtocolValidationLevel {
        self.validation_level
    }
}

#[cfg(test)]
impl<'de> Deserialize<'de> for McpTransportContract {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Err(serde::de::Error::custom(
            "mcp_transport_provenance_required",
        ))
    }
}

/// A real stdio transport failure, classified only after an attested probe.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct McpTransportFailureContract {
    outcome: ProtocolContractOutcome,
    validation_level: ProtocolValidationLevel,
    _proof: Option<crate::ai_runtime::mcp_host_runtime::McpStdioTransportProof>,
}

#[cfg(test)]
impl McpTransportFailureContract {
    pub(crate) fn from_probe(
        probe: crate::ai_runtime::mcp_host_runtime::McpStdioTransportProbe,
    ) -> Result<Self, EvalContractError> {
        let (failure, proof) = probe
            .into_failure()
            .map_err(|_| EvalContractError::new("mcp_transport_failure_expected"))?;
        Ok(Self {
            outcome: ProtocolContractOutcome::from_mcp_runtime_failure(failure),
            validation_level: if proof.is_some() {
                ProtocolValidationLevel::ContractVerified
            } else {
                ProtocolValidationLevel::FailureClassifiedOnly
            },
            _proof: proof,
        })
    }

    pub(crate) const fn outcome(&self) -> ProtocolContractOutcome {
        self.outcome
    }

    pub(crate) const fn validation_level(&self) -> ProtocolValidationLevel {
        self.validation_level
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
