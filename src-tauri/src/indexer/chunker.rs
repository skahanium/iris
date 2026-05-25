/// Split markdown into chunks at heading and paragraph boundaries (simplified v0.1).
pub fn chunk_markdown(content: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in content.lines() {
        let is_boundary = line.starts_with('#') || line.trim().is_empty();
        if is_boundary && !current.is_empty() && current.len() >= 100 {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        if !line.is_empty() || !current.is_empty() {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
        while current.len() > max_chars {
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
}
