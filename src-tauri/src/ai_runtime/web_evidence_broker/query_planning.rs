//! Legacy diagnostic query planning. Frozen production actions use the approved query once.

pub(crate) fn plan_search_queries(raw_query: &str) -> Vec<String> {
    let sanitized = sanitize_search_query(raw_query);
    if sanitized.is_empty() {
        return Vec::new();
    }
    let mut planned = Vec::new();
    push_unique_query(&mut planned, sanitized.clone());

    for segment in sanitized.split(['。', '？', '?', '！', '!', '\n', ';', '；']) {
        let segment = segment.trim();
        if segment.len() >= 4 {
            push_unique_query(&mut planned, truncate_query(segment, 120));
        }
        if planned.len() >= 3 {
            break;
        }
    }

    if planned.len() < 3 {
        let keywords = keyword_query(&sanitized);
        if !keywords.is_empty() {
            push_unique_query(&mut planned, keywords);
        }
    }

    planned.truncate(3);
    planned
}

fn push_unique_query(planned: &mut Vec<String>, query: String) {
    let normalized = normalize_query_whitespace(&query);
    if normalized.is_empty() {
        return;
    }
    if !planned
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&normalized))
    {
        planned.push(normalized);
    }
}

fn sanitize_search_query(raw_query: &str) -> String {
    let redacted_digits = redact_long_digit_runs(raw_query);
    let kept_tokens = redacted_digits
        .split_whitespace()
        .filter(|token| !is_sensitive_query_token(token))
        .collect::<Vec<_>>()
        .join(" ");
    truncate_query(&normalize_query_whitespace(&kept_tokens), 160)
}

fn redact_long_digit_runs(input: &str) -> String {
    let mut out = String::new();
    let mut digit_run = String::new();
    for ch in input.chars() {
        if ch.is_ascii_digit() {
            digit_run.push(ch);
            continue;
        }
        flush_digit_run(&mut out, &mut digit_run);
        out.push(ch);
    }
    flush_digit_run(&mut out, &mut digit_run);
    out
}

fn flush_digit_run(out: &mut String, digit_run: &mut String) {
    if digit_run.is_empty() {
        return;
    }
    if digit_run.len() < 7 {
        out.push_str(digit_run);
    } else {
        out.push(' ');
    }
    digit_run.clear();
}

fn is_sensitive_query_token(token: &str) -> bool {
    let trimmed = token.trim_matches(|ch: char| ch.is_ascii_punctuation());
    let lower = trimmed.to_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || (trimmed.contains('@') && trimmed.contains('.'))
}

fn keyword_query(query: &str) -> String {
    let stopwords = [
        "请", "帮我", "一下", "关于", "这个", "那个", "需要", "搜索", "查询", "总结", "核对",
        "please", "search", "about", "with", "from", "that", "this", "the", "and", "for",
    ];
    let mut words = Vec::new();
    for token in query.split(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                ',' | '.' | ':' | '：' | '，' | '。' | '?' | '？' | '!' | '！' | ';' | '；'
            )
    }) {
        let token = token.trim();
        if token.len() < 2 {
            continue;
        }
        if stopwords
            .iter()
            .any(|word| token.eq_ignore_ascii_case(word))
        {
            continue;
        }
        if !words
            .iter()
            .any(|word: &&str| word.eq_ignore_ascii_case(token))
        {
            words.push(token);
        }
        if words.len() >= 8 {
            break;
        }
    }
    truncate_query(&words.join(" "), 120)
}

fn truncate_query(query: &str, max_chars: usize) -> String {
    query.chars().take(max_chars).collect::<String>()
}

fn normalize_query_whitespace(query: &str) -> String {
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches([' ', '\n', '\t', '。', '，', ',', '.', '？', '?'])
        .to_string()
}
