//! Controlled HTTPS page fetch for AI evidence (single-page, read-only).

use std::net::IpAddr;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::llm::http_politeness::throttle_host;
use crate::network::cert_pinning::https_client_builder;
use crate::security::ipc_policy::validate_https_url;
use crate::storage::db::Database;

pub const DEFAULT_MAX_CHARS: usize = 24_000;
pub const HARD_MAX_CHARS: usize = 64_000;
const MAX_RESPONSE_BYTES: usize = 2_000_000;
const FETCH_TIMEOUT_SECS: u64 = 15;
const CACHE_TTL_HOURS: i64 = 24;

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Iris/1.1.0 (+https://github.com/skahanium/iris)",
];

fn random_user_agent() -> &'static str {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::time::Instant::now().hash(&mut hasher);
    USER_AGENTS[hasher.finish() as usize % USER_AGENTS.len()]
}

/// Result of fetching and extracting a web page.
#[derive(Debug, Clone)]
pub struct PageFetchResult {
    pub url: String,
    pub title: String,
    pub text: String,
    pub truncated: bool,
    pub from_cache: bool,
    pub content_hash: String,
}

struct CachedRow {
    title: Option<String>,
    body_text: String,
    content_hash: String,
}

fn url_hash(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.trim().as_bytes());
    hex::encode(hasher.finalize())
}

fn content_hash(text: &str) -> String {
    crate::cas::hash::content_hash_str(text)
}

/// Validate URL for fetch: HTTPS only, no SSRF to private/local hosts.
pub fn validate_fetch_url(url: &str) -> AppResult<()> {
    validate_https_url(url)?;
    let trimmed = url.trim();
    if trimmed.contains("..") {
        return Err(AppError::msg("非法 URL"));
    }
    let host = extract_host(trimmed).ok_or_else(|| AppError::msg("无法解析 URL 主机名"))?;
    if host.contains('@') {
        return Err(AppError::msg("URL 不允许包含用户信息"));
    }
    let host_lower = host.to_lowercase();
    if host_lower == "localhost" || host_lower.ends_with(".localhost") {
        return Err(AppError::msg("不允许访问本地主机"));
    }
    if host_lower == "0.0.0.0" {
        return Err(AppError::msg("不允许访问该地址"));
    }
    if let Ok(ip) = host_lower.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(AppError::msg("不允许访问内网或保留地址"));
        }
        return Err(AppError::msg("仅允许域名 URL，不支持直接 IP 访问"));
    }
    if is_private_host_hint(&host_lower) {
        return Err(AppError::msg("不允许访问内网或保留地址"));
    }
    Ok(())
}

fn extract_host(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Some("@".into());
    }
    let host = parsed.host_str()?;
    Some(
        host.strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or(host)
            .to_owned(),
    )
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || (v4.octets()[0] == 169 && v4.octets()[1] == 254)
                // RFC 6598 Carrier-grade NAT
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
                // RFC 2544 benchmarking
                || (v4.octets()[0] == 198 && v4.octets()[1] >= 18 && v4.octets()[1] <= 19)
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified() || is_ipv6_private(v6),
    }
}

fn is_ipv6_private(v6: std::net::Ipv6Addr) -> bool {
    let s = v6.segments();
    // fc00::/7 — Unique Local Address
    (s[0] & 0xFE00) == 0xFC00
    // fe80::/10 — Link-local
    || (s[0] & 0xFFC0) == 0xFE80
    // ::ffff:0:0/96 — IPv4-mapped IPv6
    || (s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0xFFFF)
    // 64:ff9b::/96 — IPv4/IPv6 translation
    || (s[0] == 0x0064 && s[1] == 0xFF9B)
    // ::1 — loopback (defense-in-depth)
    || v6.is_loopback()
}

fn is_private_host_hint(host: &str) -> bool {
    // Try parsing as IP address first
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_blocked_ip(ip);
    }

    // DNS rebinding detection: domain names containing private IP octets
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() >= 4 {
        if let (Ok(a), Ok(b)) = (parts[0].parse::<u8>(), parts[1].parse::<u8>()) {
            // 10.x.x.x, 127.x.x.x, 192.168.x.x
            if a == 10
                || a == 127
                || (a == 192 && b == 168)
                // 172.16-31.x.x
                || (a == 172 && (16..=31).contains(&b))
                // 169.254.x.x
                || (a == 169 && b == 254)
            {
                return true;
            }
        }
    }

    // Common private/intranet domain suffixes
    host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".localhost")
        || host.ends_with(".lan")
        || host == "localhost"
}

/// Extract readable plain text from HTML with semantic selectors and noise filtering.
pub fn extract_readable_text(html: &str) -> (String, Option<String>) {
    let document = Html::parse_document(html);
    let title = Selector::parse("title")
        .ok()
        .and_then(|sel| document.select(&sel).next())
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|t| !t.is_empty());

    // Semantic content selectors in priority order
    for selector in [
        "main",
        "article",
        "[role=main]",
        "[role=article]",
        ".post-content",
        ".article-content",
        ".entry-content",
        ".content",
        ".main-content",
        "#content",
        "#main-content",
        ".markdown-body",
    ] {
        if let Ok(sel) = Selector::parse(selector) {
            if let Some(el) = document.select(&sel).next() {
                let text = normalize_whitespace(&el.text().collect::<String>());
                if text.len() > 100 {
                    return (text, title);
                }
            }
        }
    }

    // Fallback: strip noise elements from body
    if let Ok(body_sel) = Selector::parse("body") {
        if let Some(el) = document.select(&body_sel).next() {
            let noise_tags = [
                "script", "style", "nav", "footer", "header", "aside", "noscript",
            ];
            let mut body_html = el.html();
            for tag in &noise_tags {
                if let Ok(noise_sel) = Selector::parse(tag) {
                    for noise_el in document.select(&noise_sel) {
                        let noise_html = noise_el.html();
                        body_html = body_html.replace(&noise_html, "");
                    }
                }
            }
            let cleaned = Html::parse_document(&body_html);
            if let Ok(body_sel2) = Selector::parse("body") {
                if let Some(cleaned_body) = cleaned.select(&body_sel2).next() {
                    let text = normalize_whitespace(&cleaned_body.text().collect::<String>());
                    if !text.is_empty() {
                        return (text, title);
                    }
                }
            }
        }
    }

    (
        normalize_whitespace(&document.root_element().text().collect::<String>()),
        title,
    )
}

pub fn normalize_whitespace(text: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

fn load_cache(db: &Database, hash: &str) -> AppResult<Option<CachedRow>> {
    db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT title, body_text, content_hash FROM web_page_cache
             WHERE url_hash = ?1 AND expires_at > datetime('now')",
        )?;
        let mut rows = stmt.query(rusqlite::params![hash])?;
        if let Some(row) = rows.next()? {
            Ok(Some(CachedRow {
                title: row.get(0)?,
                body_text: row.get(1)?,
                content_hash: row.get(2)?,
            }))
        } else {
            Ok(None)
        }
    })
}

fn store_cache(
    db: &Database,
    hash: &str,
    url: &str,
    title: Option<&str>,
    body: &str,
    hash_body: &str,
) -> AppResult<()> {
    let fetched_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let expires_at = (Utc::now() + ChronoDuration::hours(CACHE_TTL_HOURS))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO web_page_cache (url_hash, url, title, body_text, content_hash, fetched_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(url_hash) DO UPDATE SET
               url = excluded.url,
               title = excluded.title,
               body_text = excluded.body_text,
               content_hash = excluded.content_hash,
               fetched_at = excluded.fetched_at,
               expires_at = excluded.expires_at",
            rusqlite::params![
                hash,
                url,
                title,
                body,
                hash_body,
                fetched_at,
                expires_at
            ],
        )?;
        Ok(())
    })
}

/// 清除所有网页缓存（供前端 IPC 调用）。
pub fn clear_web_cache(db: &Database) -> AppResult<usize> {
    db.with_conn(|conn| {
        let deleted = conn.execute("DELETE FROM web_page_cache", [])?;
        Ok(deleted)
    })
}

/// 清理过期网页缓存。
pub fn cleanup_expired_web_cache(db: &Database) -> AppResult<usize> {
    db.with_conn(|conn| {
        let deleted = conn.execute(
            "DELETE FROM web_page_cache WHERE expires_at < datetime('now')",
            [],
        )?;
        Ok(deleted)
    })
}

/// Fetch a page (with SQLite cache and per-host throttle).
pub async fn fetch_web_page(
    db: &Database,
    url: &str,
    max_chars: usize,
) -> AppResult<PageFetchResult> {
    let url = url.trim();
    validate_fetch_url(url)?;
    let max_chars = max_chars.clamp(1, HARD_MAX_CHARS);
    let hash = url_hash(url);

    tracing::info!(
        url_hash = %&hash[..8],
        "web_fetch_start"
    );

    if let Some(cached) = load_cache(db, &hash)? {
        tracing::info!(
            url_hash = %&hash[..8],
            from_cache = true,
            "web_fetch_complete"
        );
        let truncated = cached.body_text.chars().count() > max_chars;
        let text: String = cached.body_text.chars().take(max_chars).collect();
        return Ok(PageFetchResult {
            url: url.to_string(),
            title: cached.title.unwrap_or_default(),
            text,
            truncated,
            from_cache: true,
            content_hash: cached.content_hash,
        });
    }

    let host = extract_host(url).unwrap_or_default();
    throttle_host(&host).await?;

    let client = https_client_builder()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
        .user_agent(random_user_agent())
        .build()
        .map_err(|e| AppError::msg(format!("Failed to build HTTP client: {e}")))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::msg(format!("网页请求失败: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::msg(format!(
            "网页返回 HTTP {}",
            response.status()
        )));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    if !content_type.is_empty()
        && !content_type.contains("text/html")
        && !content_type.contains("text/plain")
        && !content_type.contains("application/xhtml")
    {
        return Err(AppError::msg("仅支持 HTML 或纯文本页面"));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::msg(format!("读取网页失败: {e}")))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(AppError::msg("网页体积超过限制"));
    }

    let html = String::from_utf8_lossy(&bytes);
    let (mut text, title_opt) = if content_type.contains("text/plain") {
        (normalize_whitespace(&html), None)
    } else {
        extract_readable_text(&html)
    };

    if text.is_empty() {
        return Err(AppError::msg("未能从页面提取正文"));
    }

    let full_hash = content_hash(&text);
    store_cache(db, &hash, url, title_opt.as_deref(), &text, &full_hash)?;

    let truncated = text.chars().count() > max_chars;
    if truncated {
        text = text.chars().take(max_chars).collect();
    }

    tracing::info!(
        url_hash = %&hash[..8],
        from_cache = false,
        char_count = text.chars().count(),
        "web_fetch_complete"
    );

    Ok(PageFetchResult {
        url: url.to_string(),
        title: title_opt.unwrap_or_default(),
        text,
        truncated,
        from_cache: false,
        content_hash: full_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_localhost() {
        assert!(validate_fetch_url("https://localhost/x").is_err());
    }

    #[test]
    fn validate_rejects_private_ip() {
        assert!(validate_fetch_url("https://192.168.1.1/").is_err());
    }

    #[test]
    fn validate_rejects_ipv6_mapped() {
        assert!(validate_fetch_url("https://[::ffff:192.168.1.1]/").is_err());
    }

    #[test]
    fn validate_rejects_ipv6_link_local() {
        assert!(validate_fetch_url("https://[fe80::1]/").is_err());
    }

    #[test]
    fn validate_rejects_ipv6_loopback_with_port() {
        assert!(validate_fetch_url("https://[::1]:443/").is_err());
    }

    #[test]
    fn validate_rejects_userinfo() {
        assert!(validate_fetch_url("https://user:pass@example.com/").is_err());
    }

    #[test]
    fn validate_rejects_ipv6_ula() {
        assert!(validate_fetch_url("https://[fd00::1]/").is_err());
    }

    #[test]
    fn validate_rejects_ipv6_translation() {
        assert!(validate_fetch_url("https://[64:ff9b::192.168.1.1]/").is_err());
    }

    #[test]
    fn validate_rejects_cgnat() {
        assert!(validate_fetch_url("https://100.64.0.1/").is_err());
    }

    #[test]
    fn validate_rejects_benchmark() {
        assert!(validate_fetch_url("https://198.18.0.1/").is_err());
    }

    #[test]
    fn validate_rejects_dns_rebinding() {
        assert!(validate_fetch_url("https://192.168.1.1.nip.io/").is_err());
    }

    #[test]
    fn validate_rejects_172_private() {
        assert!(validate_fetch_url("https://172.16.0.1/").is_err());
        assert!(validate_fetch_url("https://172.31.255.255/").is_err());
    }

    #[test]
    fn validate_accepts_172_public() {
        // 172.32.x.x is public — only 172.16-31 is private
        validate_fetch_url("https://172.32.0.1.example.com/").unwrap();
    }

    #[test]
    fn validate_rejects_local_suffix() {
        assert!(validate_fetch_url("https://myserver.local/").is_err());
    }

    #[test]
    fn validate_accepts_https_domain() {
        validate_fetch_url("https://www.example.com/doc").unwrap();
    }

    #[test]
    fn extract_text_from_html() {
        let html = r#"<!DOCTYPE html><html><head><title>Hi</title></head>
        <body><main><p>Hello <b>world</b></p></main></body></html>"#;
        let (text, title) = extract_readable_text(html);
        assert_eq!(title.as_deref(), Some("Hi"));
        assert!(text.contains("Hello world"));
    }

    #[test]
    fn extract_filters_noise() {
        let html = r#"<!DOCTYPE html><html><head><title>Test</title></head>
        <body>
            <nav>Skip this navigation menu</nav>
            <header>Page header text</header>
            <main><p>Main content here with enough text to pass the length threshold for extraction testing purposes.</p></main>
            <footer>Footer copyright info</footer>
        </body></html>"#;
        let (text, _title) = extract_readable_text(html);
        assert!(text.contains("Main content"));
    }

    #[test]
    fn extract_content_selector_priority() {
        let html = r#"<!DOCTYPE html><html><head><title>P</title></head>
        <body>
            <div class="article-content">Real article body content here</div>
            <main>Less specific main content</main>
        </body></html>"#;
        let (text, _title) = extract_readable_text(html);
        assert!(text.contains("Real article body"));
    }

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize_whitespace("a   b\n\nc"), "a b c");
    }
}
