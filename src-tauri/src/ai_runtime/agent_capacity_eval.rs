//! Versioned, provider-neutral contracts for Agent answer-capacity evaluation.
//!
//! This module deliberately stores only stable synthetic identifiers and
//! bounded verdict codes. Raw prompts, model answers, note paths, source URLs,
//! provider payloads, and credentials are not part of any serializable type.

use contract::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use telemetry::*;
use tool_class::evaluation_local_read_tool_names;
#[path = "agent_capacity_eval/baseline_identity.rs"]
mod baseline_identity;
#[path = "agent_capacity_eval/scenario.rs"]
mod scenario;
#[cfg(not(test))]
#[allow(
    unused_imports,
    reason = "scenario plan table is consumed by the evaluation entrypoints"
)]
pub(crate) use scenario::*;
#[cfg(test)]
pub(crate) use scenario::*;
#[path = "agent_capacity_eval/verdict.rs"]
mod verdict;
pub(crate) use baseline_identity::{
    BaselineIdentity, CAPACITY_REPORT_SCHEMA_V3, EVAL_SUMMARY_SCHEMA_V3,
};
#[cfg(test)]
pub(crate) use baseline_identity::{WorkingTree, AGENT_ANSWER_V1_FIXTURE};
#[cfg(test)]
pub(crate) use contract::{
    AnswerObservation, CaseManifest, CheckStatus, CitationExpectation, CitationObservation,
    EvidenceGroup, FactSupportObservation, ImplicitVaultExpectation, ObservedSource, RequiredFact,
    RequiredSource, SafetyViolation, SourceKind, VerdictReason, WebAnswerContamination,
    WebQueryBoundary, WebSearchPolicy, WebState, ONLINE_WEB_DEGRADATION_DISCLOSURE,
};
#[cfg(test)]
pub(crate) use telemetry::EvalRunMode;
pub(crate) use telemetry::EvaluationTelemetryTap;
#[cfg(test)]
pub(crate) use tool_class::{
    observed_eval_tool_class, permission_denial_category, runtime_capability_to_eval_tool_name,
    summarize_web_query_boundary,
};
pub(crate) use verdict::*;
#[cfg(test)]
pub(crate) use verdict::{aggregate_capacity_scorecard, evaluate_case, measure_case_quality};
// ─── child modules (split of the former single-file evaluator) ───
#[path = "agent_capacity_eval/contract.rs"]
mod contract;
#[path = "agent_capacity_eval/telemetry.rs"]
mod telemetry;
#[path = "agent_capacity_eval/tool_class.rs"]
mod tool_class;

#[cfg(test)]
#[path = "agent_capacity_eval/test_support.rs"]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::*;

/// One independently varied pressure axis. The deterministic suite proves the
/// Iris runtime boundary only; it never promotes those observations to a live
/// model capability claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PressureDimension {
    Input,
    History,
    ConversationTurns,
    LocalMaterial,
    LocalMaterialChars,
    RetrievalDistractors,
    IndexScale,
    VectorAvailability,
    ReasoningDepth,
    ToolLoop,
    WebEvidenceCount,
    WebLatency,
    Output,
    CombinedTerminal,
}

/// A geometric schedule with focused levels adjacent to a known production
/// boundary. Values are abstract load units documented by the dimension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PressureStaircase {
    dimension: PressureDimension,
    levels: Vec<u32>,
}

impl PressureStaircase {
    pub(crate) const fn dimension(&self) -> PressureDimension {
        self.dimension
    }

    pub(crate) fn levels(&self) -> &[u32] {
        &self.levels
    }
}

/// Build the fixed scheduling contract. This function schedules work; it does
/// not assert that any level passed until an execution probe supplies evidence.
pub(crate) fn generate_pressure_staircases() -> Result<Vec<PressureStaircase>, EvalContractError> {
    let staircases = vec![
        PressureStaircase {
            dimension: PressureDimension::Input,
            levels: vec![1_000, 4_000, 8_000, 12_000, 15_500, 16_000, 16_001],
        },
        PressureStaircase {
            dimension: PressureDimension::History,
            levels: vec![1, 6, 7, 8, 20, 50],
        },
        PressureStaircase {
            dimension: PressureDimension::ConversationTurns,
            levels: vec![1, 20, 50, 100],
        },
        PressureStaircase {
            dimension: PressureDimension::LocalMaterial,
            levels: vec![1, 2, 4, 8, 11, 12, 13],
        },
        PressureStaircase {
            dimension: PressureDimension::LocalMaterialChars,
            levels: vec![8_000, 16_000, 24_000, 32_000, 32_001],
        },
        PressureStaircase {
            dimension: PressureDimension::RetrievalDistractors,
            levels: vec![0, 10, 48, 100, 1_000],
        },
        PressureStaircase {
            dimension: PressureDimension::IndexScale,
            levels: vec![48, 1_000, 10_000, 50_000],
        },
        PressureStaircase {
            dimension: PressureDimension::VectorAvailability,
            // 0=available, 1=rebuilding, 2=unavailable
            levels: vec![0, 1, 2],
        },
        PressureStaircase {
            dimension: PressureDimension::ReasoningDepth,
            levels: vec![1, 2, 4, 6, 7, 8, 9],
        },
        PressureStaircase {
            dimension: PressureDimension::ToolLoop,
            levels: vec![1, 2, 4, 8, 16, 24, 25],
        },
        PressureStaircase {
            dimension: PressureDimension::WebEvidenceCount,
            // This axis measures the one deterministic strict-Web prefetch,
            // which is capped at eight provider results. The separate Run Web
            // reservation tests retain the twelve-row aggregate budget and
            // the hard-boundary suite keeps proving that thirteen is blocked.
            levels: vec![1, 2, 4, 8, 9, 12, 13],
        },
        PressureStaircase {
            dimension: PressureDimension::WebLatency,
            levels: vec![0, 3, 9, 11],
        },
        PressureStaircase {
            dimension: PressureDimension::Output,
            levels: vec![1_000, 2_000, 4_000, 8_000, 16_000, 32_000, 32_001],
        },
        // The six values identify six predefined cross-axis terminal cases,
        // rather than pretending a combined load has one scalar unit.
        PressureStaircase {
            dimension: PressureDimension::CombinedTerminal,
            levels: vec![1, 2, 3, 4, 5, 6],
        },
    ];
    if staircases.iter().any(|staircase| {
        staircase.levels.is_empty() || staircase.levels.windows(2).any(|pair| pair[0] >= pair[1])
    }) {
        return Err(EvalContractError::new("pressure_staircase_invalid"));
    }
    Ok(staircases)
}

/// Five repeated observations at one pressure level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StableLevelObservation {
    level: u32,
    passes: [bool; 5],
}

impl StableLevelObservation {
    pub(crate) const fn new(level: u32, passes: [bool; 5]) -> Self {
        Self { level, passes }
    }

    fn pass_count(&self) -> usize {
        self.passes.iter().filter(|passed| **passed).count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StableBoundary {
    stable_level: u32,
    next_level: u32,
}

impl StableBoundary {
    pub(crate) const fn stable_level(self) -> u32 {
        self.stable_level
    }

    pub(crate) const fn next_level(self) -> u32 {
        self.next_level
    }
}

/// Return the highest adjacent pair meeting the predeclared stability rule:
/// at least four of five passes now and no more than two of five one level up.
pub(crate) fn calculate_stable_boundary(
    observations: &[StableLevelObservation],
) -> Result<StableBoundary, EvalContractError> {
    if observations.len() < 2
        || observations
            .windows(2)
            .any(|pair| pair[0].level >= pair[1].level)
    {
        return Err(EvalContractError::new(
            "stable_boundary_observations_invalid",
        ));
    }
    observations
        .windows(2)
        .rev()
        .find(|pair| pair[0].pass_count() >= 4 && pair[1].pass_count() <= 2)
        .map(|pair| StableBoundary {
            stable_level: pair[0].level,
            next_level: pair[1].level,
        })
        .ok_or_else(|| EvalContractError::new("stable_boundary_not_observed"))
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

/// Truncation observed for one executed case.
///
/// Only the truncation that actually happens is recorded here. The "nothing was
/// truncated" counter is written by `record_final_output_validation`, which
/// already owns the accepted-versus-rejected decision, so there is no `None`
/// variant to pass in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TruncationOutcome {
    ToolResultTruncated,
}

/// Budget outcome observed for one executed case.
///
/// As with [`TruncationOutcome`], the normal "stayed within budget" counter is
/// written by `record_final_output_validation` rather than passed in here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BudgetOutcome {
    ModelTurnsExhausted,
    ToolCallsExhausted,
    OutputBudgetReached,
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
    quality_atoms: CaseQualityAtoms,
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

/// Whether an online Web transport failure produced no user-visible claim to assess.
///
/// These samples stay non-passing because they did not exercise the answer path, but
/// they must not be counted as fabricated facts or attribution violations.
pub(crate) fn no_answer_external_terminal_failure(
    terminal_failed: bool,
    terminal_error_code: Option<&str>,
    web_state: WebState,
    requires_web: bool,
    answer_is_empty: bool,
    sources_are_empty: bool,
    safety_violations_are_empty: bool,
) -> bool {
    terminal_failed
        && web_state == WebState::Online
        && requires_web
        && answer_is_empty
        && sources_are_empty
        && safety_violations_are_empty
        && matches!(
            terminal_error_code,
            Some(
                "agent_run_provider_unavailable"
                    | "agent_run_provider_timeout"
                    | "agent_run_web_provider_unavailable"
                    | "agent_run_web_provider_timeout"
                    | "agent_run_web_provider_failed"
                    | "agent_run_web_evidence_invalid"
            )
        )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeEvidenceSummary {
    terminal_state: EvaluationTerminalState,
    terminal_error_code: Option<&'static str>,
    event_count: u32,
    observed_source_kinds: Vec<SourceKind>,
    tool_call_count: u32,
    degradation_observed: bool,
    /// Content-free result of the pre-dispatch local-material Web boundary.
    /// It records no query or material text, only whether the boundary had a
    /// clean witness, blocked an attempt, or could not be verified.
    web_query_boundary: WebQueryBoundary,
    observed_tool_classes: Vec<ObservedEvalToolClass>,
    /// Closed diagnostic categories for an execution-time permission denial.
    /// These are intentionally broader than tool names: evaluation reports
    /// must help find a surface/gate mismatch without retaining a model's
    /// raw tool label or any call arguments.
    permission_denial_categories: Vec<PermissionDenialCategory>,
}

/// Closed, content-free view of a model-observed tool. This makes the matrix
/// diagnose a surface mismatch without persisting a tool label or arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ObservedEvalToolClass {
    LocalRead,
    RuntimeContext,
    WebSearch,
    ExternalRead,
    OtherCatalogTool,
    UnknownTool,
}

impl ObservedEvalToolClass {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::LocalRead => "local_read",
            Self::RuntimeContext => "runtime_context",
            Self::WebSearch => "web_search",
            Self::ExternalRead => "external_read",
            Self::OtherCatalogTool => "other_catalog_tool",
            Self::UnknownTool => "unknown_tool",
        }
    }
}

/// Privacy-safe classification for a denied tool that reached the execution
/// gate. It distinguishes an actual local-read boundary from a model/tool
/// surface mismatch while keeping the report free of provider labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PermissionDenialCategory {
    LocalRead,
    RuntimeContext,
    WebSearch,
    OtherCatalogTool,
    UnknownTool,
}

impl PermissionDenialCategory {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::LocalRead => "local_read",
            Self::RuntimeContext => "runtime_context",
            Self::WebSearch => "web_search",
            Self::OtherCatalogTool => "other_catalog_tool",
            Self::UnknownTool => "unknown_tool",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BoundaryKind {
    OfflineDirectGate,
    ExplicitLocalIsolation,
    OfflineWebDegradation,
    OfflineHybridPartialEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BoundaryReason {
    Verified,
    TerminalStateMismatch,
    WebDispatchObservedOffline,
    LocalIsolationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BoundaryVerdict {
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
    executed_case_count: u32,
    completed_case_count: u32,
    answered_case_count: u32,
    expected_refusal_count: u32,
    unexpected_failure_count: u32,
    passed: u32,
    failed: u32,
    boundary_case_count: u32,
    groups: GroupCounts,
    languages: LanguageCounts,
    telemetry: EvaluationTelemetrySummary,
    scorecard: CapacityScorecard,
    cases: Vec<EvaluationCaseSummary>,
    baseline_identity: BaselineIdentity,
}

impl EvaluationSummary {
    pub(crate) const fn case_count(&self) -> u32 {
        self.case_count
    }

    pub(crate) const fn completed_case_count(&self) -> u32 {
        self.completed_case_count
    }

    pub(crate) const fn passed(&self) -> u32 {
        self.passed
    }

    pub(crate) const fn boundary_case_count(&self) -> u32 {
        self.boundary_case_count
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

    pub(crate) const fn baseline_identity(&self) -> &BaselineIdentity {
        &self.baseline_identity
    }
}

/// Select the fixed core subset. Selection alone makes no capability claim.
pub(crate) fn select_core_scenarios(
    mode: EvalRunMode,
) -> Result<Vec<CoreScenario>, EvalContractError> {
    let scenarios = generate_core_scenarios()?;
    Ok(match mode {
        EvalRunMode::Full => scenarios,
        // The release smoke is the complete online interaction matrix. It
        // cannot turn an incomplete sample into a release signal; offline and
        // hard-boundary coverage remains in the security track.
        EvalRunMode::Smoke => scenarios
            .into_iter()
            .filter(|scenario| scenario.web_state() == WebState::Online)
            .collect(),
    })
}

#[allow(
    dead_code,
    reason = "fault-injection surface: every variant has a handled branch, but not every fault is injected by the current case table"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvalFault {
    MissingFact { case_id: u32 },
    WrongFact { case_id: u32 },
    MissingCitation { case_id: u32 },
    OfflineWebDispatch { case_id: u32 },
    UnauthorizedLocalRead { case_id: u32 },
    UnauthorizedLocalScope { case_id: u32 },
    LocalToWebDisclosure { case_id: u32 },
    OnlineWebDegradation { case_id: u32 },
    OnlineWebDegradationFabrication { case_id: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LiveCapabilityCombination {
    layer: &'static str,
    paired_with: &'static str,
    status: &'static str,
}

impl LiveCapabilityCombination {
    pub(crate) const fn layer(&self) -> &'static str {
        self.layer
    }

    pub(crate) const fn status(&self) -> &'static str {
        self.status
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LiveCapabilityMatrix {
    combinations: Vec<LiveCapabilityCombination>,
}

impl LiveCapabilityMatrix {
    pub(crate) fn combinations(&self) -> &[LiveCapabilityCombination] {
        &self.combinations
    }
}

/// Pairwise capability sampling plan. Missing live profiles stay `live_not_tested`
/// while protocol doubles remain `contract_verified`.
pub(crate) fn pairwise_live_capability_matrix(
    available_layers: &[&str],
) -> Result<LiveCapabilityMatrix, EvalContractError> {
    const LAYERS: &[&str] = &[
        "openai_compatible_chat",
        "anthropic_messages",
        "openai_responses",
        "compatible_vendor",
        "mcp_search_only",
        "mcp_search_fetch",
        "mcp_stdio",
    ];
    let available = available_layers.iter().copied().collect::<HashSet<_>>();
    let mut combinations = Vec::new();
    for (index, layer) in LAYERS.iter().enumerate() {
        let paired_with = LAYERS[(index + 1) % LAYERS.len()];
        let status = if available.contains(layer) {
            "live_not_tested"
        } else if matches!(
            *layer,
            "openai_compatible_chat"
                | "anthropic_messages"
                | "openai_responses"
                | "mcp_search_only"
                | "mcp_search_fetch"
                | "mcp_stdio"
        ) {
            "contract_verified"
        } else {
            "live_not_tested"
        };
        combinations.push(LiveCapabilityCombination {
            layer,
            paired_with,
            status,
        });
    }
    Ok(LiveCapabilityMatrix { combinations })
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
            "executedCaseCount",
            "completedCaseCount",
            "answeredCaseCount",
            "expectedRefusalCount",
            "unexpectedFailureCount",
            "passed",
            "failed",
            "boundaryCaseCount",
            "groups",
            "languages",
            "telemetry",
            "scorecard",
            "cases",
            "baselineIdentity",
        ],
    )?;
    exact_string(root.get("schemaVersion"), &[EVAL_SUMMARY_SCHEMA_V3])?;
    exact_string(root.get("evidenceLevel"), &["headless_deterministic"])?;
    exact_string(root.get("runMode"), &["smoke", "full"])?;
    let case_count = bounded_u64(root.get("caseCount"), summary_count_bound())?;
    let executed_case_count = bounded_u64(root.get("executedCaseCount"), summary_count_bound())?;
    let completed_case_count = bounded_u64(root.get("completedCaseCount"), summary_count_bound())?;
    let answered_case_count = bounded_u64(root.get("answeredCaseCount"), summary_count_bound())?;
    let expected_refusal_count =
        bounded_u64(root.get("expectedRefusalCount"), summary_count_bound())?;
    let unexpected_failure_count =
        bounded_u64(root.get("unexpectedFailureCount"), summary_count_bound())?;
    let passed = bounded_u64(root.get("passed"), summary_count_bound())?;
    let failed = bounded_u64(root.get("failed"), summary_count_bound())?;
    let boundary_case_count = bounded_u64(root.get("boundaryCaseCount"), 4)?;
    let run_mode = root
        .get("runMode")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
    if executed_case_count != case_count
        || completed_case_count != answered_case_count
        || answered_case_count
            .saturating_add(expected_refusal_count)
            .saturating_add(unexpected_failure_count)
            != case_count
        || passed != answered_case_count.saturating_add(expected_refusal_count)
        || failed != unexpected_failure_count
        || (run_mode == "smoke" && unexpected_failure_count != 0)
    {
        return Err(EvalContractError::new(
            "evaluation_summary_count_inconsistent",
        ));
    }
    validate_group_counts(root.get("groups"), case_count)?;
    validate_language_counts(root.get("languages"), case_count)?;
    validate_telemetry_summary(root.get("telemetry"))?;
    validate_capacity_scorecard(root.get("scorecard"))?;
    validate_baseline_identity(root.get("baselineIdentity"))?;

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
    let mut observed_answered = 0_u64;
    let mut observed_expected_refusals = 0_u64;
    let mut observed_boundaries = 0_u64;
    for case in cases {
        let (case_id, overall_pass, has_boundary) = validate_case_summary(case)?;
        if !case_ids.insert(case_id) {
            return Err(EvalContractError::new("evaluation_summary_case_duplicate"));
        }
        observed_answered = observed_answered.saturating_add(u64::from(overall_pass));
        observed_expected_refusals = observed_expected_refusals
            .saturating_add(u64::from(serialized_case_is_expected_refusal(case)));
        observed_boundaries = observed_boundaries.saturating_add(u64::from(has_boundary));
    }
    if observed_answered != answered_case_count
        || observed_expected_refusals != expected_refusal_count
        || observed_boundaries != boundary_case_count
    {
        return Err(EvalContractError::new(
            "evaluation_summary_count_inconsistent",
        ));
    }
    Ok(())
}

fn serialized_case_is_expected_refusal(case: &serde_json::Value) -> bool {
    case.get("webState").and_then(serde_json::Value::as_str) == Some("offline")
        && matches!(
            case.get("evidenceGroup")
                .and_then(serde_json::Value::as_str),
            Some("web_only" | "hybrid")
        )
        && case
            .pointer("/runtimeEvidence/terminalState")
            .and_then(serde_json::Value::as_str)
            == Some("failed")
        && case
            .pointer("/runtimeEvidence/terminalErrorCode")
            .and_then(serde_json::Value::as_str)
            == Some("agent_run_web_verification_required")
        && case
            .pointer("/verdict/safety/status")
            .and_then(serde_json::Value::as_str)
            == Some("pass")
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

fn exact_hex(value: Option<&serde_json::Value>, length: usize) -> Result<(), EvalContractError> {
    let value = value
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
    if value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(EvalContractError::new("evaluation_summary_value_invalid"))
    }
}

fn validate_baseline_identity(value: Option<&serde_json::Value>) -> Result<(), EvalContractError> {
    let object = exact_object(
        value.ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &[
            "sourceCommit",
            "workingTree",
            "comparable",
            "release",
            "scoringSchema",
            "scenarioSetHash",
            "fixtureHashes",
            "os",
            "arch",
        ],
    )?;
    exact_hex(object.get("sourceCommit"), 40)?;
    exact_hex(object.get("scenarioSetHash"), 64)?;
    exact_string(object.get("release"), &[env!("CARGO_PKG_VERSION")])?;
    exact_string(object.get("scoringSchema"), &[CAPACITY_REPORT_SCHEMA_V3])?;
    let working_tree = object
        .get("workingTree")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
    if working_tree != "clean" && working_tree != "dirty" {
        return Err(EvalContractError::new("evaluation_summary_value_invalid"));
    }
    let comparable = exact_bool(object.get("comparable"))?;
    if comparable != (working_tree == "clean") {
        return Err(EvalContractError::new("evaluation_summary_value_invalid"));
    }
    let fixtures = exact_object(
        object
            .get("fixtureHashes")
            .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &["agentAnswerV1"],
    )?;
    exact_hex(fixtures.get("agentAnswerV1"), 64)?;
    for key in ["os", "arch"] {
        let token = object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
        if token.is_empty() || token.contains('/') || token.contains('\\') || token.contains("..") {
            return Err(EvalContractError::new("evaluation_summary_value_invalid"));
        }
    }
    Ok(())
}

/// Upper bound for every per-case and per-summary count in the serialized
/// summary.
///
/// Derived from the declared matrix instead of a literal: the bound exists to
/// reject unbounded values, not to freeze the number of cases. A hard-coded 48
/// silently rejected the 49th case, which reads as a contract violation rather
/// than as the stale bound it is.
fn summary_count_bound() -> u64 {
    u64::try_from(BASE_QUESTION_PLANS.len() * 2).unwrap_or(u64::MAX)
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

fn optional_quality_bps(
    quality: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<u64>, EvalContractError> {
    match quality.get(key) {
        Some(serde_json::Value::Null) => Ok(None),
        Some(value) => bounded_u64(Some(value), 10_000).map(Some),
        None => Err(EvalContractError::new("evaluation_summary_shape_invalid")),
    }
}

fn validate_quality_ratio(
    quality: &serde_json::Map<String, serde_json::Value>,
    bps_key: &str,
    numerator_key: &str,
    denominator_key: &str,
    gate_key: Option<&str>,
    threshold: u64,
) -> Result<(), EvalContractError> {
    let bps = optional_quality_bps(quality, bps_key)?;
    bounded_u64(quality.get(numerator_key), 1_000_000)?;
    let denominator = bounded_u64(quality.get(denominator_key), 1_000_000)?;
    if denominator == 0 {
        if bps.is_some() {
            return Err(EvalContractError::new("evaluation_summary_value_invalid"));
        }
        if let Some(gate_key) = gate_key {
            if quality.get(gate_key) != Some(&serde_json::Value::Bool(false)) {
                return Err(EvalContractError::new("evaluation_summary_value_invalid"));
            }
        }
        return Ok(());
    }
    let bps = bps.ok_or_else(|| EvalContractError::new("evaluation_summary_value_invalid"))?;
    if let Some(gate_key) = gate_key {
        let gate = exact_bool(quality.get(gate_key))?;
        if gate != (bps >= threshold) {
            return Err(EvalContractError::new("evaluation_summary_value_invalid"));
        }
    }
    Ok(())
}

fn validate_quality_f1(
    quality: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), EvalContractError> {
    optional_quality_bps(quality, "factF1Bps")?;
    bounded_u64(quality.get("factF1Numerator"), 1_000_000)?;
    if bounded_u64(quality.get("factF1Denominator"), 1_000_000)? != 0 {
        return Err(EvalContractError::new("evaluation_summary_value_invalid"));
    }
    Ok(())
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
            bounded_u64(object.get(key), summary_count_bound())
                .map(|count| total.saturating_add(count))
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
            bounded_u64(object.get(key), summary_count_bound())
                .map(|count| total.saturating_add(count))
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
            "webToolCalls",
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
    bounded_u64(object.get("webToolCalls"), 1_000)?;
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
            "qualityAtoms",
            "overallPass",
        ],
    )?;
    let case_id = bounded_u64(object.get("caseId"), summary_count_bound())?;
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
    validate_case_quality_atoms(object.get("qualityAtoms"))?;
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
    let terminal_completed = object
        .get("runtimeEvidence")
        .and_then(|evidence| evidence.get("terminalState"))
        .and_then(serde_json::Value::as_str)
        == Some("completed");
    let overall_pass = exact_bool(object.get("overallPass"))?;
    if overall_pass != (boundary_pass && verdict_pass && terminal_completed) {
        return Err(EvalContractError::new(
            "evaluation_summary_verdict_inconsistent",
        ));
    }
    Ok((case_id, overall_pass, has_boundary))
}

fn validate_case_quality_atoms(value: Option<&serde_json::Value>) -> Result<(), EvalContractError> {
    let object = exact_object(
        value.ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &[
            "requiredFacts",
            "truePositiveFacts",
            "falseNegativeFacts",
            "falsePositiveFacts",
            "requiredSources",
            "recalledRequiredSources",
            "citationRequired",
            "citationSupported",
            "constraintsRequired",
            "constraintsSatisfied",
            "authorizationViolation",
            "offlineWebLeak",
            "unsupportedHighRiskClaim",
            "degradationSignaled",
        ],
    )?;
    for key in object.keys() {
        bounded_u64(object.get(key.as_str()), 1_000)?;
    }
    Ok(())
}

fn validate_capacity_scorecard(value: Option<&serde_json::Value>) -> Result<(), EvalContractError> {
    let object = exact_object(
        value.ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &["hardAdmission", "quality", "performance", "faultRecovery"],
    )?;
    if object.contains_key("overallScore") {
        return Err(EvalContractError::new("evaluation_summary_shape_invalid"));
    }
    let hard = exact_object(
        object
            .get("hardAdmission")
            .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &[
            "authorizationViolations",
            "offlineWebLeaks",
            "unsupportedHighRiskClaims",
            "zeroToleranceGate",
        ],
    )?;
    bounded_u64(hard.get("authorizationViolations"), 1_000)?;
    bounded_u64(hard.get("offlineWebLeaks"), 1_000)?;
    bounded_u64(hard.get("unsupportedHighRiskClaims"), 1_000)?;
    exact_bool(hard.get("zeroToleranceGate"))?;
    let quality = exact_object(
        object
            .get("quality")
            .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &[
            "factPrecisionBps",
            "factPrecisionNumerator",
            "factPrecisionDenominator",
            "factRecallBps",
            "factRecallNumerator",
            "factRecallDenominator",
            "factF1Bps",
            "factF1Numerator",
            "factF1Denominator",
            "requiredSourceRecallBps",
            "requiredSourceRecallNumerator",
            "requiredSourceRecallDenominator",
            "citationSupportBps",
            "citationSupportNumerator",
            "citationSupportDenominator",
            "constraintAdherenceBps",
            "constraintAdherenceNumerator",
            "constraintAdherenceDenominator",
            "factRecallGate",
            "citationSupportGate",
            "constraintAdherenceGate",
        ],
    )?;
    validate_quality_ratio(
        quality,
        "factPrecisionBps",
        "factPrecisionNumerator",
        "factPrecisionDenominator",
        None,
        0,
    )?;
    validate_quality_ratio(
        quality,
        "factRecallBps",
        "factRecallNumerator",
        "factRecallDenominator",
        Some("factRecallGate"),
        9_000,
    )?;
    validate_quality_f1(quality)?;
    validate_quality_ratio(
        quality,
        "requiredSourceRecallBps",
        "requiredSourceRecallNumerator",
        "requiredSourceRecallDenominator",
        None,
        0,
    )?;
    validate_quality_ratio(
        quality,
        "citationSupportBps",
        "citationSupportNumerator",
        "citationSupportDenominator",
        Some("citationSupportGate"),
        9_500,
    )?;
    validate_quality_ratio(
        quality,
        "constraintAdherenceBps",
        "constraintAdherenceNumerator",
        "constraintAdherenceDenominator",
        Some("constraintAdherenceGate"),
        9_500,
    )?;
    let performance = exact_object(
        object
            .get("performance")
            .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &[
            "totalModelTimeP50Ms",
            "totalModelTimeP95Ms",
            "ttftP50Ms",
            "ttftP95Ms",
            "modelTurns",
            "toolCalls",
        ],
    )?;
    for key in [
        "totalModelTimeP50Ms",
        "totalModelTimeP95Ms",
        "ttftP50Ms",
        "ttftP95Ms",
    ] {
        match performance.get(key) {
            Some(serde_json::Value::Null) => {}
            Some(value) => {
                bounded_u64(Some(value), 3_600_000)?;
            }
            None => return Err(EvalContractError::new("evaluation_summary_shape_invalid")),
        }
    }
    bounded_u64(performance.get("modelTurns"), 10_000)?;
    bounded_u64(performance.get("toolCalls"), 10_000)?;
    let fault = exact_object(
        object
            .get("faultRecovery")
            .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &["degradationCases", "constraintFailCases", "truncationCases"],
    )?;
    bounded_u64(fault.get("degradationCases"), 1_000)?;
    bounded_u64(fault.get("constraintFailCases"), 1_000)?;
    bounded_u64(fault.get("truncationCases"), 1_000)?;
    Ok(())
}

fn validate_runtime_evidence(value: Option<&serde_json::Value>) -> Result<(), EvalContractError> {
    let object = exact_object(
        value.ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?,
        &[
            "terminalState",
            "terminalErrorCode",
            "eventCount",
            "observedSourceKinds",
            "toolCallCount",
            "degradationObserved",
            "webQueryBoundary",
            "observedToolClasses",
            "permissionDenialCategories",
        ],
    )?;
    exact_string(
        object.get("terminalState"),
        &["completed", "failed", "cancelled"],
    )?;
    match object.get("terminalErrorCode") {
        Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(code)) => {
            if code.is_empty() || code.len() > 64 || !code.starts_with("agent_run_") {
                return Err(EvalContractError::new("evaluation_summary_value_invalid"));
            }
        }
        _ => return Err(EvalContractError::new("evaluation_summary_shape_invalid")),
    }
    bounded_u64(object.get("eventCount"), 10_000)?;
    bounded_u64(object.get("toolCallCount"), 1_000)?;
    exact_bool(object.get("degradationObserved"))?;
    exact_string(
        object.get("webQueryBoundary"),
        &[
            "not_applicable",
            "confirmed_clean",
            "blocked_local_material",
            "unknown",
        ],
    )?;
    let observed_tool_classes = object
        .get("observedToolClasses")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
    if observed_tool_classes.len() > 6 {
        return Err(EvalContractError::new("evaluation_summary_value_invalid"));
    }
    let mut observed_tool_classes_set = HashSet::with_capacity(observed_tool_classes.len());
    for class in observed_tool_classes {
        exact_string(
            Some(class),
            &[
                "local_read",
                "runtime_context",
                "web_search",
                "external_read",
                "other_catalog_tool",
                "unknown_tool",
            ],
        )?;
        let class = class
            .as_str()
            .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
        if !observed_tool_classes_set.insert(class) {
            return Err(EvalContractError::new("evaluation_summary_value_invalid"));
        }
    }
    let denial_categories = object
        .get("permissionDenialCategories")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
    if denial_categories.len() > 5 {
        return Err(EvalContractError::new("evaluation_summary_value_invalid"));
    }
    let mut observed_denials = HashSet::with_capacity(denial_categories.len());
    for category in denial_categories {
        exact_string(
            Some(category),
            &[
                "local_read",
                "runtime_context",
                "web_search",
                "other_catalog_tool",
                "unknown_tool",
            ],
        )?;
        let category = category
            .as_str()
            .ok_or_else(|| EvalContractError::new("evaluation_summary_shape_invalid"))?;
        if !observed_denials.insert(category) {
            return Err(EvalContractError::new("evaluation_summary_value_invalid"));
        }
    }
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
    if bounded_u64(object.get("caseId"), summary_count_bound())? != expected_case_id {
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
            "online_degradation_disclosed",
            "online_degradation_fabrication",
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
            "local_material_web_query_blocked",
            "local_material_web_query_unverified",
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
