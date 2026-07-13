//! Safe diagnostic redaction for AI runtime boundaries.

/// Redact classified paths, document title metadata, suspicious tokens, API keys,
/// and file path leaks from diagnostic strings.
///
/// Strips:
/// - `.classified/` path segments
/// - `title`, `document`, `document_title`, and `note_title` key/value pairs
/// - Content following the `涉密` marker through end of line
/// - Long base64-looking tokens (40+ consecutive base64 chars)
/// - API key patterns (`sk-*`, `key=*`, `token=*`, `Bearer *`)
/// - Absolute file paths (`/Users/...`, `/home/...`, `/tmp/...`)
pub fn redact_classified_leaks(input: &str) -> String {
    let mut out = input.to_string();

    // 1. Redact .classified/ path segments
    while let Some(start) = out.find(".classified/") {
        let end = out[start..]
            .find(['/', '"', '\'', ' ', '\n'])
            .map(|p| start + p)
            .unwrap_or(out.len());
        out.replace_range(start..end, "[REDACTED]");
    }

    // 2. Redact content after 涉密 markers through end-of-line
    while let Some(marker_start) = out.find("涉密") {
        let after_marker = marker_start + "涉密".len();
        let line_end = out[after_marker..]
            .find('\n')
            .map(|p| after_marker + p)
            .unwrap_or(out.len());
        out.replace_range(marker_start..line_end, "[REDACTED]");
    }

    // 3. Redact explicit title/document metadata fields that may carry
    // classified document names in provider or tool errors.
    let metadata_keys = ["title", "document", "document_title", "note_title"];
    for key in metadata_keys {
        for marker in [format!("\"{key}\":"), format!("{key}:")] {
            while let Some(start) = out.find(&marker) {
                let value_start = start + marker.len();
                let end = out[value_start..]
                    .find([',', '\n', '}', ']'])
                    .map(|p| value_start + p)
                    .unwrap_or(out.len());
                out.replace_range(start..end, &format!("{key}:\"[REDACTED]\""));
            }
        }
    }

    // 4. Redact long base64-looking tokens (40+ consecutive base64 chars)
    //    Scan byte-by-byte, tracking runs of [A-Za-z0-9+/=].
    let mut result = String::with_capacity(out.len());
    let mut run_start: Option<usize> = None;
    for (byte_idx, ch) in out.char_indices() {
        if ch.is_ascii_alphanumeric() || ch == '+' || ch == '/' || ch == '=' {
            if run_start.is_none() {
                run_start = Some(byte_idx);
            }
        } else {
            if let Some(start) = run_start {
                if byte_idx - start >= 40 {
                    result.push_str("[REDACTED:TOKEN]");
                } else {
                    result.push_str(&out[start..byte_idx]);
                }
            }
            result.push(ch);
            run_start = None;
        }
    }
    // Flush any trailing run
    if let Some(start) = run_start {
        let end = out.len();
        if end - start >= 40 {
            result.push_str("[REDACTED:TOKEN]");
        } else {
            result.push_str(&out[start..end]);
        }
    }
    out = result;

    // 5. Redact API key patterns: sk-*, key=VALUE, token=VALUE
    let api_prefixes: &[&str] = &["sk-", "key=", "token=", "secret="];
    for prefix in api_prefixes {
        while let Some(start) = out.find(prefix) {
            let val_start = start + prefix.len();
            let end = out[val_start..]
                .find(|c: char| {
                    c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == '}' || c == ']'
                })
                .map(|p| val_start + p)
                .unwrap_or(out.len());
            // Only redact if value is long enough to look like a real secret
            if end - val_start >= 16 {
                out.replace_range(start..end, "[REDACTED:SECRET]");
            } else {
                break;
            }
        }
    }

    // 6. Redact Bearer tokens
    while let Some(start) = out.find("Bearer ") {
        let val_start = start + "Bearer ".len();
        let end = out[val_start..]
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == '}')
            .map(|p| val_start + p)
            .unwrap_or(out.len());
        if end - val_start >= 16 {
            out.replace_range(start..end, "[REDACTED:TOKEN]");
        } else {
            break;
        }
    }

    // 7. Redact absolute file paths (Unix-style: /Users/..., /home/..., /tmp/...)
    let path_prefixes: &[&str] = &["/Users/", "/home/", "/tmp/", "/var/", "/opt/"];
    for prefix in path_prefixes {
        while let Some(start) = out.find(prefix) {
            let end = out[start..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '\n')
                .map(|p| start + p)
                .unwrap_or(out.len());
            out.replace_range(start..end, "[REDACTED:PATH]");
        }
    }

    out
}
