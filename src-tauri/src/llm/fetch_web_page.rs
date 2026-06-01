//! Controlled HTTPS page fetch for AI evidence (single-page, read-only).

use std::net::IpAddr;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::llm::http_politeness::throttle_host;
use crate::network::cert_pinning::pinned_client_builder;
use crate::security::ipc_policy::validate_https_url;
use crate::storage::db::Database;

pub const DEFAULT_MAX_CHARS: usize = 24_000;
pub const HARD_MAX_CHARS: usize = 64_000;
const MAX_RESPONSE_BYTES: usize = 2_000_000;
const FETCH_TIMEOUT_SECS: u64 = 15;
const CACHE_TTL_HOURS: i64 = 24;

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
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
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
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split('/').next()?.split('?').next()?.split('#').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.octets()[0] == 169 && v4.octets()[1] == 254
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}

fn is_private_host_hint(host: &str) -> bool {
    host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || host.starts_with("[::1]")
        || host == "::1"
        || host.starts_with("172.16.")
        || host.starts_with("172.17.")
        || host.starts_with("172.18.")
        || host.starts_with("172.19.")
        || host.starts_with("172.2")
        || host.starts_with("172.30.")
        || host.starts_with("172.31.")
}

/// Extract readable plain text from HTML.
pub fn extract_readable_text(html: &str) -> (String, Option<String>) {
    let document = Html::parse_document(html);
    let title = Selector::parse("title")
        .ok()
        .and_then(|sel| document.select(&sel).next())
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|t| !t.is_empty());

    for selector in ["main", "article", "[role=main]"] {
        if let Ok(sel) = Selector::parse(selector) {
            if let Some(el) = document.select(&sel).next() {
                let text = normalize_whitespace(&el.text().collect::<String>());
                if text.len() > 80 {
                    return (text, title);
                }
            }
        }
    }
    if let Ok(sel) = Selector::parse("body") {
        if let Some(el) = document.select(&sel).next() {
            let text = normalize_whitespace(&el.text().collect::<String>());
            return (text, title);
        }
    }
    (normalize_whitespace(&document.root_element().text().collect::<String>()), title)
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

    if let Some(cached) = load_cache(db, &hash)? {
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
    throttle_host(&host)?;

    let client = pinned_client_builder()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
        .user_agent("Iris/1.0 (+https://github.com/skahanium/iris)")
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
    store_cache(
        db,
        &hash,
        url,
        title_opt.as_deref(),
        &text,
        &full_hash,
    )?;

    let truncated = text.chars().count() > max_chars;
    if truncated {
        text = text.chars().take(max_chars).collect();
    }

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
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize_whitespace("a   b\n\nc"), "a b c");
    }
}
