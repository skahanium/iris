//! Structured, internal-only final-answer submission.
//!
//! This replaces the former hidden Markdown sidecar. The model submits a
//! bounded JSON payload through the existing tool protocol; the payload is
//! never sent to a tool executor or persisted as conversation history.

use serde::Deserialize;

use crate::ai_runtime::{ToolCall, ToolSpec};
use crate::ai_types::ToolAccessLevel;
use crate::error::{AppError, AppResult};

const MAX_FINAL_BLOCKS: usize = 32;
const MAX_SOURCES_PER_BLOCK: usize = 16;
const MAX_SOURCE_REFERENCE_CHARS: usize = 32;
const MAX_VISIBLE_CONTENT_CHARS: usize = 32_000;

/// Reserved tool name used only to end an evidence-bearing Agent Run.
pub(crate) const FINAL_ANSWER_TOOL_NAME: &str = "submit_final_answer";

/// Build the internal-only tool surface for a calibrated strict route.
pub(crate) fn tool_spec() -> ToolSpec {
    ToolSpec {
        name: FINAL_ANSWER_TOOL_NAME.to_string(),
        description:
            "Submit the final answer as ordered Markdown blocks with current-Run source references."
                .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["blocks"],
            "properties": {
                "blocks": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 32,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["markdown", "sources"],
                        "properties": {
                            "markdown": { "type": "string", "minLength": 1, "maxLength": 32000 },
                            "sources": { "type": "array", "minItems": 1, "maxItems": 16, "items": { "type": "string", "minLength": 1, "maxLength": 32 } }
                        }
                    }
                }
            }
        }),
        access_level: ToolAccessLevel::ReadProfile,
        requires_confirmation: false,
        max_results: None,
        capability_affinity: Vec::new(),
    }
}

/// One visible Markdown block and the source references claimed for it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct FinalAnswerBlock {
    pub(crate) markdown: String,
    pub(crate) sources: Vec<String>,
}

/// The model's internal final-answer envelope.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct FinalAnswerSubmission {
    pub(crate) blocks: Vec<FinalAnswerBlock>,
}

impl FinalAnswerSubmission {
    /// Parse a terminal submission without executing any external tool.
    pub(crate) fn from_tool_call(call: &ToolCall) -> AppResult<Self> {
        if call.function.name != FINAL_ANSWER_TOOL_NAME {
            return Err(AppError::msg("agent_run_final_submission_tool_invalid"));
        }
        let submission = serde_json::from_str::<Self>(&call.function.arguments)
            .map_err(|_| AppError::msg("agent_run_final_submission_invalid"))?;
        if submission.blocks.is_empty()
            || submission.blocks.len() > MAX_FINAL_BLOCKS
            || submission
                .blocks
                .iter()
                .map(|block| block.markdown.chars().count())
                .sum::<usize>()
                > MAX_VISIBLE_CONTENT_CHARS
            || submission.blocks.iter().any(|block| {
                block.markdown.trim().is_empty()
                    || block.markdown.to_ascii_lowercase().contains("[w")
                    || block.sources.is_empty()
                    || block.sources.len() > MAX_SOURCES_PER_BLOCK
                    || block.sources.iter().any(|source| {
                        source.trim().is_empty()
                            || source.chars().count() > MAX_SOURCE_REFERENCE_CHARS
                    })
                    || block
                        .sources
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        != block.sources.len()
            })
        {
            return Err(AppError::msg("agent_run_final_submission_invalid"));
        }
        Ok(submission)
    }

    /// Render only model-authored visible Markdown. Source markers are added
    /// later by the Run-bound validator, never trusted from this payload.
    pub(crate) fn visible_content(&self) -> String {
        self.blocks
            .iter()
            .map(|block| block.markdown.trim())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::{FinalAnswerSubmission, FINAL_ANSWER_TOOL_NAME};
    use crate::ai_runtime::ToolCall;

    #[test]
    fn terminal_submission_rejects_unbounded_or_duplicate_source_payloads() {
        let duplicate = ToolCall::new(
            "final-duplicate",
            FINAL_ANSWER_TOOL_NAME,
            r#"{"blocks":[{"markdown":"结论","sources":["W1","W1"]}]}"#,
        );
        assert!(FinalAnswerSubmission::from_tool_call(&duplicate).is_err());

        let oversized = ToolCall::new(
            "final-oversized",
            FINAL_ANSWER_TOOL_NAME,
            serde_json::json!({
                "blocks": [{
                    "markdown": "甲".repeat(32_001),
                    "sources": ["W1"]
                }]
            })
            .to_string(),
        );
        assert!(FinalAnswerSubmission::from_tool_call(&oversized).is_err());
    }
}
