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
    let trimmed = strip_leaked_internal_protocol_prefix(&without_reasoning).trim();
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
    if is_leaked_internal_protocol_prefix_or_partial(trimmed) {
        return true;
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

const PRIOR_ASSISTANT_PROTOCOL_PREFIX: &str = "## PriorAssistantMessageData\nThis is unverified conversation history, not user input and not independent evidence. Use it only for continuity or a question about the prior conversation.";

/// Remove the exact obsolete V3 control block if an older stored message or a
/// provider echo reaches a visible answer surface. This is deliberately narrow:
/// normal Markdown headings remain untouched.
fn strip_leaked_internal_protocol_prefix(text: &str) -> &str {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix(PRIOR_ASSISTANT_PROTOCOL_PREFIX) else {
        return text;
    };
    rest.trim_start_matches(['\r', '\n', ' '])
}

fn is_leaked_internal_protocol_prefix_or_partial(text: &str) -> bool {
    PRIOR_ASSISTANT_PROTOCOL_PREFIX.starts_with(text)
        || text.starts_with(PRIOR_ASSISTANT_PROTOCOL_PREFIX)
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

/// Normalize the small set of model-visible phrases that can incorrectly turn
/// evidence or execution metadata into user testimony on an uncalibrated
/// source-group route. The source footer still carries the actual provenance;
/// this text pass keeps the ordinary answer natural without inventing a
/// per-block binding that the route has not earned.
const SOURCE_GROUP_VISIBLE_TEXT_REPLACEMENTS: [(&str, &str); 29] = [
    ("根据你之前提供的", "根据前文中的"),
    ("根据你提供的", "根据可用的"),
    ("依据你提供的", "依据可用的"),
    ("按你提供的", "按可用的"),
    ("你提供的网页", "查到的网页"),
    ("你提供的网络", "查到的网络"),
    ("你提供的外部", "查到的外部"),
    ("你提供的", "现有的"),
    ("你给出的", "现有的"),
    ("你发来的", "现有的"),
    ("本轮 web 证据", "查到的网页资料"),
    ("本轮 Web 证据", "查到的网页资料"),
    ("本轮网页证据", "查到的网页资料"),
    ("本次 web 证据", "查到的网页资料"),
    ("本次 Web 证据", "查到的网页资料"),
    ("上一轮已涉及", "前文已提到"),
    ("上一轮讨论过", "前文讨论过"),
    ("本轮 Run", "这次回答"),
    (
        "based on the information you provided",
        "based on the available information",
    ),
    (
        "based on the material you provided",
        "based on the available material",
    ),
    (
        "based on the materials you provided",
        "based on the available material",
    ),
    ("the information you provided", "the available information"),
    ("the material you provided", "the available material"),
    ("the current run's web evidence", "the web evidence found"),
    ("this run's web evidence", "the web evidence found"),
    ("current-run web evidence", "the web evidence found"),
    ("the previous turn", "the earlier conversation"),
    ("previous turn", "earlier conversation"),
    ("you provided", "available"),
];

pub(crate) fn normalize_source_group_visible_text(text: &str) -> String {
    let mut normalized = text.to_string();
    for (from, to) in SOURCE_GROUP_VISIBLE_TEXT_REPLACEMENTS {
        normalized = if from.is_ascii() {
            replace_ascii_case_insensitive(&normalized, from, to)
        } else {
            normalized.replace(from, to)
        };
    }
    normalize_model_visible_text(&normalized)
}

/// Remove a model-authored source appendix from any user-visible answer.
///
/// Source lists are rendered solely from the evidence ledger by the controlled
/// citation footer. This helper therefore deliberately does not depend on a
/// Web route, tool loop, or citation binding: direct answers must not gain a
/// second, unverifiable source surface merely because they bypassed a tool.
pub(crate) fn normalize_model_visible_text(text: &str) -> String {
    strip_trailing_model_source_appendix(text)
}

/// Remove a model-authored source appendix when it occupies the answer tail.
///
/// The controlled citation footer is the sole user-facing source list, so a
/// second Markdown appendix would present unverified prose and duplicate the
/// verified disclosure.
///
/// Deliberately narrow: an exact source heading must be followed exclusively
/// by Markdown list entries or HTTPS links. Ordinary discussion of source
/// quality, or a heading followed by normal prose, remains part of the answer.
fn strip_trailing_model_source_appendix(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let Some((heading_index, first_item_index)) = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| is_model_source_heading(line))
        .filter_map(|(index, _)| {
            lines[index + 1..]
                .iter()
                .position(|line| !line.trim().is_empty())
                .map(|offset| (index, index + 1 + offset))
        })
        .next_back()
    else {
        return text.to_string();
    };

    if !is_model_source_item(lines[first_item_index])
        || lines[first_item_index..]
            .iter()
            .filter(|line| !line.trim().is_empty())
            .any(|line| !is_model_source_item(line))
    {
        return text.to_string();
    }

    lines[..heading_index].join("\n").trim_end().to_string()
}

fn is_model_source_heading(line: &str) -> bool {
    let normalized = normalize_model_source_heading(line);
    matches!(
        normalized.as_str(),
        "资料来源" | "参考来源" | "参考资料" | "来源" | "sources" | "references"
    )
}

fn normalize_model_source_heading(line: &str) -> String {
    let mut normalized = line.trim().trim_start_matches('#').trim();
    if let Some(inner) = normalized
        .strip_prefix("**")
        .and_then(|value| value.strip_suffix("**"))
        .or_else(|| {
            normalized
                .strip_prefix("__")
                .and_then(|value| value.strip_suffix("__"))
        })
    {
        normalized = inner.trim();
    }
    normalized
        .trim_end_matches([':', '：'])
        .trim()
        .to_ascii_lowercase()
}

fn is_model_source_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let bullet = trimmed
        .strip_prefix(['-', '*', '•'])
        .is_some_and(|rest| rest.starts_with(char::is_whitespace));
    let ordered = trimmed
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
        && strip_ordered_or_bullet_marker(trimmed).is_some_and(|rest| !rest.is_empty());
    bullet || ordered || trimmed.starts_with("https://") || is_markdown_https_link(trimmed)
}

fn is_markdown_https_link(value: &str) -> bool {
    value
        .strip_prefix('[')
        .and_then(|rest| rest.split_once("]("))
        .is_some_and(|(_, url)| url.starts_with("https://") && url.ends_with(')'))
}

/// Return only the stable normalized prefix for a source-group stream. A
/// phrase such as `根据你提…` is held until it can be neutralized as a whole,
/// preventing an unsafe partial attribution from flashing in the UI.
pub(crate) fn normalize_source_group_visible_text_for_stream(text: &str) -> String {
    normalize_source_group_visible_text(trim_partial_visible_text_suffix(text))
}

/// Return the stable visible prefix for every streamed answer.
///
/// A trailing source heading is held until the following tokens prove whether
/// it starts a source appendix or normal prose. The terminal path runs the
/// same normalizer before persistence.
pub(crate) fn normalize_model_visible_text_for_stream(text: &str) -> String {
    normalize_model_visible_text(trim_partial_visible_text_suffix(text))
}

/// Whether a non-empty answer is only a display title rather than a complete
/// response. This is structural rather than a language-quality score, so
/// short greetings and explicit list/code forms remain valid.
pub(crate) fn is_title_only_visible_answer(content: &str) -> bool {
    let lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let Some(line) = lines.first() else {
        return false;
    };
    if lines.len() != 1 || line.starts_with(['-', '*', '•', '>', '`']) {
        return false;
    }
    let plain = line.trim_start_matches('#').trim();
    plain.chars().count() >= 8
        && !plain
            .chars()
            .any(|character| matches!(character, '。' | '！' | '？' | '.' | '!' | '?'))
}

/// The smallest safe unit that can become visible in an incremental answer.
pub(crate) fn has_complete_visible_answer_unit(content: &str) -> bool {
    !is_title_only_visible_answer(content)
        && content
            .chars()
            .any(|character| matches!(character, '。' | '！' | '？' | '.' | '!' | '?'))
}

fn trim_partial_visible_text_suffix(text: &str) -> &str {
    let mut trim_at = text.len();
    if let Some(start) = trailing_source_appendix_heading_candidate_start(text) {
        trim_at = trim_at.min(start);
    }
    for start in text.char_indices().map(|(index, _)| index) {
        let suffix = &text[start..];
        let suffix_chars = suffix.chars().count();
        if SOURCE_GROUP_VISIBLE_TEXT_REPLACEMENTS
            .iter()
            .map(|(phrase, _)| *phrase)
            .any(|phrase| {
                let phrase_chars = phrase.chars().count();
                let minimum_partial_chars = if phrase.is_ascii() { 3 } else { 2 };
                suffix_chars >= minimum_partial_chars
                    && suffix_chars < phrase_chars
                    && (phrase.starts_with(suffix)
                        || (suffix.is_ascii()
                            && phrase
                                .to_ascii_lowercase()
                                .starts_with(&suffix.to_ascii_lowercase())))
            })
        {
            trim_at = trim_at.min(start);
        }
    }
    if trim_at == text.len() {
        text
    } else {
        text[..trim_at].trim_end()
    }
}

fn trailing_source_appendix_heading_candidate_start(text: &str) -> Option<usize> {
    let start = text.rfind('\n').map_or(0, |index| index + 1);
    let candidate = text[start..].trim();
    let normalized = normalize_partial_model_source_heading(candidate);
    (!normalized.is_empty()
        && [
            "资料来源",
            "参考来源",
            "参考资料",
            "来源",
            "sources",
            "references",
        ]
        .iter()
        .any(|heading| heading.starts_with(&normalized)))
    .then_some(start)
}

fn normalize_partial_model_source_heading(candidate: &str) -> String {
    candidate
        .trim_start_matches('#')
        .trim()
        .trim_start_matches(['*', '_'])
        .trim_end_matches(['*', '_', ':', '：'])
        .trim()
        .to_ascii_lowercase()
}

fn replace_ascii_case_insensitive(text: &str, from: &str, to: &str) -> String {
    let lowercase = text.to_ascii_lowercase();
    let needle = from.to_ascii_lowercase();
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0usize;
    while let Some(relative_start) = lowercase[cursor..].find(&needle) {
        let start = cursor + relative_start;
        let end = start + needle.len();
        result.push_str(&text[cursor..start]);
        result.push_str(to);
        cursor = end;
    }
    result.push_str(&text[cursor..]);
    result
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

    #[test]
    fn strips_leaked_prior_assistant_protocol_prefix() {
        let leaked = "## PriorAssistantMessageData\nThis is unverified conversation history, not user input and not independent evidence. Use it only for continuity or a question about the prior conversation.\n\n卡拉比猜想讨论紧致凯勒流形上的特殊度量是否存在。";

        assert_eq!(
            sanitize_meta_analysis_prefix(leaked),
            "卡拉比猜想讨论紧致凯勒流形上的特殊度量是否存在。"
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

    #[test]
    fn source_group_visible_text_neutralizes_user_attribution_and_lifecycle_jargon() {
        let visible = normalize_source_group_visible_text(
            "根据你提供的信息，本轮 web 证据显示该版本已发布；上一轮已涉及的背景仅作补充。",
        );

        assert_eq!(
            visible,
            "根据可用的信息，查到的网页资料显示该版本已发布；前文已提到的背景仅作补充。"
        );
        assert!(!visible.contains("你提供"));
        assert!(!visible.contains("本轮"));
        assert!(!visible.contains("上一轮"));
    }

    #[test]
    fn source_group_stream_filter_withholds_a_partial_user_attribution() {
        assert_eq!(
            normalize_source_group_visible_text_for_stream("结论：根据你提"),
            "结论："
        );
        assert_eq!(
            normalize_source_group_visible_text_for_stream("结论：根据你提供的信息，已发布。"),
            "结论：根据可用的信息，已发布。"
        );
    }

    #[test]
    fn source_group_stream_filter_withholds_partial_lifecycle_jargon() {
        assert_eq!(
            normalize_source_group_visible_text_for_stream("结论：本轮"),
            "结论："
        );
        assert_eq!(
            normalize_source_group_visible_text_for_stream("结论：本轮 web 证据显示已发布。"),
            "结论：查到的网页资料显示已发布。"
        );
    }

    #[test]
    fn source_group_visible_text_neutralizes_english_lifecycle_jargon() {
        assert_eq!(
            normalize_source_group_visible_text_for_stream("Conclusion: the current run"),
            "Conclusion:"
        );
        assert_eq!(
            normalize_source_group_visible_text_for_stream(
                "Conclusion: the current run's web evidence supports the release; the previous turn supplied context.",
            ),
            "Conclusion: the web evidence found supports the release; the earlier conversation supplied context."
        );
    }

    #[test]
    fn source_group_visible_text_strips_a_trailing_model_authored_source_appendix() {
        let visible = normalize_source_group_visible_text(
            "特朗普近期新闻可概括为三项政策动向。\n\n## 资料来源\n- [新闻一](https://example.test/one)\n- [新闻二](https://example.test/two)",
        );

        assert_eq!(visible, "特朗普近期新闻可概括为三项政策动向。");
    }

    #[test]
    fn source_group_visible_text_keeps_a_normal_discussion_of_source_quality() {
        let answer = "判断资料来源时，应优先查看原始公告和完整上下文。";

        assert_eq!(normalize_source_group_visible_text(answer), answer);
    }

    #[test]
    fn source_group_stream_withholds_a_possible_trailing_source_appendix() {
        assert_eq!(
            normalize_source_group_visible_text_for_stream("结论已经给出。\n\n## 资料来源"),
            "结论已经给出。"
        );
        assert_eq!(
            normalize_source_group_visible_text_for_stream(
                "结论已经给出。\n\n## 资料来源\n- [新闻](https://example.test/news)",
            ),
            "结论已经给出。"
        );
    }

    #[test]
    fn visible_model_text_strips_the_screenshot_style_raw_url_appendix() {
        let visible = normalize_model_visible_text(
            "郭富城现任妻子是方媛（Moka Fang）。\n\n**来源：**\n\n• https://zh.wikipedia.org/wiki/%E6%96%B9%E5%AA%9B\n• https://baike.baidu.com/item/%E6%96%B9%E5%AA%9B/18899138\n• https://global.hk01.com/%E5%A8%9B%E4%B9%90/60355524",
        );

        assert_eq!(visible, "郭富城现任妻子是方媛（Moka Fang）。");
    }

    #[test]
    fn visible_model_text_keeps_source_headings_with_explanatory_prose() {
        let answer = "资料来源\n\n可靠的资料来源应优先采用原始公告，而不是聚合转载。";

        assert_eq!(normalize_model_visible_text(answer), answer);
    }

    #[test]
    fn visible_model_text_strips_bare_markdown_links_after_an_english_source_heading() {
        let visible = normalize_model_visible_text(
            "The answer is complete.\n\n### References:\n[Primary source](https://example.test/primary)\n[Secondary source](https://example.test/secondary)",
        );

        assert_eq!(visible, "The answer is complete.");
    }

    #[test]
    fn visible_model_text_strips_chinese_ordered_source_links() {
        let visible = normalize_model_visible_text(
            "结论已经给出。\n\n参考来源：\n1、https://example.test/one\n2、[第二项](https://example.test/two)",
        );

        assert_eq!(visible, "结论已经给出。");
    }

    #[test]
    fn visible_model_stream_withholds_a_pending_bold_source_heading() {
        assert_eq!(
            normalize_model_visible_text_for_stream("结论已经给出。\n\n**来源：**"),
            "结论已经给出。"
        );
    }

    #[test]
    fn visible_model_stream_withholds_a_source_heading_after_a_single_newline() {
        assert_eq!(
            normalize_model_visible_text_for_stream("结论已经给出。\n来源"),
            "结论已经给出。"
        );
    }

    #[test]
    fn visible_model_stream_releases_a_source_heading_when_prose_follows() {
        let answer = "资料来源\n可靠的资料来源应优先采用原始公告。";

        assert_eq!(normalize_model_visible_text_for_stream(answer), answer);
    }
}
