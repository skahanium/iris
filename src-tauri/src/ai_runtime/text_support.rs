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

/// Remove model reasoning markup and an obvious planning prefix before displaying or persisting an
/// answer.
///
/// This is deliberately shared by the streaming surface and the Run terminal paths. A provider
/// may put its hidden planning prose in `content` instead of a dedicated reasoning field; allowing
/// one path to normalize it while another persists the raw content would leak it back into history.
pub fn sanitize_meta_analysis_prefix(text: &str) -> String {
    let without_reasoning = strip_reasoning_tags(text);
    let trimmed = without_reasoning.trim();
    if trimmed.is_empty() || !looks_like_meta_analysis_prefix(trimmed) {
        return trimmed.to_string();
    }

    let mut kept = Vec::new();
    let mut dropping = true;
    let mut dropped_meta = false;
    for paragraph in trimmed
        .split("\n\n")
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        if dropping
            && (looks_like_meta_analysis_paragraph(paragraph)
                || (dropped_meta && looks_like_meta_analysis_continuation(paragraph)))
        {
            dropped_meta = true;
            continue;
        }
        dropping = false;
        kept.push(paragraph);
    }
    kept.join("\n\n")
}

/// Whether a partial streaming prefix must remain private until it can be classified.
pub(crate) fn starts_with_meta_analysis_or_partial_prefix(text: &str) -> bool {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return false;
    }
    if looks_like_meta_analysis_prefix(trimmed) {
        return true;
    }

    let lower = trimmed.to_ascii_lowercase();
    META_ANALYSIS_EN_PREFIXES
        .iter()
        .any(|prefix| prefix.starts_with(lower.as_str()) || lower.starts_with(prefix))
        || META_ANALYSIS_ZH_PREFIXES
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
}

fn looks_like_meta_analysis_prefix(text: &str) -> bool {
    looks_like_meta_analysis_paragraph(text.lines().next().unwrap_or(text))
}

fn looks_like_meta_analysis_paragraph(paragraph: &str) -> bool {
    let trimmed = paragraph.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    META_ANALYSIS_EN_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
        || (lower.starts_with("given ") && contains_explicit_meta_context(&lower))
        || lower.contains("current task focus")
        || lower.contains("persona is")
        || META_ANALYSIS_ZH_PREFIXES
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
}

fn contains_explicit_meta_context(text: &str) -> bool {
    [
        "system prompt",
        "system instruction",
        "authorized material",
        "provided material",
        "tool result",
        "conversation context",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn looks_like_meta_analysis_continuation(paragraph: &str) -> bool {
    if looks_like_meta_analysis_plan_continuation(paragraph) {
        return true;
    }
    let lower = paragraph.trim_start().to_ascii_lowercase();
    ["i should ", "i need to ", "i will ", "i'll ", "we need to "]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

fn looks_like_meta_analysis_plan_continuation(paragraph: &str) -> bool {
    let Some(item) = strip_ordered_or_bullet_marker(paragraph) else {
        return false;
    };
    let lower = item.to_ascii_lowercase();
    [
        "never ", "only ", "not ", "do not ", "must ", "should ", "need to ", "use ", "answer ",
        "provide ", "infer ", "ignore ", "不要", "仅", "只", "必须", "应该", "需要", "先", "然后",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}

fn strip_ordered_or_bullet_marker(paragraph: &str) -> Option<&str> {
    let trimmed = paragraph.trim_start();
    if matches!(trimmed.chars().next(), Some('-' | '*' | '•')) {
        return Some(trimmed[trimmed.chars().next()?.len_utf8()..].trim_start());
    }
    let marker_len = trimmed.char_indices().find_map(|(index, character)| {
        matches!(character, '.' | '、' | ')' | '）').then_some(index + character.len_utf8())
    })?;
    let marker = &trimmed[..marker_len];
    let valid_ordered = marker
        .trim_end_matches(['.', '、', ')', '）'])
        .trim()
        .chars()
        .all(|character| character.is_ascii_digit());
    if valid_ordered {
        Some(trimmed[marker_len..].trim_start())
    } else {
        None
    }
}

fn strip_reasoning_tags(content: &str) -> String {
    const TAGS: [(&str, &str); 3] = [
        ("<thinking>", "</thinking>"),
        ("<think>", "</think>"),
        ("<reasoning>", "</reasoning>"),
    ];

    let mut visible = String::new();
    let mut cursor = 0usize;
    while let Some((start, open, close)) = TAGS
        .iter()
        .filter_map(|(open, close)| {
            find_ascii_case_insensitive(content, open, cursor).map(|start| (start, *open, *close))
        })
        .min_by_key(|(start, _, _)| *start)
    {
        visible.push_str(&content[cursor..start]);
        let body_start = start + open.len();
        if let Some(close_start) = find_ascii_case_insensitive(content, close, body_start) {
            cursor = close_start + close.len();
        } else {
            cursor = content.len();
            break;
        }
    }
    visible.push_str(&content[cursor..]);
    if let Some(partial_start) = find_partial_reasoning_open(&visible) {
        visible.truncate(partial_start);
    }
    visible
}

fn find_partial_reasoning_open(content: &str) -> Option<usize> {
    const OPEN_TAGS: [&str; 3] = ["<thinking>", "<think>", "<reasoning>"];
    const MIN_PARTIAL_PREFIX_LEN: usize = 3;

    let bytes = content.as_bytes();
    (0..bytes.len()).find(|&start| {
        OPEN_TAGS.iter().any(|open| {
            let open = open.as_bytes();
            let shared_prefix_len = bytes[start..]
                .iter()
                .zip(open)
                .take_while(|(left, right)| left.eq_ignore_ascii_case(right))
                .count();
            (MIN_PARTIAL_PREFIX_LEN..open.len()).contains(&shared_prefix_len)
        })
    })
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || bytes.len() < needle.len() || from > bytes.len() - needle.len() {
        return None;
    }
    (from..=bytes.len() - needle.len())
        .find(|&index| bytes[index..index + needle.len()].eq_ignore_ascii_case(needle))
}

const META_ANALYSIS_EN_PREFIXES: [&str; 16] = [
    "the user is asking",
    "the user is greeting",
    "the user is requesting",
    "the user is inquiring",
    "the user asks",
    "the user wants",
    "the user requested",
    "the user has asked",
    "the system prompt ",
    "system prompt ",
    "looking at the conversation",
    "looking at the context",
    "looking at the system prompt",
    "the current task ",
    "the persona ",
    "based on the system ",
];

const META_ANALYSIS_ZH_PREFIXES: [&str; 9] = [
    "用户的问题是",
    "用户想要",
    "用户询问",
    "用户希望",
    "用户的需求",
    "用户要求",
    "当前任务",
    "任务重点",
    "根据系统提示",
];

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

    #[test]
    fn strips_a_multistep_meta_analysis_prefix_without_touching_the_answer() {
        let meta = "The user is asking for current sports information. I should inspect the system instructions before answering.\n\nThe system prompt requires verified evidence before a final response.\n\n1. Never use external knowledge to fill in details\n2. Only answer based on authorized materials\n3. Not infer facts that are not provided\n\n这是基于联网证据的最终答复。";

        assert_eq!(
            sanitize_meta_analysis_prefix(meta),
            "这是基于联网证据的最终答复。"
        );
    }

    #[test]
    fn strips_reasoning_tags_before_normalizing_the_visible_answer() {
        assert_eq!(
            sanitize_meta_analysis_prefix(
                "<think>The user asks for a summary.</think>\n\nHere is the summary."
            ),
            "Here is the summary."
        );
    }

    #[test]
    fn strips_an_incomplete_reasoning_opening_from_the_final_answer() {
        assert_eq!(
            sanitize_meta_analysis_prefix("结论在这里。<thi内部规划不应可见"),
            "结论在这里。"
        );
        assert_eq!(
            sanitize_meta_analysis_prefix("<reasoning内部规划不应可见"),
            ""
        );
    }

    #[test]
    fn preserves_normal_chinese_and_english_answers_that_use_common_openers() {
        let answer = "用户可以在设置中启用兼容模型。\n\n首先，打开设置页面。\n\n好的，我会继续说明。\n\nGiven sufficient context, the answer can be concise and accurate.";

        assert_eq!(sanitize_meta_analysis_prefix(answer), answer);
    }

    #[test]
    fn strips_contextual_given_meta_analysis_without_treating_all_given_answers_as_meta() {
        let meta = "Given there is no current result in the authorized materials and the system prompt requires evidence-only answers.\n\n请提供可验证材料后我再回答。";

        assert_eq!(
            sanitize_meta_analysis_prefix(meta),
            "请提供可验证材料后我再回答。"
        );
    }
}
