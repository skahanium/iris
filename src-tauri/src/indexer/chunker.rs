/// Split markdown into chunks at heading and paragraph boundaries (simplified v0.1).
pub fn chunk_markdown(content: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    const MIN_CHARS: usize = 100;

    for line in content.lines() {
        let is_boundary = line.starts_with('#') || line.trim().is_empty();
        if is_boundary && !current.is_empty() && current.chars().count() >= MIN_CHARS {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        if !line.is_empty() || !current.is_empty() {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
        while current.chars().count() > max_chars {
            let split_at = current
                .char_indices()
                .nth(max_chars)
                .map(|(i, _)| i)
                .unwrap_or(current.len());
            let (head, tail) = current.split_at(split_at);
            chunks.push(head.trim().to_string());
            current = tail.to_string();
        }
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    if chunks.is_empty() && !content.trim().is_empty() {
        chunks.push(content.trim().to_string());
    }

    chunks
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
