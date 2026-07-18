//! Shared LLM error sanitization helpers for connectivity probes.

/// 截断错误响应文本，防止大段 HTML/JSON 错误体泄露到前端
pub(crate) fn truncate_error_text(text: &str) -> String {
    const MAX_CHARS: usize = 500;
    let sanitized = sanitize_error_text(text);
    let char_count = sanitized.chars().count();
    if char_count <= MAX_CHARS {
        sanitized
    } else {
        let end = sanitized
            .char_indices()
            .nth(MAX_CHARS)
            .map(|(i, _)| i)
            .unwrap_or(sanitized.len());
        format!("{}…(已截断，共 {} 字符)", &sanitized[..end], char_count)
    }
}

fn sanitize_error_text(text: &str) -> String {
    let re = regex::Regex::new(
        r"(?i)(bearer\s+[a-zA-Z0-9\-._~+/]+=*|sk-[a-zA-Z0-9]{20,}|api[_-]?key[=:]\s*[a-zA-Z0-9\-._~+/]+)",
    )
    .unwrap();
    re.replace_all(text, "[REDACTED]").to_string()
}
