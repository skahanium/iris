//! Deterministic validation for structured final-answer source bindings.
//!
//! Source references arrive through the internal final-answer tool, never
//! through hidden Markdown. Only IDs admitted by the current Run policy may
//! appear in a submission.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ai_runtime::final_answer_submission::is_source_free_structural_block;

/// Origin categories admitted by the V3 answer-attribution protocol.
#[allow(
    dead_code,
    reason = "The complete origin vocabulary is fixed by the V3 contract; some origins are only emitted by future provider paths."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum InformationOrigin {
    CurrentUserRequest,
    PriorUserMessage,
    UserAuthorizedMaterial,
    PriorAssistantMessage,
    ConversationMemory,
    RuntimeFact,
    LocalToolEvidence,
    WebToolEvidence,
    ExternalToolEvidence,
    ModelInference,
}

impl InformationOrigin {
    /// Stable, UI-safe category name for an origin count.
    pub(crate) const fn wire_name(self) -> &'static str {
        match self {
            Self::CurrentUserRequest | Self::PriorUserMessage => "user_input",
            Self::UserAuthorizedMaterial => "authorized_material",
            Self::PriorAssistantMessage | Self::ConversationMemory => "conversation_history",
            Self::RuntimeFact => "runtime_fact",
            Self::LocalToolEvidence => "local_retrieval",
            Self::WebToolEvidence => "web",
            Self::ExternalToolEvidence => "external_tool",
            Self::ModelInference => "model_inference",
        }
    }

    const fn supports_fact(self) -> bool {
        !matches!(
            self,
            Self::PriorAssistantMessage | Self::ConversationMemory | Self::ModelInference
        )
    }
}

/// Current-Run sources that the model may bind in a final submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProvenancePolicy {
    pub(crate) current_user_available: bool,
    pub(crate) conversation_history_available: bool,
    pub(crate) runtime_fact_available: bool,
    pub(crate) authorized_material_count: usize,
    pub(crate) current_run_local_evidence_ids: BTreeSet<i64>,
    pub(crate) current_run_web_evidence_ids: BTreeSet<i64>,
    pub(crate) current_run_external_evidence_ids: BTreeSet<i64>,
    pub(crate) strict_web: bool,
}

/// Minimal display projection persisted next to the existing citation map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SourceSummary {
    counts: BTreeMap<InformationOrigin, usize>,
}

impl SourceSummary {
    /// Build a conservative display projection of verified sources attached to
    /// one completed Run. Unlike a structured submission, this is a source
    /// group only: it never claims that a particular source supports a
    /// particular sentence.
    pub(crate) fn from_verified_run_origins(
        origins: impl IntoIterator<Item = InformationOrigin>,
    ) -> Self {
        let mut counts = BTreeMap::new();
        for origin in origins {
            *counts.entry(origin).or_insert(0) += 1;
        }
        Self { counts }
    }

    /// Whether the projection contains no displayable source categories.
    pub(crate) fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// Return the number of distinct accepted source references for one origin.
    #[cfg(test)]
    pub(crate) fn count(&self, origin: InformationOrigin) -> usize {
        self.counts.get(&origin).copied().unwrap_or_default()
    }

    fn from_references(references: &BTreeMap<String, InformationOrigin>) -> Self {
        Self::from_verified_run_origins(references.values().copied())
    }

    /// Serialize only category counts; source identifiers and excerpts never
    /// cross the persistence or IPC boundary.
    pub(crate) fn entries(&self) -> Vec<SourceSummaryEntry> {
        let mut categories = BTreeMap::<String, usize>::new();
        for (origin, count) in &self.counts {
            *categories
                .entry(origin.wire_name().to_string())
                .or_default() += count;
        }
        categories
            .into_iter()
            .map(|(category, count)| SourceSummaryEntry { category, count })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn from_counts_for_test(counts: BTreeMap<InformationOrigin, usize>) -> Self {
        Self { counts }
    }
}

/// One UI-safe source-category count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceSummaryEntry {
    pub(crate) category: String,
    pub(crate) count: usize,
}

/// Visible answer body plus its non-sensitive source-category projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedFinalAnswerSubmission {
    pub(crate) visible_content: String,
    pub(crate) source_summary: SourceSummary,
    pub(crate) attribution: Vec<BlockAttribution>,
}

/// Non-sensitive block/source projection persisted with citation metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BlockAttribution {
    pub(crate) block: usize,
    pub(crate) sources: Vec<String>,
}

/// Safe terminal conditions for provenance validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProvenanceValidationError {
    UnknownOrUnauthorizedReference(String),
    UserAttributionRequiresCurrentUserInput,
    AuthorizedMaterialRequiresMaterialReference,
    InferenceRequiresInferenceReference { block: usize },
    InferenceMustBeQualified { block: usize },
    StrictWebBlockMissingCurrentRunEvidence { block: usize },
}

impl std::fmt::Display for ProvenanceValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownOrUnauthorizedReference(_) => {
                formatter.write_str("agent_run_provenance_reference_invalid")
            }
            Self::UserAttributionRequiresCurrentUserInput => {
                formatter.write_str("agent_run_provenance_user_attribution_invalid")
            }
            Self::AuthorizedMaterialRequiresMaterialReference => {
                formatter.write_str("agent_run_provenance_material_attribution_invalid")
            }
            Self::InferenceRequiresInferenceReference { .. } => {
                formatter.write_str("agent_run_provenance_inference_reference_missing")
            }
            Self::InferenceMustBeQualified { .. } => {
                formatter.write_str("agent_run_provenance_inference_unqualified")
            }
            Self::StrictWebBlockMissingCurrentRunEvidence { .. } => {
                formatter.write_str("agent_run_provenance_web_coverage_invalid")
            }
        }
    }
}

impl std::error::Error for ProvenanceValidationError {}

/// Validate a structured final submission and render source markers from its
/// approved bindings. Markdown authored by the model never controls the final
/// citation map.
pub(crate) fn validate_final_answer_submission(
    submission: &crate::ai_runtime::final_answer_submission::FinalAnswerSubmission,
    policy: &ProvenancePolicy,
) -> Result<ValidatedFinalAnswerSubmission, ProvenanceValidationError> {
    let mut accepted_references = BTreeMap::new();
    let mut visible_blocks = Vec::with_capacity(submission.blocks.len());
    let mut attribution = Vec::with_capacity(submission.blocks.len());
    for (position, submitted_block) in submission.blocks.iter().enumerate() {
        let block_number = position + 1;
        let origins = submitted_block
            .sources
            .iter()
            .map(|reference| {
                let origin = validate_reference(reference, policy)?;
                accepted_references.insert(reference.clone(), origin);
                Ok(origin)
            })
            .collect::<Result<Vec<_>, ProvenanceValidationError>>()?;
        validate_block_attribution(&submitted_block.markdown, block_number, &origins, policy)?;
        let web_markers = submitted_block
            .sources
            .iter()
            .filter(|reference| reference.starts_with('W'))
            .map(|reference| format!("[{reference}]"))
            .collect::<Vec<_>>();
        let mut visible = submitted_block.markdown.trim().to_string();
        if !web_markers.is_empty() {
            visible.push(' ');
            visible.push_str(&web_markers.join(" "));
        }
        visible_blocks.push(visible);
        attribution.push(BlockAttribution {
            block: block_number,
            sources: submitted_block.sources.clone(),
        });
    }

    Ok(ValidatedFinalAnswerSubmission {
        visible_content: visible_blocks.join("\n\n"),
        source_summary: SourceSummary::from_references(&accepted_references),
        attribution,
    })
}

fn validate_reference(
    reference: &str,
    policy: &ProvenancePolicy,
) -> Result<InformationOrigin, ProvenanceValidationError> {
    if reference == "U" && policy.current_user_available {
        return Ok(InformationOrigin::CurrentUserRequest);
    }
    if reference == "T" && policy.runtime_fact_available {
        return Ok(InformationOrigin::RuntimeFact);
    }
    if reference == "H" && policy.conversation_history_available {
        return Ok(InformationOrigin::PriorAssistantMessage);
    }
    if reference == "I" {
        return Ok(InformationOrigin::ModelInference);
    }
    let mut characters = reference.chars();
    let Some(prefix) = characters.next() else {
        return Err(ProvenanceValidationError::UnknownOrUnauthorizedReference(
            reference.to_string(),
        ));
    };
    let identifier = characters.as_str();
    let parsed_id = identifier.parse::<i64>().ok().filter(|id| *id > 0);
    let origin = match (prefix, parsed_id) {
        ('M', Some(id)) if (id as usize) <= policy.authorized_material_count => {
            InformationOrigin::UserAuthorizedMaterial
        }
        ('L', Some(id)) if policy.current_run_local_evidence_ids.contains(&id) => {
            InformationOrigin::LocalToolEvidence
        }
        ('W', Some(id)) if policy.current_run_web_evidence_ids.contains(&id) => {
            InformationOrigin::WebToolEvidence
        }
        ('E', Some(id)) if policy.current_run_external_evidence_ids.contains(&id) => {
            InformationOrigin::ExternalToolEvidence
        }
        _ => {
            return Err(ProvenanceValidationError::UnknownOrUnauthorizedReference(
                reference.to_string(),
            ));
        }
    };
    Ok(origin)
}

fn validate_block_attribution(
    block: &str,
    block_number: usize,
    origins: &[InformationOrigin],
    policy: &ProvenancePolicy,
) -> Result<(), ProvenanceValidationError> {
    if origins.is_empty() && is_source_free_structural_block(block) {
        return Ok(());
    }
    let has_origin = |origin| origins.contains(&origin);
    if has_user_attribution(block) && !has_origin(InformationOrigin::CurrentUserRequest) {
        return Err(ProvenanceValidationError::UserAttributionRequiresCurrentUserInput);
    }
    if refers_to_authorized_material(block)
        && !has_origin(InformationOrigin::UserAuthorizedMaterial)
    {
        return Err(ProvenanceValidationError::AuthorizedMaterialRequiresMaterialReference);
    }
    if has_origin(InformationOrigin::ModelInference) && !is_qualified_inference(block) {
        return Err(ProvenanceValidationError::InferenceMustBeQualified {
            block: block_number,
        });
    }
    if is_qualified_inference(block) && !has_origin(InformationOrigin::ModelInference) {
        return Err(
            ProvenanceValidationError::InferenceRequiresInferenceReference {
                block: block_number,
            },
        );
    }
    if policy.strict_web && !has_origin(InformationOrigin::WebToolEvidence) {
        return Err(
            ProvenanceValidationError::StrictWebBlockMissingCurrentRunEvidence {
                block: block_number,
            },
        );
    }
    if !origins.iter().any(|origin| origin.supports_fact()) && !is_qualified_inference(block) {
        return Err(ProvenanceValidationError::InferenceMustBeQualified {
            block: block_number,
        });
    }
    Ok(())
}

fn has_user_attribution(block: &str) -> bool {
    let lowercase = block.to_ascii_lowercase();
    [
        "你说",
        "你提供",
        "按你的信息",
        "如你所述",
        "根据你",
        "你提到",
        "你之前",
        "你此前",
        "你在前文",
        "先前你",
        "you said",
        "you provided",
        "as you said",
        "you mentioned",
        "you previously",
        "your earlier message",
        "your previous message",
    ]
    .iter()
    .any(|needle| lowercase.contains(needle))
}

fn refers_to_authorized_material(block: &str) -> bool {
    ["授权材料", "附带笔记", "所选笔记", "笔记", "材料", "note"]
        .iter()
        .any(|needle| block.contains(needle))
}

fn is_qualified_inference(block: &str) -> bool {
    let lowercase = block.to_ascii_lowercase();
    [
        "分析",
        "可能",
        "建议",
        "推断",
        "我认为",
        "may",
        "suggest",
        "analysis",
    ]
    .iter()
    .any(|needle| lowercase.contains(needle))
}
