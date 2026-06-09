use rusqlite::Connection;

use crate::error::AppResult;

/// Detect CJK character ranges (CJK Unified Ideographs, Extension A, etc.).
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}'   // CJK Extension A
        | '\u{F900}'..='\u{FAFF}'   // CJK Compatibility Ideographs
        | '\u{2F800}'..='\u{2FA1F}' // CJK Compatibility Supplement
    )
}

/// Generate CJK bigrams from text for FTS5 indexing.
///
/// Converts CJK runs like "你好世界" into space-separated bigrams
/// ("你好 好世 世界") so that SQLite's unicode61 tokenizer produces
/// word-level matches instead of single-character tokens.
fn cjk_bigrams(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if is_cjk(chars[i]) {
            let start = i;
            while i < chars.len() && is_cjk(chars[i]) {
                i += 1;
            }
            let cjk_run = &chars[start..i];
            // Overlapping bigrams
            for w in cjk_run.windows(2) {
                out.push(w[0]);
                out.push(w[1]);
                out.push(' ');
            }
            // Also include individual chars for single-char search
            for &c in cjk_run {
                out.push(c);
                out.push(' ');
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Upsert a row into FTS5 shadow content with CJK bigram augmentation.
pub fn upsert_fts(conn: &Connection, path: &str, title: &str, content: &str) -> AppResult<()> {
    conn.execute("DELETE FROM files_fts WHERE path = ?1", [path])?;
    // Prepend CJK bigrams so unicode61 tokenizer handles Chinese/Japanese/Korean
    let cjk = cjk_bigrams(content);
    let fts_content = if cjk.len() > content.len() {
        format!("{} {}", cjk, content)
    } else {
        content.to_string()
    };
    conn.execute(
        "INSERT INTO files_fts (path, title, content) VALUES (?1, ?2, ?3)",
        rusqlite::params![path, title, fts_content],
    )?;
    Ok(())
}

/// Remove FTS entry for a path.
pub fn delete_fts(conn: &Connection, path: &str) -> AppResult<()> {
    conn.execute("DELETE FROM files_fts WHERE path = ?1", [path])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_bigrams_empty() {
        assert_eq!(cjk_bigrams(""), "");
    }

    #[test]
    fn cjk_bigrams_ascii_only() {
        assert_eq!(cjk_bigrams("hello world"), "hello world");
    }

    #[test]
    fn cjk_bigrams_two_chars() {
        // "你好" -> bigram "你好" + individual "你 好"
        let result = cjk_bigrams("你好");
        assert!(result.contains("你好"));
        assert!(result.contains("你"));
        assert!(result.contains("好"));
    }

    #[test]
    fn cjk_bigrams_mixed() {
        let result = cjk_bigrams("hello 你好 world");
        assert!(result.contains("hello"));
        assert!(result.contains("你好"));
        assert!(result.contains("world"));
    }

    #[test]
    fn is_cjk_basic() {
        assert!(is_cjk('中'));
        assert!(is_cjk('文'));
        assert!(!is_cjk('a'));
        assert!(!is_cjk('1'));
        assert!(!is_cjk(' '));
    }
}
