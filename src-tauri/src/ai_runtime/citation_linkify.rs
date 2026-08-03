//! Pure helpers that turn bare web footnotes into Markdown HTTPS links.

use crate::ai_types::{CitationBinding, CitationBindingMode, WebCitationEntry};
use serde_json::Value;

/// One web evidence row used to rewrite model footnotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebCitationLink {
    /// Session-local citation index (1-based).
    pub(crate) index: i64,
    /// Ledger label such as `[C1]`.
    pub(crate) label: String,
    /// Safe display title.
    pub(crate) title: String,
    /// HTTPS URL from the evidence ledger.
    pub(crate) url: String,
}

/// Deterministic binding outcome for a strict-Web answer. Formatting variance
/// is absorbed locally; only the absence of usable evidence remains terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CitationBindingOutcome {
    pub(crate) content: String,
    pub(crate) binding: CitationBinding,
}

/// A V3 strict-Web answer cannot use answer-level source-group fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StrictCitationBindingError {
    MissingPreciseCurrentRunMarkers,
}

impl std::fmt::Display for StrictCitationBindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("agent_run_strict_citation_markers_missing")
    }
}

/// Bind citations only when the body contains precise current-Run markers.
pub(crate) fn bind_strict_current_run_citations(
    content: &str,
    cites: &[WebCitationLink],
) -> Result<CitationBindingOutcome, StrictCitationBindingError> {
    let outcome = bind_current_run_citations(content, cites);
    if outcome.binding.mode == CitationBindingMode::SourceGroupFallback {
        return Err(StrictCitationBindingError::MissingPreciseCurrentRunMarkers);
    }
    Ok(outcome)
}

/// Normalize model markers into the current Run projection without asking a
/// model to rewrite prose. Unknown numeric markers are removed from the body
/// and the verified answer falls back to an answer-level source group.
pub(crate) fn bind_current_run_citations(
    content: &str,
    cites: &[WebCitationLink],
) -> CitationBindingOutcome {
    let chars = content.chars().collect::<Vec<_>>();
    let mut normalized = String::with_capacity(content.len());
    let mut cursor = 0;
    let mut referenced_indices = Vec::new();
    let mut normalized_marker = false;
    let mut fallback_reason = None;

    while cursor < chars.len() {
        if chars[cursor] != '[' {
            normalized.push(chars[cursor]);
            cursor += 1;
            continue;
        }
        let mut end = cursor + 1;
        while end < chars.len() && chars[end] != ']' {
            end += 1;
        }
        if end == chars.len() {
            normalized.push(chars[cursor]);
            cursor += 1;
            continue;
        }
        let original_label = chars[cursor + 1..end].iter().collect::<String>();
        let label = normalize_marker_label(&original_label);
        match parse_marker_index(&label) {
            Some(index) if cites.iter().any(|cite| cite.index == index) => {
                if original_label != format!("W{index}") {
                    normalized_marker = true;
                }
                if !referenced_indices.contains(&index) {
                    referenced_indices.push(index);
                }
                normalized.push_str(&format!("[W{index}]"));
            }
            Some(_) => {
                fallback_reason = Some("unknown_marker".to_string());
                normalized.push_str("[来源待确认]");
            }
            None => {
                normalized.push('[');
                normalized.push_str(&original_label);
                normalized.push(']');
            }
        }
        cursor = end + 1;
    }

    let binding = if fallback_reason.is_some() || referenced_indices.is_empty() {
        CitationBinding {
            mode: CitationBindingMode::SourceGroupFallback,
            referenced_indices: Vec::new(),
            fallback_reason: fallback_reason.or_else(|| Some("missing_marker".to_string())),
        }
    } else {
        CitationBinding {
            mode: if normalized_marker {
                CitationBindingMode::Normalized
            } else {
                CitationBindingMode::Exact
            },
            referenced_indices,
            fallback_reason: None,
        }
    };
    CitationBindingOutcome {
        content: normalized,
        binding,
    }
}

/// Remove model-authored citation marker syntax when a Run intentionally uses
/// answer-level source-group disclosure. In that mode no visible marker may
/// imply that the harness verified a claim-level binding.
pub(crate) fn strip_model_authored_citation_markers(content: &str) -> String {
    let chars = content.chars().collect::<Vec<_>>();
    let mut sanitized = String::with_capacity(content.len());
    let mut cursor = 0;
    while cursor < chars.len() {
        if chars[cursor] != '[' {
            sanitized.push(chars[cursor]);
            cursor += 1;
            continue;
        }
        let mut end = cursor + 1;
        while end < chars.len() && chars[end] != ']' {
            end += 1;
        }
        if end == chars.len() {
            sanitized.push(chars[cursor]);
            cursor += 1;
            continue;
        }
        let label = chars[cursor + 1..end].iter().collect::<String>();
        if parse_marker_index(&normalize_marker_label(&label)).is_none() {
            sanitized.push('[');
            sanitized.push_str(&label);
            sanitized.push(']');
        }
        cursor = end + 1;
    }
    sanitized
}

/// Apply the source-group citation policy to a still-growing model stream.
///
/// A marker such as `[W1]` may arrive across several provider chunks.  Hold
/// only a possible trailing marker prefix back until it is complete; emitting
/// `[W` even briefly would falsely suggest claim-level verification in a
/// source-group Run.
pub(crate) fn strip_model_authored_citation_markers_for_stream(content: &str) -> String {
    let stable = content
        .rfind('[')
        .filter(|start| !content[*start..].contains(']'))
        .filter(|start| possible_citation_marker_prefix(&content[*start..]))
        .map_or(content, |start| &content[..start]);
    strip_model_authored_citation_markers(stable)
}

fn possible_citation_marker_prefix(value: &str) -> bool {
    let mut characters = value.chars();
    if characters.next() != Some('[') {
        return false;
    }
    match characters.next() {
        None => true,
        Some('W' | 'w') => characters.all(|character| character.is_ascii_digit()),
        Some(character) => {
            character.is_ascii_digit() && characters.all(|item| item.is_ascii_digit())
        }
    }
}

/// Rewrite bare footnote markers / source lines into clickable Markdown links.
///
/// Models often emit Unicode superscript footnotes without URLs. When the Run
/// already registered HTTPS web evidence, convert those lines to
/// `[1. Title](https://...)` so the UI can open the system browser.
pub(crate) fn linkify_web_citations(content: &str, cites: &[WebCitationLink]) -> String {
    if cites.is_empty() || content.trim().is_empty() {
        return content.to_string();
    }

    let normalized = normalize_superscript_brackets(content);
    let with_lists = linkify_source_list_lines(&normalized, cites);
    linkify_inline_markers(&with_lists, cites)
}

/// Remove prior Run web-citation syntax before an assistant answer is reused as
/// model history. The persisted answer and its UI citation map stay unchanged;
/// only the provider-facing copy loses historical URLs and `[Wn]` labels so a
/// later strict-Web Run cannot mistake them for current-Run evidence.
pub(crate) fn sanitize_web_citations_for_model_history(
    content: &str,
    citations: &[WebCitationEntry],
) -> String {
    if content.trim().is_empty() {
        return content.to_string();
    }

    let sanitized = sanitize_markdown_http_links(content, citations);
    let sanitized = sanitize_bare_http_urls(&sanitized);
    let sanitized = citations.iter().fold(sanitized, |history, citation| {
        history.replace(
            &format!("[W{}]", citation.index),
            &historical_marker(citation.index),
        )
    });
    sanitize_unknown_current_run_markers(&sanitized)
}

fn sanitize_markdown_http_links(content: &str, citations: &[WebCitationEntry]) -> String {
    let mut sanitized = String::with_capacity(content.len());
    let mut cursor = 0;
    while let Some(relative_start) = content[cursor..].find('[') {
        let start = cursor + relative_start;
        sanitized.push_str(&content[cursor..start]);
        let Some(relative_close) = content[start..].find(']') else {
            sanitized.push_str(&content[start..]);
            cursor = content.len();
            break;
        };
        let close = start + relative_close;
        let label = &content[start + 1..close];
        let link_start = close + 1;
        if content[link_start..].starts_with('(') {
            let url_start = link_start + 1;
            if let Some(end) = markdown_link_end(content, url_start) {
                let url = &content[url_start..end];
                if url.starts_with("https://") || url.starts_with("http://") {
                    if let Some(citation) = citations.iter().find(|citation| {
                        citation.url == url
                            && parse_marker_index(label)
                                .is_some_and(|index| index == citation.index)
                    }) {
                        sanitized.push_str(&historical_marker(citation.index));
                    } else {
                        sanitized.push_str(&format!("[历史链接: {}]", label.trim()));
                    }
                    cursor = end + 1;
                    continue;
                }
            }
        }
        sanitized.push('[');
        cursor = start + 1;
    }
    sanitized.push_str(&content[cursor..]);
    sanitized
}

fn markdown_link_end(content: &str, url_start: usize) -> Option<usize> {
    let mut depth = 1_u32;
    for (offset, character) in content[url_start..].char_indices() {
        match character {
            '(' => depth = depth.saturating_add(1),
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(url_start + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn sanitize_bare_http_urls(content: &str) -> String {
    let mut sanitized = String::with_capacity(content.len());
    let mut cursor = 0;
    loop {
        let https = content[cursor..]
            .find("https://")
            .map(|offset| cursor + offset);
        let http = content[cursor..]
            .find("http://")
            .map(|offset| cursor + offset);
        let Some(start) = [https, http].into_iter().flatten().min() else {
            break;
        };
        sanitized.push_str(&content[cursor..start]);
        let end = content[start..]
            .find(|character: char| {
                character.is_whitespace()
                    || matches!(character, ')' | ']' | '>' | '。' | '，' | '；')
            })
            .map(|offset| start + offset)
            .unwrap_or(content.len());
        sanitized.push_str("[历史链接]");
        cursor = end;
    }
    sanitized.push_str(&content[cursor..]);
    sanitized
}

fn sanitize_unknown_current_run_markers(content: &str) -> String {
    let mut sanitized = String::with_capacity(content.len());
    let mut cursor = 0;
    while let Some(relative_start) = content[cursor..].find("[W") {
        let start = cursor + relative_start;
        sanitized.push_str(&content[cursor..start]);
        let candidate = &content[start + 2..];
        let Some(end) = candidate.find(']') else {
            sanitized.push_str(&content[start..]);
            return sanitized;
        };
        if !candidate[..end].is_empty()
            && candidate[..end].bytes().all(|byte| byte.is_ascii_digit())
        {
            sanitized.push_str("[历史来源]");
            cursor = start + 2 + end + 1;
        } else {
            sanitized.push_str("[W");
            cursor = start + 2;
        }
    }
    sanitized.push_str(&content[cursor..]);
    sanitized
}

fn historical_marker(index: i64) -> String {
    format!("[历史来源 {index}]")
}

fn normalize_superscript_brackets(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            let mut j = i + 1;
            let mut label = String::new();
            while j < chars.len() && chars[j] != ']' {
                label.push(chars[j]);
                j += 1;
            }
            if j < chars.len() && chars[j] == ']' {
                let normalized = normalize_marker_label(&label);
                out.push('[');
                out.push_str(&normalized);
                out.push(']');
                i = j + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn normalize_marker_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| match ch {
            '\u{2070}' => '0',
            '\u{00B9}' => '1',
            '\u{00B2}' => '2',
            '\u{00B3}' => '3',
            '\u{2074}' => '4',
            '\u{2075}' => '5',
            '\u{2076}' => '6',
            '\u{2077}' => '7',
            '\u{2078}' => '8',
            '\u{2079}' => '9',
            other => other,
        })
        .collect()
}

fn linkify_source_list_lines(content: &str, cites: &[WebCitationLink]) -> String {
    let trailing_newline = content.ends_with('\n');
    let mut ordinal = 0usize;
    let mut rewritten = content
        .lines()
        .map(|line| {
            if line_already_has_https_markdown_link(line) {
                return line.to_string();
            }
            let Some((indent, marker, rest)) = parse_source_list_line(line) else {
                return line.to_string();
            };
            let cite = resolve_cite(marker, rest, ordinal, cites);
            ordinal += 1;
            let Some(cite) = cite else {
                return line.to_string();
            };
            if !cite.url.starts_with("https://") {
                return line.to_string();
            }
            let display = if rest.trim().is_empty() {
                format!("{}. {}", cite.index, display_title(cite))
            } else {
                format!("{}. {}", cite.index, rest.trim())
            };
            format!("{indent}[{display}]({})", cite.url)
        })
        .collect::<Vec<_>>()
        .join("\n");
    if trailing_newline {
        rewritten.push('\n');
    }
    rewritten
}

fn linkify_inline_markers(content: &str, cites: &[WebCitationLink]) -> String {
    let mut out = String::with_capacity(content.len());
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            let mut j = i + 1;
            let mut label = String::new();
            while j < chars.len() && chars[j] != ']' {
                label.push(chars[j]);
                j += 1;
            }
            if j < chars.len() && chars[j] == ']' {
                let after = j + 1;
                let already_linked = after < chars.len() && chars[after] == '(';
                if !already_linked {
                    if let Some(cite) = resolve_cite(&label, "", usize::MAX, cites) {
                        if cite.url.starts_with("https://") {
                            out.push_str(&format!("[{}]({})", cite.index, cite.url));
                            i = after;
                            continue;
                        }
                    }
                }
                out.push('[');
                out.push_str(&label);
                out.push(']');
                i = after;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn line_already_has_https_markdown_link(line: &str) -> bool {
    line.contains("](https://")
}

fn parse_source_list_line(line: &str) -> Option<(&str, &str, &str)> {
    let trimmed_start = line.trim_start();
    let indent_len = line.len() - trimmed_start.len();
    let indent = &line[..indent_len];
    if !trimmed_start.starts_with('[') {
        return None;
    }
    let close = trimmed_start.find(']')?;
    let marker = &trimmed_start[1..close];
    if marker.is_empty() || marker.contains('[') {
        return None;
    }
    // Source-list markers are short labels: 1, W1, C1, citation:1, etc.
    if marker.chars().count() > 24 {
        return None;
    }
    let rest = trimmed_start[close + 1..].trim_start();
    // Require trailing source text so we do not rewrite lone markers mid-prose
    // that happen to sit alone on a line without a following source name.
    if rest.is_empty() {
        return None;
    }
    // Avoid rewriting Markdown headings / links mistaken for footnotes.
    if rest.starts_with('(') || rest.starts_with('#') {
        return None;
    }
    Some((indent, marker, rest))
}

fn resolve_cite<'a>(
    marker: &str,
    rest: &str,
    ordinal: usize,
    cites: &'a [WebCitationLink],
) -> Option<&'a WebCitationLink> {
    let normalized_marker = normalize_marker_label(marker);
    if let Some(index) = parse_marker_index(&normalized_marker) {
        if let Some(cite) = cites.iter().find(|cite| cite.index == index) {
            return Some(cite);
        }
    }

    let marker_key = normalized_marker
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    if let Some(cite) = cites.iter().find(|cite| {
        cite.label
            .trim_matches(|c| c == '[' || c == ']')
            .eq_ignore_ascii_case(&marker_key)
    }) {
        return Some(cite);
    }

    let rest_l = rest.to_lowercase();
    if !rest_l.is_empty() {
        if let Some(cite) = cites.iter().find(|cite| {
            let title = cite.title.to_lowercase();
            (!title.is_empty() && rest_l.contains(&title))
                || (!title.is_empty() && title.contains(rest_l.split(',').next().unwrap_or("")))
        }) {
            return Some(cite);
        }
    }

    if ordinal < cites.len() {
        return Some(&cites[ordinal]);
    }
    None
}

fn parse_marker_index(marker: &str) -> Option<i64> {
    let trimmed = marker.trim();
    if let Ok(index) = trimmed.parse::<i64>() {
        return (index > 0).then_some(index);
    }
    for prefix in ['W', 'C', 'T', 'F', 'A', 'L', 'V', 'G', 'M'] {
        let rest = trimmed
            .strip_prefix(prefix)
            .or_else(|| trimmed.strip_prefix(prefix.to_ascii_lowercase()));
        if let Some(rest) = rest {
            if let Ok(index) = rest.parse::<i64>() {
                return (index > 0).then_some(index);
            }
        }
    }
    if let Some(rest) = trimmed
        .strip_prefix("citation:")
        .or_else(|| trimmed.strip_prefix("Citation:"))
    {
        if let Ok(index) = rest.parse::<i64>() {
            return (index > 0).then_some(index);
        }
    }
    None
}

fn display_title(cite: &WebCitationLink) -> &str {
    if cite.title.trim().is_empty() {
        cite.url.as_str()
    } else {
        cite.title.as_str()
    }
}

/// Build the persisted `citation_map_json` payload for one assistant message.
pub(crate) fn web_citation_map_json(
    cites: &[WebCitationLink],
    binding: Option<&CitationBinding>,
    source_summary: Option<&crate::ai_runtime::provenance::SourceSummary>,
    attribution: Option<&[crate::ai_runtime::provenance::BlockAttribution]>,
) -> Value {
    let web = cites
        .iter()
        .map(|cite| {
            serde_json::json!({
                "index": cite.index,
                "title": cite.title,
                "url": cite.url,
            })
        })
        .collect::<Vec<_>>();
    let mut map = serde_json::json!({ "web": web });
    if let Some(binding) = binding {
        map["binding"] = serde_json::to_value(binding).unwrap_or(serde_json::Value::Null);
    }
    if let Some(source_summary) = source_summary {
        map["sourceSummary"] =
            serde_json::to_value(source_summary.entries()).unwrap_or(serde_json::Value::Null);
    }
    if let Some(attribution) = attribution {
        map["attribution"] = serde_json::to_value(attribution).unwrap_or(serde_json::Value::Null);
    }
    map
}

/// Parse an optional safe binding projection from persisted citation metadata.
pub(crate) fn parse_web_citation_binding(raw: Option<&str>) -> Option<CitationBinding> {
    let raw = raw.filter(|value| !value.trim().is_empty())?;
    serde_json::from_str::<Value>(raw)
        .ok()?
        .get("binding")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
}

/// Parse the minimal source-category summary persisted alongside citations.
/// Older records simply have no summary.
pub(crate) fn parse_source_summary(
    raw: Option<&str>,
) -> Vec<crate::ai_runtime::provenance::SourceSummaryEntry> {
    let raw = raw.filter(|value| !value.trim().is_empty());
    raw.and_then(|value| serde_json::from_str::<Value>(value).ok())
        .and_then(|value| value.get("sourceSummary").cloned())
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

/// Parse persisted `citation_map_json` into safe UI entries (HTTPS only).
pub(crate) fn parse_web_citation_entries(raw: Option<&str>) -> Vec<WebCitationEntry> {
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    let Some(items) = value.get("web").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let index = object.get("index").and_then(Value::as_i64).unwrap_or(0);
        let title = object
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let url = object
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if index <= 0 || !url.starts_with("https://") {
            continue;
        }
        out.push(WebCitationEntry { index, title, url });
    }
    out.sort_by_key(|entry| entry.index);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cites() -> Vec<WebCitationLink> {
        vec![
            WebCitationLink {
                index: 1,
                label: "[C1]".into(),
                title: "Euronews".into(),
                url: "https://www.euronews.com/a".into(),
            },
            WebCitationLink {
                index: 2,
                label: "[C2]".into(),
                title: "新浪财经".into(),
                url: "https://finance.sina.com.cn/b".into(),
            },
            WebCitationLink {
                index: 3,
                label: "[C3]".into(),
                title: "纽约时报中文网".into(),
                url: "https://cn.nytimes.com/c".into(),
            },
        ]
    }

    #[test]
    fn rewrites_unicode_superscript_source_list_into_https_markdown_links() {
        let input = "参考：\n[¹] Euronews, 2026-07-20\n[²] 新浪财经, 2026-07-21\n[³] 纽约时报中文网, 2026-07-20\n";
        let output = linkify_web_citations(input, &sample_cites());
        assert!(output.contains("[1. Euronews, 2026-07-20](https://www.euronews.com/a)"));
        assert!(output.contains("[2. 新浪财经, 2026-07-21](https://finance.sina.com.cn/b)"));
        assert!(output.contains("[3. 纽约时报中文网, 2026-07-20](https://cn.nytimes.com/c)"));
        assert!(!output.contains('¹'));
    }

    #[test]
    fn leaves_existing_https_markdown_links_untouched() {
        let input = "[1. Euronews](https://www.euronews.com/a)\n";
        let output = linkify_web_citations(input, &sample_cites());
        assert_eq!(output, input);
    }

    #[test]
    fn linkifies_inline_numeric_markers() {
        let input = "据报道 [1] 市场上涨。";
        let output = linkify_web_citations(input, &sample_cites());
        assert!(output.contains("[1](https://www.euronews.com/a)"));
    }

    #[test]
    fn web_citation_map_json_and_parse_round_trip() {
        let cites = sample_cites();
        let binding = CitationBinding {
            mode: CitationBindingMode::Normalized,
            referenced_indices: vec![1, 3],
            fallback_reason: None,
        };
        let json = web_citation_map_json(&cites, Some(&binding), None, None);
        let raw = json.to_string();
        let parsed = parse_web_citation_entries(Some(raw.as_str()));
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].index, 1);
        assert!(parsed[0].url.starts_with("https://"));
        assert_eq!(
            parse_web_citation_binding(Some(raw.as_str())),
            Some(binding)
        );
    }

    #[test]
    fn citation_map_round_trips_only_safe_source_category_counts() {
        let summary = crate::ai_runtime::provenance::SourceSummary::from_counts_for_test(
            std::collections::BTreeMap::from([(
                crate::ai_runtime::provenance::InformationOrigin::WebToolEvidence,
                2,
            )]),
        );
        let raw = web_citation_map_json(&sample_cites(), None, Some(&summary), None).to_string();

        assert_eq!(
            parse_source_summary(Some(raw.as_str())),
            vec![crate::ai_runtime::provenance::SourceSummaryEntry {
                category: "web".to_string(),
                count: 2,
            }]
        );
        assert!(parse_source_summary(None).is_empty());
    }

    #[test]
    fn citation_map_persists_structured_block_attribution_without_source_text() {
        let attribution = vec![crate::ai_runtime::provenance::BlockAttribution {
            block: 1,
            sources: vec!["W1".to_string(), "I".to_string()],
        }];
        let json = web_citation_map_json(&sample_cites(), None, None, Some(&attribution));

        assert_eq!(json["attribution"][0]["block"], 1);
        assert_eq!(json["attribution"][0]["sources"][0], "W1");
        assert!(!json.to_string().contains("excerpt"));
    }

    #[test]
    fn sanitizes_persisted_web_markdown_for_model_history() {
        let citations = vec![WebCitationEntry {
            index: 1,
            title: "Euronews".to_string(),
            url: "https://www.euronews.com/a".to_string(),
        }];

        let sanitized = sanitize_web_citations_for_model_history(
            "历史结论见 [1](https://www.euronews.com/a)。",
            &citations,
        );

        assert_eq!(sanitized, "历史结论见 [历史来源 1]。");
    }

    #[test]
    fn sanitizes_canonical_current_run_markers_for_model_history() {
        let citations = vec![WebCitationEntry {
            index: 1,
            title: "Euronews".to_string(),
            url: "https://www.euronews.com/a".to_string(),
        }];

        let sanitized = sanitize_web_citations_for_model_history("历史结论见 [W1]。", &citations);

        assert_eq!(sanitized, "历史结论见 [历史来源 1]。");
    }

    #[test]
    fn sanitizes_unknown_markers_and_all_historical_urls_without_a_citation_map() {
        let input =
            "历史称 [W9]；见 [文档](https://user.example/a_(b)) 和 https://user.example/raw。";

        let sanitized = sanitize_web_citations_for_model_history(input, &[]);

        assert!(!sanitized.contains("[W9]"));
        assert!(!sanitized.contains("https://"));
        assert!(sanitized.contains("[历史来源]"));
        assert!(sanitized.contains("[历史链接: 文档]"));
    }

    #[test]
    fn binds_known_marker_variants_without_another_model_call() {
        let outcome =
            bind_current_run_citations("证据 [1]、[citation:2]、[¹] 与 [w3]。", &sample_cites());

        assert_eq!(outcome.content, "证据 [W1]、[W2]、[W1] 与 [W3]。");
        assert_eq!(outcome.binding.mode, CitationBindingMode::Normalized);
        assert_eq!(outcome.binding.referenced_indices, vec![1, 2, 3]);
    }

    #[test]
    fn binds_missing_or_unknown_marker_as_a_verified_source_group() {
        let missing = bind_current_run_citations("没有行内标记。", &sample_cites());
        assert_eq!(
            missing.binding.mode,
            CitationBindingMode::SourceGroupFallback
        );
        assert_eq!(
            missing.binding.fallback_reason.as_deref(),
            Some("missing_marker")
        );

        let unknown = bind_current_run_citations("错误标记 [W4]。", &sample_cites());
        assert_eq!(unknown.content, "错误标记 [来源待确认]。");
        assert_eq!(
            unknown.binding.mode,
            CitationBindingMode::SourceGroupFallback
        );
        assert_eq!(
            unknown.binding.fallback_reason.as_deref(),
            Some("unknown_marker")
        );
    }

    #[test]
    fn strict_binding_rejects_answer_level_source_group_fallback() {
        assert_eq!(
            bind_strict_current_run_citations("没有行内标记。", &sample_cites()).unwrap_err(),
            StrictCitationBindingError::MissingPreciseCurrentRunMarkers
        );
    }

    #[test]
    fn source_group_streaming_withholds_partial_model_citation_markers() {
        assert_eq!(
            strip_model_authored_citation_markers_for_stream("结论来自本轮来源 [W"),
            "结论来自本轮来源 "
        );
        assert_eq!(
            strip_model_authored_citation_markers_for_stream("结论来自本轮来源 [W1"),
            "结论来自本轮来源 "
        );
        assert_eq!(
            strip_model_authored_citation_markers_for_stream("结论来自本轮来源 [W1]。"),
            "结论来自本轮来源 。"
        );
        assert_eq!(
            strip_model_authored_citation_markers_for_stream("普通 Markdown [链接"),
            "普通 Markdown [链接"
        );
    }
}
