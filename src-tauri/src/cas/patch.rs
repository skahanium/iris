use crate::ai_types::PatchProposal;
use crate::error::AppResult;

/// Apply a confirmed patch after validating its target snapshot and range.
pub fn apply_patch(patch: &PatchProposal, current_content: &str) -> AppResult<String> {
    validate_patch_content(patch, current_content)?;

    let replaced_len = patch.range.end - patch.range.start;
    let mut new_content =
        String::with_capacity(current_content.len() - replaced_len + patch.replacement_text.len());
    new_content.push_str(&current_content[..patch.range.start]);
    new_content.push_str(&patch.replacement_text);
    new_content.push_str(&current_content[patch.range.end..]);

    Ok(new_content)
}

/// Validate the patch range and original text against the current document snapshot.
pub(crate) fn validate_patch_content(
    patch: &PatchProposal,
    current_content: &str,
) -> AppResult<()> {
    let content_len = current_content.len();
    if patch.range.start > patch.range.end
        || patch.range.end > content_len
        || !current_content.is_char_boundary(patch.range.start)
        || !current_content.is_char_boundary(patch.range.end)
    {
        return Err(crate::error::AppError::msg(format!(
            "patch range [{}, {}) exceeds content length {}",
            patch.range.start, patch.range.end, content_len
        )));
    }

    let actual_original = &current_content[patch.range.start..patch.range.end];
    if actual_original != patch.original_text {
        return Err(crate::error::AppError::msg(
            "patch original text does not match current content",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_types::{PatchProposal, RiskLevel, SourceSpan};
    use crate::cas::hash::content_hash_str;

    fn make_patch(
        base_content_hash: String,
        range: std::ops::Range<usize>,
        original_text: &str,
        replacement_text: &str,
    ) -> PatchProposal {
        PatchProposal {
            id: "test-patch".to_string(),
            target_path: "/test/file.md".to_string(),
            base_content_hash,
            range: SourceSpan {
                start: range.start,
                end: range.end,
            },
            original_text: original_text.to_string(),
            replacement_text: replacement_text.to_string(),
            evidence_packet_ids: vec![],
            risk_level: RiskLevel::Low,
            warnings: vec![],
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn applies_a_validated_patch() {
        let content = "Hello, World!";
        let patch = make_patch(content_hash_str(content), 7..12, "World", "Rust");

        assert_eq!(apply_patch(&patch, content).unwrap(), "Hello, Rust!");
    }

    #[test]
    fn rejects_out_of_bounds_patch_ranges() {
        let content = "Hello";
        let patch = make_patch(content_hash_str(content), 0..10, "Hello", "Hi");

        assert!(apply_patch(&patch, content).is_err());
    }

    #[test]
    fn rejects_stale_original_text() {
        let content = "Hello, World!";
        let patch = make_patch(content_hash_str(content), 0..5, "Wrong", "Hi");

        assert!(apply_patch(&patch, content).is_err());
    }
}
