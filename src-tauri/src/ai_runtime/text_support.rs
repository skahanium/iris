//! Stateless text-budget and response-normalization helpers used by the Run engine.
//!
//! This module intentionally owns no session, checkpoint, confirmation, tool-loop, or
//! workflow state. Those responsibilities belong to the explicit Run contract.

/// Estimate token count with a conservative CJK-aware heuristic.
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let chars = text.chars().count();
    let cjk = text
        .chars()
        .filter(|ch| {
            matches!(
                *ch as u32,
                0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3040..=0x30FF | 0xAC00..=0xD7AF
            )
        })
        .count();
    cjk.saturating_add(chars.saturating_sub(cjk).div_ceil(4))
        .max(1)
}

/// Remove an obvious model-planning prefix before displaying the answer.
pub fn sanitize_meta_analysis_prefix(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || !looks_like_meta_analysis_prefix(trimmed) {
        return trimmed.to_string();
    }

    let mut kept = Vec::new();
    let mut dropping = true;
    for paragraph in trimmed
        .split("\n\n")
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        if dropping && looks_like_meta_analysis_paragraph(paragraph) {
            continue;
        }
        dropping = false;
        kept.push(paragraph);
    }
    kept.join("\n\n")
}

fn looks_like_meta_analysis_prefix(text: &str) -> bool {
    looks_like_meta_analysis_paragraph(text.lines().next().unwrap_or(text))
}

fn looks_like_meta_analysis_paragraph(paragraph: &str) -> bool {
    let trimmed = paragraph.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("the user ")
        || lower.starts_with("this is a ")
        || lower.starts_with("the current task ")
        || lower.starts_with("i should ")
        || lower.starts_with("i'll ")
        || lower.starts_with("the persona ")
        || lower.contains("current task focus")
        || lower.contains("persona is")
        || trimmed.starts_with("用户")
        || trimmed.starts_with("当前任务")
        || trimmed.starts_with("我需要")
        || trimmed.starts_with("我应该")
        || trimmed.starts_with("我将")
        || trimmed.starts_with("首先")
        || trimmed.starts_with("接下来")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_is_cjk_aware() {
        assert!(estimate_tokens(&"汉".repeat(300)) >= 300);
        assert!(estimate_tokens(&"x".repeat(300)) <= 80);
    }

    #[test]
    fn strips_meta_analysis_but_not_answer() {
        assert_eq!(
            sanitize_meta_analysis_prefix("The user asks for a summary.\n\nHere is the summary."),
            "Here is the summary."
        );
        assert_eq!(
            sanitize_meta_analysis_prefix("A direct answer."),
            "A direct answer."
        );
    }
}
