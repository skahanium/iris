//! contract — part of the evaluator contract split out of `agent_capacity_eval.rs`.
//!
//! Moved verbatim; the parent re-exports it so existing paths keep working.

use super::*;

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

/// Disclosure token required when Online Web evidence is unavailable but the Run
/// continues with a constrained answer.
pub(crate) const ONLINE_WEB_DEGRADATION_DISCLOSURE: &str = "web-online-degradation";

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

pub(super) fn safe_label(value: &str) -> bool {
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
pub(super) struct CaseOrdinal(pub(super) u32);

pub(super) fn parse_case_ordinal(value: &str) -> Result<CaseOrdinal, EvalContractError> {
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
    pub(crate) const fn new(reason_code: &'static str) -> Self {
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
}

/// Closed outcome for the local-material boundary immediately before an
/// external Web request. This is deliberately separate from the answer-level
/// evidence check: a blocked request never left the device and therefore is
/// not an answer contamination event, but it is still a model-policy failure
/// for calibration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WebQueryBoundary {
    NotApplicable,
    ConfirmedClean,
    BlockedLocalMaterial,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SafetyViolation {
    UnauthorizedLocalRead,
    UnsupportedTool,
    LocalMaterialWebQueryBlocked,
    LocalMaterialWebQueryUnverified,
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
    OnlineDegradationDisclosed,
    OnlineDegradationFabrication,
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
    LocalMaterialWebQueryBlocked,
    LocalMaterialWebQueryUnverified,
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
            Self::OnlineDegradationDisclosed => "online_degradation_disclosed",
            Self::OnlineDegradationFabrication => "online_degradation_fabrication",
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
            Self::LocalMaterialWebQueryBlocked => "local_material_web_query_blocked",
            Self::LocalMaterialWebQueryUnverified => "local_material_web_query_unverified",
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
    pub(super) fn pass(reason_code: VerdictReason) -> Self {
        Self {
            status: CheckStatus::Pass,
            reason_code,
        }
    }

    pub(super) fn fail(reason_code: VerdictReason) -> Self {
        Self {
            status: CheckStatus::Fail,
            reason_code,
        }
    }

    pub(super) fn not_applicable(reason_code: VerdictReason) -> Self {
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
pub(super) struct ValidatedCaseId(pub(super) CaseOrdinal);

/// Stable, raw-content-free verdict consumed by reports and CI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EvaluationVerdict {
    pub(super) case_id: ValidatedCaseId,
    pub(super) authorization: CheckVerdict,
    pub(super) required_evidence: CheckVerdict,
    pub(super) fact_correctness: CheckVerdict,
    pub(super) citation_support: CheckVerdict,
    pub(super) route_efficiency: CheckVerdict,
    pub(super) degradation_or_clarification: CheckVerdict,
    pub(super) safety: CheckVerdict,
    pub(super) overall_pass: bool,
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
