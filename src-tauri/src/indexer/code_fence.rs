/// Lightweight code-fence state machine for Markdown content.
///
/// Tracks whether the current line is inside a fenced code block (``` or ~~~),
/// inline code (single backtick), or HTML comment.
pub struct FenceState {
    in_fence: bool,
    fence_char: char, // '`' or '~'
    fence_len: usize, // 3 or more
}

impl FenceState {
    pub(crate) fn new() -> Self {
        Self {
            in_fence: false,
            fence_char: '`',
            fence_len: 0,
        }
    }

    /// Feed one line and update fence state.
    /// Returns `true` if the line is inside a code block after processing.
    pub fn feed(&mut self, line: &str) -> bool {
        let trimmed = line.trim();

        if self.in_fence {
            // Check for closing fence: same char, at least same length, only whitespace after
            if let Some(stripped) =
                trimmed.strip_prefix(&self.fence_char.to_string().repeat(self.fence_len))
            {
                if stripped.is_empty() || stripped.chars().all(|c| c.is_whitespace()) {
                    self.in_fence = false;
                    return false;
                }
            }
            return true;
        }

        // Check for opening fence
        for (ch, min_len) in &[('`', 3), ('~', 3)] {
            let prefix: String = std::iter::repeat_n(*ch, *min_len).collect();
            if let Some(rest) = trimmed.strip_prefix(&prefix) {
                let count = prefix.len() + rest.chars().take_while(|c| *c == *ch).count();
                let after: String = trimmed.chars().skip(count).collect();
                let after = after.trim();
                // It's an opening fence if after is empty or purely informational (language tag)
                if after.is_empty() || !after.contains(*ch) {
                    self.in_fence = true;
                    self.fence_char = *ch;
                    self.fence_len = count;
                    return true;
                }
            }
        }

        false
    }

    /// Check whether a byte position within a line is inside inline code or an HTML comment.
    /// `pos` is a byte offset into the line.
    pub fn is_inside_inline_code_or_comment(line: &str, pos: usize) -> bool {
        let bytes = line.as_bytes();
        if pos >= bytes.len() {
            return false;
        }

        // Check inline code (backtick spans)
        let backtick_count = bytes[..=pos].iter().filter(|&&b| b == b'`').count();
        if backtick_count % 2 == 1 {
            return true;
        }

        // Check HTML comment — scan full line for <!-- --> pairs
        is_inside_html_comment(bytes, pos)
    }
}

/// Scan `bytes` for `<!-- ... -->` spans; return true if `pos` falls inside one.
fn is_inside_html_comment(bytes: &[u8], pos: usize) -> bool {
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if &bytes[i..i + 4] == b"<!--" {
            let start = i;
            let mut closed = false;
            for j in i + 4..bytes.len() {
                if j + 3 <= bytes.len() && &bytes[j..j + 3] == b"-->" {
                    let end = j + 3;
                    if pos >= start && pos < end {
                        return true;
                    }
                    closed = true;
                    i = end;
                    break;
                }
            }
            if !closed && pos >= start {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_state_tracks_basic_fence() {
        let mut fs = FenceState::new();
        assert!(!fs.feed("normal text"));
        assert!(fs.feed("```"));
        assert!(fs.feed("code inside"));
        assert!(!fs.feed("```"));
        assert!(!fs.feed("normal again"));
    }

    #[test]
    fn fence_state_handles_tilde_fence() {
        let mut fs = FenceState::new();
        assert!(!fs.feed("text"));
        assert!(fs.feed("~~~python"));
        assert!(fs.feed("code"));
        assert!(!fs.feed("~~~"));
    }

    #[test]
    fn fence_state_ignores_short_backticks() {
        let mut fs = FenceState::new();
        assert!(!fs.feed("``not a fence"));
        assert!(!fs.feed("`inline code`"));
    }

    #[test]
    fn fence_state_language_tag() {
        let mut fs = FenceState::new();
        assert!(fs.feed("```rust"));
        assert!(fs.feed("fn main() {}"));
        assert!(!fs.feed("```"));
    }

    #[test]
    fn inline_code_detection() {
        let line = "text `code` more text";
        // pos before backtick: false, inside: true, after close: false
        assert!(!FenceState::is_inside_inline_code_or_comment(line, 4));
        assert!(FenceState::is_inside_inline_code_or_comment(line, 6));
        assert!(!FenceState::is_inside_inline_code_or_comment(line, 12));
    }

    #[test]
    fn html_comment_detection() {
        let line = "text <!-- comment --> after";
        assert!(!FenceState::is_inside_inline_code_or_comment(line, 4));
        assert!(FenceState::is_inside_inline_code_or_comment(line, 8));
        assert!(!FenceState::is_inside_inline_code_or_comment(line, 22));
    }
}
