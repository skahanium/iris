//! Format-preservation content check (C23 / N04).
//!
//! This module proves — or refuses to prove — that a format-only candidate kept
//! the same prose information, block order, and link targets. It is not a full
//! CommonMark parser and does not treat T25 `normalize_markdown` as a proof.
//!
//! Allowed changes (this wave only):
//! - `StructuralWhitespace`: CRLF, trailing ASCII space/tab, folding consecutive
//!   blank lines outside fences (same shape as T25).
//! - `ListMarkerAndIndex`: a leading `[-*+]` or `\d+\.` plus following whitespace.
//! - ATX heading `#` count. Heading prose still counts as `body_text`.
//!
//! Must compare, must not strip:
//! - Prose punctuation and digits (`don't` ≠ `dont`, `3.14` ≠ `314`).
//! - Fenced code is opaque: inner text must match exactly. An unclosed fence
//!   marks related dimensions `unknown`, never `passed`.
//! - Link targets in document order: `[text](url)` and `[[wikilink]]`. Labels
//!   and display text must both remain unchanged. Code is opaque.
//! - Block order: non-empty content units outside fences, after stripping the
//!   allowed structural prefixes, must stay in the same sequence.
//!
//! Constructs the line scan cannot decide (HTML blocks, tables, fullwidth
//! spaces, zero-width characters) are `unknown`. The gate predicate is only
//! the format-preservation phrases below — polish / translate / rewrite stay
//! out so they can still change wording.

use crate::ai_runtime::ToolCallResult;

const FORMAT_PRESERVATION_UNPROVEN: &str = "format_preservation_unproven";

const FORMAT_REQUEST_MARKERS: &[&str] = &[
    "格式整理",
    "整理格式",
    "格式规范化",
    "规范化格式",
    "format this note",
    "normalize the markdown formatting",
];

/// K16 four-state item status. Unknown is never treated as passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReportStatus {
    Passed,
    Failed,
    Unknown,
    #[allow(
        dead_code,
        reason = "K16 four-state; this wave scores body/order/links instead of marking not-applicable"
    )]
    NotApplicable,
}

impl ReportStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
            Self::NotApplicable => "not-applicable",
        }
    }

    fn allows_proof(self) -> bool {
        matches!(self, Self::Passed | Self::NotApplicable)
    }
}

/// Per-dimension format-preservation report. Each item has denominator 1 unless
/// that dimension is `not-applicable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContentPreservationReport {
    pub(crate) body_text: ReportStatus,
    pub(crate) block_order: ReportStatus,
    pub(crate) link_targets: ReportStatus,
}

impl ContentPreservationReport {
    fn unknown() -> Self {
        Self {
            body_text: ReportStatus::Unknown,
            block_order: ReportStatus::Unknown,
            link_targets: ReportStatus::Unknown,
        }
    }

    #[cfg(test)]
    fn failed_body() -> Self {
        Self {
            body_text: ReportStatus::Failed,
            block_order: ReportStatus::Failed,
            link_targets: ReportStatus::Failed,
        }
    }

    #[cfg(test)]
    fn proven_vacuous() -> Self {
        Self {
            body_text: ReportStatus::Passed,
            block_order: ReportStatus::Passed,
            link_targets: ReportStatus::Passed,
        }
    }

    /// True only when every dimension is `passed` or `not-applicable`.
    pub(crate) fn is_proven(&self) -> bool {
        self.body_text.allows_proof()
            && self.block_order.allows_proof()
            && self.link_targets.allows_proof()
    }
}

/// Whether the user asked for format cleanup rather than polish / rewrite.
pub(crate) fn is_format_preservation_request(message: &str) -> bool {
    contains_any(message, FORMAT_REQUEST_MARKERS)
}

/// Compare `original` and `candidate` under the format-preservation contract.
pub(crate) fn check_format_preservation(
    original: &str,
    candidate: &str,
) -> ContentPreservationReport {
    let left = scan_document(original);
    let right = scan_document(candidate);
    if left.unknown || right.unknown {
        return ContentPreservationReport::unknown();
    }

    let body = if same_bag(&left.prose, &right.prose) && same_bag(&left.fences, &right.fences) {
        ReportStatus::Passed
    } else {
        ReportStatus::Failed
    };
    let order = if left.blocks == right.blocks {
        ReportStatus::Passed
    } else {
        ReportStatus::Failed
    };
    let links = if left.link_targets == right.link_targets {
        ReportStatus::Passed
    } else {
        ReportStatus::Failed
    };
    ContentPreservationReport {
        body_text: body,
        block_order: order,
        link_targets: links,
    }
}

/// Block frozen confirmation when a format-preservation write is unproven.
///
/// Returns `None` when the gate does not apply or the candidate is proven, so
/// the caller may still request confirmation. Never writes files.
#[cfg(test)]
pub(crate) fn blocked_format_write_result(
    user_message: &str,
    tool_name: &str,
    args: &serde_json::Value,
) -> Option<ToolCallResult> {
    if !is_format_preservation_request(user_message) {
        return None;
    }
    let report = match tool_name {
        "replace_selection" => match (
            string_arg(args, &["original_text", "selection"]),
            string_arg(args, &["replacement"]),
        ) {
            (Some(original), Some(replacement)) => check_format_preservation(original, replacement),
            _ => ContentPreservationReport::unknown(),
        },
        "insert_text_at_cursor" => {
            let text = string_arg(args, &["text"]).unwrap_or("");
            if text.is_empty() {
                ContentPreservationReport::proven_vacuous()
            } else {
                ContentPreservationReport::failed_body()
            }
        }
        _ => return None,
    };
    if report.is_proven() {
        None
    } else {
        Some(unproven_tool_result(tool_name, &report))
    }
}

pub(crate) fn unproven_tool_result(
    tool_name: &str,
    report: &ContentPreservationReport,
) -> ToolCallResult {
    ToolCallResult {
        tool_name: tool_name.to_string(),
        success: false,
        output: serde_json::json!({
            "error": FORMAT_PRESERVATION_UNPROVEN,
            "bodyText": report.body_text.as_str(),
            "blockOrder": report.block_order.as_str(),
            "linkTargets": report.link_targets.as_str(),
        }),
        duration_ms: 0,
        tokens_used: None,
        error: Some(FORMAT_PRESERVATION_UNPROVEN.to_string()),
    }
}

#[cfg(test)]
fn string_arg<'a>(args: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| args.get(*key).and_then(serde_json::Value::as_str))
}

fn contains_any(message: &str, markers: &[&str]) -> bool {
    let lowered = message.to_lowercase();
    markers
        .iter()
        .any(|marker| lowered.contains(&marker.to_lowercase()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Block {
    Prose(String),
    Fence(String),
    Blank,
}

struct ScannedDocument {
    unknown: bool,
    blocks: Vec<Block>,
    prose: Vec<String>,
    fences: Vec<String>,
    link_targets: Vec<String>,
}

fn scan_document(text: &str) -> ScannedDocument {
    if has_undecidable_space(text) {
        return ScannedDocument {
            unknown: true,
            blocks: Vec::new(),
            prose: Vec::new(),
            fences: Vec::new(),
            link_targets: Vec::new(),
        };
    }

    let mut unknown = false;
    let mut fence: Option<(char, usize)> = None;
    let mut fence_buf = String::new();
    let mut blocks = Vec::new();
    let mut link_targets = Vec::new();

    for raw_line in text.split_inclusive('\n') {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some((marker, marker_len)) = fence {
            if let Some((close_marker, close_len)) = markdown_fence_marker(line) {
                let tail = line.trim_start_matches(' ').get(close_len..).unwrap_or("");
                if close_marker == marker
                    && close_len >= marker_len
                    && tail.trim_matches([' ', '\t']).is_empty()
                {
                    // Delimiters are retained too: changing the language/info
                    // or fence spelling is outside this wave's allowed set.
                    fence_buf.push_str(line);
                    blocks.push(Block::Fence(std::mem::take(&mut fence_buf)));
                    fence = None;
                    continue;
                }
            }
            fence_buf.push_str(raw_line);
            continue;
        }

        if let Some(marker) = markdown_fence_marker(line) {
            let info = &line.trim_start_matches(' ')[marker.1..];
            if marker.0 == '`' && info.contains('`') {
                unknown = true;
            }
            fence = Some(marker);
            fence_buf.clear();
            fence_buf.push_str(line);
            fence_buf.push('\n');
            continue;
        }
        if line.contains('\r')
            || looks_like_html_line(line)
            || looks_like_table_line(line)
            || line.starts_with([' ', '\t'])
            || undecidable_inline_code(line)
            || unsupported_block_delimiter(line)
        {
            unknown = true;
        }
        let trimmed = trim_ascii_end(line);
        if trimmed.is_empty() {
            if !blocks.is_empty() && !matches!(blocks.last(), Some(Block::Blank)) {
                blocks.push(Block::Blank);
            }
            continue;
        }
        let (_, targets) = extract_links(trimmed);
        link_targets.extend(targets);
        // Preserve the complete link spelling and labels, including anything
        // resembling a link inside inline code. Only block prefixes may vary.
        let unit = strip_structural_prefix(trimmed);
        let kind = if unit.len() == trimmed.len() {
            "prose:"
        } else if trimmed.starts_with('#') {
            "heading:"
        } else {
            "list:"
        };
        if kind == "list:"
            && matches!(blocks.last(), Some(Block::Prose(previous)) if previous.starts_with("prose:"))
        {
            // Whether a numbered marker interrupts an existing paragraph
            // depends on its index. Do not erase that semantic distinction.
            unknown = true;
        }
        blocks.push(Block::Prose(format!("{kind}{unit}")));
    }

    let unclosed = fence.is_some();
    let mut prose = Vec::new();
    let mut fences = Vec::new();
    for block in &blocks {
        match block {
            Block::Prose(text) => prose.push(text.clone()),
            Block::Fence(text) => fences.push(text.clone()),
            Block::Blank => {}
        }
    }

    ScannedDocument {
        unknown: unknown || unclosed,
        blocks,
        prose,
        fences,
        link_targets,
    }
}

fn same_bag(left: &[String], right: &[String]) -> bool {
    let mut a = left.to_vec();
    let mut b = right.to_vec();
    a.sort();
    b.sort();
    a == b
}

fn has_undecidable_space(text: &str) -> bool {
    text.chars().any(|ch| {
        matches!(
            ch,
            '\u{3000}' | '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' | '\u{2060}'
        )
    })
}

fn trim_ascii_end(line: &str) -> &str {
    line.trim_end_matches([' ', '\t'])
}

fn looks_like_html_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let mut chars = trimmed.chars();
    if chars.next() != Some('<') {
        return false;
    }
    matches!(
        chars.next(),
        Some(ch) if ch.is_ascii_alphabetic() || matches!(ch, '/' | '!' | '?')
    )
}

fn looks_like_table_line(line: &str) -> bool {
    // Tables need not start with '|'. Ignore only recognized link spellings;
    // their complete raw body is still compared above.
    extract_links(line).0.contains('|')
}

fn unsupported_block_delimiter(line: &str) -> bool {
    let compact = line
        .chars()
        .filter(|ch| !matches!(ch, ' ' | '\t'))
        .collect::<String>();
    ['-', '*', '_', '=']
        .iter()
        .any(|marker| compact.len() >= 3 && compact.chars().all(|ch| ch == *marker))
}

fn undecidable_inline_code(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut index = 0;
    let mut open = None;
    while index < bytes.len() {
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index] == b'`' {
            index += 1;
        }
        let len = index - start;
        match open {
            None => open = Some(len),
            Some(count) if count == len => open = None,
            _ => {}
        }
    }
    open.is_some()
}

fn markdown_fence_marker(line: &str) -> Option<(char, usize)> {
    let leading_spaces = line.chars().take_while(|ch| *ch == ' ').count();
    if leading_spaces > 3 {
        return None;
    }
    let rest = &line[leading_spaces..];
    let marker = rest.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let len = rest.chars().take_while(|ch| *ch == marker).count();
    (len >= 3).then_some((marker, len))
}

fn strip_structural_prefix(line: &str) -> &str {
    let mut indent = 0usize;
    for ch in line.chars() {
        if ch == ' ' && indent < 3 {
            indent += 1;
        } else {
            break;
        }
    }
    let rest = &line[indent..];

    let hashes = rest.chars().take_while(|ch| *ch == '#').count();
    if (1..=6).contains(&hashes) {
        let after = &rest[hashes..];
        if after.is_empty() || after.starts_with(' ') || after.starts_with('\t') {
            return after.trim_start_matches([' ', '\t']);
        }
    }

    if let Some(marker) = rest.chars().next() {
        if matches!(marker, '-' | '*' | '+') {
            let after = &rest[marker.len_utf8()..];
            if after.starts_with(' ') || after.starts_with('\t') {
                return after.trim_start_matches([' ', '\t']);
            }
        }
    }

    let digits = rest.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if (1..=9).contains(&digits) {
        let after_digits = &rest[digits..];
        if let Some(stripped) = after_digits.strip_prefix('.') {
            if stripped.is_empty() || stripped.starts_with(' ') || stripped.starts_with('\t') {
                return stripped.trim_start_matches([' ', '\t']);
            }
        }
    }

    line
}

fn extract_links(line: &str) -> (String, Vec<String>) {
    let chars: Vec<char> = line.chars().collect();
    let mut masked = String::new();
    let mut targets = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] == '[' && chars.get(index + 1) == Some(&'[') {
            if let Some((end, target)) = parse_wikilink(&chars, index) {
                targets.push(target.clone());
                masked.push('⟦');
                masked.push_str(&target);
                masked.push('⟧');
                index = end;
                continue;
            }
        }
        if chars[index] == '[' {
            if let Some((end, target)) = parse_markdown_link(&chars, index) {
                targets.push(target.clone());
                masked.push('⟦');
                masked.push_str(&target);
                masked.push('⟧');
                index = end;
                continue;
            }
        }
        masked.push(chars[index]);
        index += 1;
    }
    (masked, targets)
}

fn parse_wikilink(chars: &[char], start: usize) -> Option<(usize, String)> {
    let inner_start = start + 2;
    let mut cursor = inner_start;
    while cursor + 1 < chars.len() {
        if chars[cursor] == ']' && chars[cursor + 1] == ']' {
            let inner: String = chars[inner_start..cursor].iter().collect();
            let target = inner.split('|').next().unwrap_or("").to_string();
            return Some((cursor + 2, target));
        }
        cursor += 1;
    }
    None
}

fn parse_markdown_link(chars: &[char], start: usize) -> Option<(usize, String)> {
    let mut cursor = start + 1;
    while cursor + 1 < chars.len() {
        if chars[cursor] == ']' && chars[cursor + 1] == '(' {
            let url_start = cursor + 2;
            let mut url_end = url_start;
            while url_end < chars.len() && chars[url_end] != ')' {
                url_end += 1;
            }
            if url_end >= chars.len() {
                return None;
            }
            let url: String = chars[url_start..url_end].iter().collect();
            return Some((url_end + 1, url));
        }
        if chars[cursor] == '[' {
            return None;
        }
        cursor += 1;
    }
    None
}
