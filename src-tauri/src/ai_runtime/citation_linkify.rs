//! Pure helpers that turn bare web footnotes into Markdown HTTPS links.

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
}
