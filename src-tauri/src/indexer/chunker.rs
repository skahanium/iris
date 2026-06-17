/// Split markdown into chunks at heading and paragraph boundaries (simplified v0.1).
pub fn chunk_markdown(content: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_chars = 0usize;
    let max_chars = max_chars.max(1);
    const MIN_CHARS: usize = 100;

    for line in content.lines() {
        let is_boundary = line.starts_with('#') || line.trim().is_empty();
        if is_boundary && !current.is_empty() && current_chars >= MIN_CHARS {
            push_non_empty_trimmed(&mut chunks, &current);
            current.clear();
            current_chars = 0;
        }
        if !line.is_empty() || !current.is_empty() {
            if !current.is_empty() {
                current.push('\n');
                current_chars += 1;
            }
            current.push_str(line);
            current_chars += line.chars().count();
        }
        while current_chars > max_chars {
            let trimmed = current.trim_start();
            if trimmed.len() != current.len() {
                current = trimmed.to_string();
                current_chars = current.chars().count();
                if current_chars <= max_chars {
                    break;
                }
            }
            let split_at = byte_index_after_chars(&current, max_chars);
            let (head, tail) = current.split_at(split_at);
            push_non_empty_trimmed(&mut chunks, head);
            current = tail.to_string();
            current_chars = current_chars.saturating_sub(max_chars);
        }
    }

    push_non_empty_trimmed(&mut chunks, &current);

    if chunks.is_empty() && !content.trim().is_empty() {
        chunks.push(content.trim().to_string());
    }

    chunks
}

fn byte_index_after_chars(text: &str, char_count: usize) -> usize {
    text.char_indices()
        .nth(char_count)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

fn push_non_empty_trimmed(chunks: &mut Vec<String>, text: &str) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        chunks.push(trimmed.to_string());
    }
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
    fn whitespace_only_returns_empty() {
        let chunks = chunk_markdown("   \n\n  ", 512);
        assert!(chunks.is_empty());
    }
}
