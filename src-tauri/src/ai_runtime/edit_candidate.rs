//! C24 unified edit-candidate contract (generate ≠ write).
//!
//! Input: tool name, tool arguments, and the on-disk or virtual full note body.
//! Tools: `replace_selection` | `insert_text_at_cursor` only. Preview uses the
//! existing `apply_patch` path. `base_content_hash` must equal
//! `content_hash_str(original_body)` or preparation fails closed — no freeze,
//! no write.
//!
//! Host memory holds the applied body for format-preservation and freeze JSON.
//! Confirmation events and tool call results still project only `summary` +
//! `targets`; this module never puts note body, `original_text`, `replacement`,
//! or diff hunks into them. The on-demand confirmation diff preview
//! (`confirmation_diff`) is a separate transient IPC response for the owning
//! user's review and is never persisted into events, tool results, audit, or
//! logs.
//!
//! `added_chars` / `removed_chars` are Unicode scalar differences
//! (`candidate_chars.saturating_sub(original_chars)` and the reverse), not a
//! line diff; unified diff computation lives in `confirmation_diff`.
//!
//! Format-unproven stays `format_preservation_unproven` in the C23 gate; it is
//! not an `EditCandidateError`.

use crate::ai_runtime::ToolCallResult;
use crate::ai_types::{PatchProposal, RiskLevel, SourceSpan};
use crate::cas::hash::content_hash_str;
use crate::cas::patch::apply_patch;

/// Host-side typed edit candidate. Never serialized into confirmation events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditCandidate {
    pub(crate) operation: String,
    pub(crate) target_path: String,
    pub(crate) base_content_hash: String,
    pub(crate) expected_post_content_hash: String,
    pub(crate) original_chars: usize,
    pub(crate) candidate_chars: usize,
    pub(crate) added_chars: usize,
    pub(crate) removed_chars: usize,
    /// Applied body kept in host memory only.
    pub(crate) candidate_body: String,
}

/// Reasons a write tool cannot become a freezeable candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditCandidateError {
    MissingFields,
    HashMismatch,
    PatchRejected,
    UnsupportedTool,
}

impl EditCandidateError {
    fn as_str(self) -> &'static str {
        match self {
            Self::MissingFields => "edit_candidate_missing_fields",
            Self::HashMismatch => "edit_candidate_hash_mismatch",
            Self::PatchRejected => "edit_candidate_patch_rejected",
            Self::UnsupportedTool => "edit_candidate_unsupported_tool",
        }
    }
}

/// Build a typed candidate from tool arguments and the current note body.
pub(crate) fn prepare_edit_candidate(
    tool_name: &str,
    args: &serde_json::Value,
    original_body: &str,
) -> Result<EditCandidate, EditCandidateError> {
    if !matches!(tool_name, "replace_selection" | "insert_text_at_cursor") {
        return Err(EditCandidateError::UnsupportedTool);
    }
    let target_path = string_arg(args, &["target_path", "path"])
        .filter(|path| !path.trim().is_empty())
        .ok_or(EditCandidateError::MissingFields)?
        .trim()
        .replace('\\', "/");
    let range = parse_range(args).ok_or(EditCandidateError::MissingFields)?;
    let original_text = string_arg(args, &["original_text", "selection"]).unwrap_or("");
    let replacement_keys = if tool_name == "insert_text_at_cursor" {
        ["text", "replacement"]
    } else {
        ["replacement", "text"]
    };
    let replacement_text =
        string_arg(args, &replacement_keys).ok_or(EditCandidateError::MissingFields)?;
    let claimed_hash =
        string_arg(args, &["base_content_hash"]).ok_or(EditCandidateError::MissingFields)?;
    let base_content_hash = content_hash_str(original_body);
    if claimed_hash != base_content_hash {
        return Err(EditCandidateError::HashMismatch);
    }

    let candidate_body = apply_patch(
        &PatchProposal {
            id: "edit-candidate-preview".to_string(),
            target_path: target_path.clone(),
            base_content_hash: base_content_hash.clone(),
            range,
            original_text: original_text.to_string(),
            replacement_text: replacement_text.to_string(),
            evidence_packet_ids: Vec::new(),
            risk_level: RiskLevel::Low,
            warnings: Vec::new(),
            created_at: String::new(),
        },
        original_body,
    )
    .map_err(|_| EditCandidateError::PatchRejected)?;

    let original_chars = original_body.chars().count();
    let candidate_chars = candidate_body.chars().count();
    Ok(EditCandidate {
        operation: tool_name.to_string(),
        target_path,
        base_content_hash,
        expected_post_content_hash: content_hash_str(&candidate_body),
        original_chars,
        candidate_chars,
        added_chars: candidate_chars.saturating_sub(original_chars),
        removed_chars: original_chars.saturating_sub(candidate_chars),
        candidate_body,
    })
}

/// Tool failure when a write cannot be prepared. Contains no note body.
pub(crate) fn blocked_unprepared_candidate(
    tool_name: &str,
    err: &EditCandidateError,
) -> ToolCallResult {
    let code = err.as_str();
    ToolCallResult {
        tool_name: tool_name.to_string(),
        success: false,
        output: serde_json::json!({ "error": code }),
        duration_ms: 0,
        tokens_used: None,
        error: Some(code.to_string()),
    }
}

/// Confirmation summary: operation and character counts, never note prose.
pub(crate) fn safe_candidate_summary(candidate: &EditCandidate) -> String {
    format!(
        "等待确认：{} 将修改 1 个目标（+{} / −{} 字符）",
        candidate.operation, candidate.added_chars, candidate.removed_chars
    )
}

fn string_arg<'a>(args: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| args.get(*key).and_then(serde_json::Value::as_str))
}

fn parse_range(args: &serde_json::Value) -> Option<SourceSpan> {
    let range = args.get("range")?.as_object()?;
    Some(SourceSpan {
        start: usize::try_from(range.get("start")?.as_u64()?).ok()?,
        end: usize::try_from(range.get("end")?.as_u64()?).ok()?,
    })
}
