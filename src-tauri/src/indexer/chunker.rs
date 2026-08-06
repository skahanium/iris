//! Split markdown into chunks at heading and paragraph boundaries.
//!
//! Heading boundaries are ignored inside fenced code blocks so FTS/citation spans
//! stay aligned with editor-visible structure.

use super::code_fence::FenceState;

pub fn chunk_markdown(content: &str, max_chars: usize) -> Vec<String> {
    chunk_markdown_with_metadata(content, max_chars)
        .into_iter()
        .map(|chunk| chunk.content)
        .collect()
}

/// Markdown chunk plus citation metadata derived from the source body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownChunk {
    /// Trimmed chunk text stored in the `chunks.content` index column.
    pub content: String,
    /// Markdown heading ancestry at the chunk start, joined by ` > `.
    pub heading_path: Option<String>,
    /// UTF-8 byte start offset into the parsed Markdown body.
    pub source_start: usize,
    /// UTF-8 byte end offset into the parsed Markdown body.
    pub source_end: usize,
    /// Stable hash of the trimmed chunk content.
    pub content_hash: String,
}

/// Split markdown while preserving enough source metadata for stable citations.
pub fn chunk_markdown_with_metadata(content: &str, max_chars: usize) -> Vec<MarkdownChunk> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_start = 0usize;
    let mut current_chars = 0usize;
    let mut current_heading_path: Option<String> = None;
    let mut current_heading_source_end: Option<usize> = None;
    let mut heading_stack: Vec<String> = Vec::new();
    let max_chars = max_chars.max(1);
    const TARGET_CHARS: usize = 320;
    const HARD_MAX_CHARS: usize = 384;
    const OVERLAP_CHARS: usize = 48;
    let target_chars = max_chars.min(TARGET_CHARS);
    let hard_max_chars = max_chars.min(HARD_MAX_CHARS).max(target_chars);
    let overlap_chars = if target_chars > OVERLAP_CHARS {
        OVERLAP_CHARS
    } else {
        0
    };
    let mut fence = FenceState::new();
    let lines = lines_with_offsets(content);

    for (line_index, &(line_start, line, line_with_eol_len)) in lines.iter().enumerate() {
        let in_fence = fence.feed(line);
        let (heading, heading_source_end) = if in_fence {
            (None, None)
        } else if let Some(heading) = parse_heading(line) {
            (Some(heading), Some(line_start + line_with_eol_len))
        } else {
            let setext = lines
                .get(line_index + 1)
                .and_then(|&(next_start, next_line, next_len)| {
                    parse_setext_underline(next_line)
                        .filter(|_| !is_fence_delimiter(line))
                        .map(|level| (level, next_start + next_len))
                })
                .map(|(level, source_end)| ((level, line.trim().to_string()), source_end))
                .filter(|((_, heading), _)| !heading.is_empty());
            match setext {
                Some((heading, source_end)) => (Some(heading), Some(source_end)),
                None => (None, None),
            }
        };

        if heading.is_some()
            || (!in_fence && line.trim().is_empty() && current_chars >= target_chars)
        {
            push_non_empty_trimmed(
                &mut chunks,
                &current,
                current_start,
                current_heading_path.clone(),
            );
            current.clear();
            current_chars = 0;
            current_heading_source_end = None;
        }
        if let Some((level, heading)) = heading {
            heading_stack.truncate(level.saturating_sub(1));
            heading_stack.push(heading);
            current_heading_path = heading_path(&heading_stack);
        }
        if !line.is_empty() || !current.is_empty() {
            if current.is_empty() {
                current_start = line_start;
                current_heading_path = heading_path(&heading_stack);
                current_heading_source_end = heading_source_end;
            }
            let raw_line = &content[line_start..line_start + line_with_eol_len];
            current.push_str(raw_line);
            current_chars += raw_line.chars().count();
        }
        while current_chars > hard_max_chars {
            let trimmed = current.trim_start();
            if trimmed.len() != current.len() {
                current_start += current.len() - trimmed.len();
                current = trimmed.to_string();
                current_chars = current.chars().count();
                if current_chars <= hard_max_chars {
                    break;
                }
            }
            let split_at = byte_index_after_chars(&current, target_chars);
            let (head, tail) = current.split_at(split_at);
            push_non_empty_trimmed(
                &mut chunks,
                head,
                current_start,
                current_heading_path.clone(),
            );
            let mut overlap_start =
                byte_index_after_chars(&current, target_chars.saturating_sub(overlap_chars));
            if let Some(heading_end) = current_heading_source_end {
                let protected_bytes = heading_end.saturating_sub(current_start);
                overlap_start = overlap_start.max(protected_bytes.min(split_at));
            }
            current_start += overlap_start;
            current = format!("{}{}", &current[overlap_start..split_at], tail);
            current_chars = current.chars().count();
            if current_heading_source_end.is_some_and(|heading_end| current_start >= heading_end) {
                current_heading_source_end = None;
            }
        }
        if line_with_eol_len > line.len() && current.is_empty() {
            current_start = line_start + line_with_eol_len;
        }
    }

    push_non_empty_trimmed(
        &mut chunks,
        &current,
        current_start,
        current_heading_path.clone(),
    );

    if chunks.is_empty() && !content.trim().is_empty() {
        let (start, end, trimmed) = trim_offsets(content, 0);
        chunks.push(markdown_chunk(
            trimmed,
            start,
            end,
            heading_path(&heading_stack),
        ));
    }

    chunks
}

fn byte_index_after_chars(text: &str, char_count: usize) -> usize {
    text.char_indices()
        .nth(char_count)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

fn push_non_empty_trimmed(
    chunks: &mut Vec<MarkdownChunk>,
    text: &str,
    source_start: usize,
    heading_path: Option<String>,
) {
    let (start, end, trimmed) = trim_offsets(text, source_start);
    if !trimmed.is_empty() {
        chunks.push(markdown_chunk(trimmed, start, end, heading_path));
    }
}

fn markdown_chunk(
    content: &str,
    source_start: usize,
    source_end: usize,
    heading_path: Option<String>,
) -> MarkdownChunk {
    MarkdownChunk {
        content: content.to_string(),
        heading_path,
        source_start,
        source_end,
        content_hash: crate::cas::hash::content_hash_str(content),
    }
}

fn trim_offsets(text: &str, source_start: usize) -> (usize, usize, &str) {
    let trimmed_start = text.len() - text.trim_start().len();
    let trimmed_end = text.trim_end().len();
    if trimmed_start > trimmed_end {
        return (source_start, source_start, "");
    }
    let trimmed = &text[trimmed_start..trimmed_end];
    (
        source_start + trimmed_start,
        source_start + trimmed_end,
        trimmed,
    )
}

fn parse_heading(line: &str) -> Option<(usize, String)> {
    let leading_spaces = line.bytes().take_while(|byte| *byte == b' ').count();
    if leading_spaces > 3 {
        return None;
    }
    let trimmed = &line[leading_spaces..];
    let level = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    if level == 0 || level > 6 {
        return None;
    }
    if trimmed
        .as_bytes()
        .get(level)
        .is_some_and(|byte| !byte.is_ascii_whitespace())
    {
        return None;
    }
    let rest = trimmed[level..].trim();
    if rest.is_empty() {
        None
    } else {
        Some((level, rest.trim_matches('#').trim().to_string()))
    }
}

fn parse_setext_underline(line: &str) -> Option<usize> {
    let leading_spaces = line.bytes().take_while(|byte| *byte == b' ').count();
    if leading_spaces > 3 {
        return None;
    }
    let marker = line[leading_spaces..].trim_end();
    let first = marker.as_bytes().first().copied()?;
    if !matches!(first, b'=' | b'-') || !marker.bytes().all(|byte| byte == first) {
        return None;
    }
    Some(if first == b'=' { 1 } else { 2 })
}

fn is_fence_delimiter(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

fn heading_path(stack: &[String]) -> Option<String> {
    if stack.is_empty() {
        None
    } else {
        Some(stack.join(" > "))
    }
}

fn lines_with_offsets(content: &str) -> Vec<(usize, &str, usize)> {
    let mut lines = Vec::new();
    let mut offset = 0usize;
    for raw in content.split_inclusive('\n') {
        let without_lf = raw.strip_suffix('\n').unwrap_or(raw);
        let line = without_lf.strip_suffix('\r').unwrap_or(without_lf);
        lines.push((offset, line, raw.len()));
        offset += raw.len();
    }
    if content.is_empty() {
        return lines;
    }
    if !content.ends_with('\n') && lines.is_empty() {
        lines.push((0, content, content.len()));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_heading() {
        let md = "# Title\n\nParagraph one.\n\n## Sub\n\nParagraph two.";
        let chunks = chunk_markdown(md, 512);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn single_paragraph_one_chunk() {
        let chunks = chunk_markdown("Just one paragraph.", 512);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Just one paragraph.");
    }

    #[test]
    fn heading_split_creates_multiple_chunks() {
        let content_a = "Content line A.\n".repeat(20);
        let content_b = "Content line B.\n".repeat(20);
        let md = format!("# One\n\n{}\n\n# Two\n\n{}", content_a, content_b);
        let chunks = chunk_markdown(&md, 512);
        assert!(
            chunks.len() >= 2,
            "should split at headings, got {}",
            chunks.len()
        );
        assert!(chunks[0].contains("# One"));
    }

    #[test]
    fn metadata_tracks_heading_path_and_source_span() {
        let md = "# Root\n\nIntro paragraph.\n\n## Details\n\nAlpha evidence lives here.";
        let chunks = chunk_markdown_with_metadata(md, 512);
        let detail = chunks
            .iter()
            .find(|chunk| chunk.content.contains("Alpha evidence"))
            .expect("detail chunk");

        assert_eq!(detail.heading_path.as_deref(), Some("Root > Details"));
        assert!(detail.source_start < detail.source_end);
        assert_eq!(
            md[detail.source_start..detail.source_end].trim(),
            detail.content
        );
        assert_eq!(detail.content_hash.len(), 64);
    }

    #[test]
    fn max_chars_splits_long_paragraph() {
        let long = "word ".repeat(200);
        let chunks = chunk_markdown(&long, 100);
        assert!(chunks.len() > 1, "long paragraph should be split");
    }

    #[test]
    fn max_chars_split_does_not_emit_empty_chunks_for_leading_whitespace() {
        let chunks = chunk_markdown("   abcdef", 2);
        assert!(!chunks.is_empty());
        assert!(
            chunks.iter().all(|chunk| !chunk.trim().is_empty()),
            "chunks should not contain empty trimmed entries: {chunks:?}"
        );
        assert_eq!(chunks.concat(), "abcdef");
    }

    #[test]
    fn empty_content_returns_empty() {
        let chunks = chunk_markdown("", 512);
        assert!(chunks.is_empty());
    }

    #[test]
    fn ignores_heading_markers_inside_fenced_code() {
        let md = "```python\n# not a heading\nprint('x')\n```\n\n# Real\n\nBody.";
        let chunks = chunk_markdown_with_metadata(md, 512);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].heading_path, None);
        assert_eq!(chunks[1].heading_path.as_deref(), Some("Real"));
    }

    #[test]
    fn whitespace_only_returns_empty() {
        let chunks = chunk_markdown("   \n\n  ", 512);
        assert!(chunks.is_empty());
    }

    #[test]
    fn heading_flushes_a_short_preamble_before_updating_heading_path() {
        let chunks = chunk_markdown_with_metadata("brief\n# Next\nbody", 512);
        assert_eq!(chunks[0].content, "brief");
        assert_eq!(chunks[0].heading_path, None);
        assert_eq!(chunks[1].heading_path.as_deref(), Some("Next"));
    }

    #[test]
    fn setext_heading_starts_a_new_chunk_outside_fences() {
        let chunks = chunk_markdown_with_metadata("Intro\n=====\n\nBody", 512);
        assert_eq!(chunks[0].heading_path.as_deref(), Some("Intro"));
    }

    #[test]
    fn atx_heading_accepts_at_most_three_leading_spaces() {
        let chunks = chunk_markdown_with_metadata("   # Three\nbody\n    # Four\nmore", 512);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.heading_path.as_deref() == Some("Three")));
    }

    #[test]
    fn setext_markers_inside_fences_do_not_change_heading_path() {
        let chunks = chunk_markdown_with_metadata("```\nNot a heading\n=====\n```", 512);
        assert!(chunks.iter().all(|chunk| chunk.heading_path.is_none()));
    }

    #[test]
    fn internal_policy_keeps_hard_splits_below_384_chars_with_bounded_overlap() {
        let content = "a".repeat(500);
        let chunks = chunk_markdown_with_metadata(&content, 2_000);

        assert_eq!(chunks.len(), 2);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.content.chars().count() <= 384));
        assert_eq!(chunks[0].source_end - chunks[1].source_start, 48);
        assert_eq!(
            &content[chunks[1].source_start..chunks[0].source_end],
            "a".repeat(48)
        );
    }

    #[test]
    fn hard_split_overlap_does_not_repeat_the_heading_line() {
        let content = format!("# Heading\n{}", "a".repeat(400));
        let chunks = chunk_markdown_with_metadata(&content, 2_000);

        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].content.starts_with("# Heading"));
        assert!(!chunks[1].content.contains("# Heading"));
    }

    #[test]
    fn crlf_source_spans_slice_the_exact_original_text() {
        let content = "前言\r\n# 标题\r\n正文";
        let chunks = chunk_markdown_with_metadata(content, 512);

        for chunk in chunks {
            assert_eq!(
                &content[chunk.source_start..chunk.source_end],
                chunk.content
            );
        }
    }

    #[test]
    fn crlf_multibyte_source_spans_remain_byte_accurate() {
        let content = "甲乙\r\n丙丁\r\n戊己";
        let chunks = chunk_markdown_with_metadata(content, 512);

        assert_eq!(chunks.len(), 1);
        assert_eq!(
            &content[chunks[0].source_start..chunks[0].source_end],
            chunks[0].content
        );
    }

    #[test]
    fn crlf_hard_split_source_spans_remain_byte_accurate() {
        let content = format!("# 标题\r\n{}", "界".repeat(500));
        let chunks = chunk_markdown_with_metadata(&content, 2_000);

        assert!(chunks.len() > 1);
        for chunk in chunks {
            assert_eq!(
                &content[chunk.source_start..chunk.source_end],
                chunk.content
            );
        }
    }

    #[test]
    fn hard_split_does_not_overlap_a_long_atx_heading() {
        let content = format!("# {}\nbody", "heading".repeat(80));
        let chunks = chunk_markdown_with_metadata(&content, 2_000);

        assert!(chunks.len() > 1);
        assert_eq!(chunks[0].source_end, chunks[1].source_start);
    }

    #[test]
    fn hard_split_does_not_overlap_a_long_setext_heading() {
        let content = format!("{}\n=====\nbody", "heading".repeat(80));
        let chunks = chunk_markdown_with_metadata(&content, 2_000);

        assert!(chunks.len() > 1);
        assert_eq!(chunks[0].source_end, chunks[1].source_start);
    }
}
