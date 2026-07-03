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

pub const HARD_MAX_CHARS: usize = 64_000;
const MAX_RESPONSE_BYTES: usize = 2_000_000;
const FETCH_TIMEOUT_SECS: u64 = 15;
const CACHE_TTL_HOURS: i64 = 24;
const MAX_WEB_PAGE_CACHE_ROWS: usize = 256;
pub const PAGE_FETCH_CACHE_BROKER_VERSION: &str = "web-evidence-broker.v1";

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Iris/1.2.2 (+https://github.com/skahanium/iris)",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageFetchCacheScope {
    pub vault_id: Option<String>,
    pub provider_id: String,
    pub provider_kind: String,
    pub provider_config_hash: String,
    pub broker_version: String,
}

impl PageFetchCacheScope {
    pub fn native(vault_id: Option<String>, broker_version: &str) -> Self {
        Self {
            vault_id,
            provider_id: "native.fetch".into(),
            provider_kind: "native".into(),
            provider_config_hash: native_fetch_provider_config_hash(),
            broker_version: broker_version.into(),
        }
    }
}

fn native_fetch_provider_config_hash() -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"native.fetch");
    hasher.update(b"\0");
    hasher.update(HARD_MAX_CHARS.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(MAX_RESPONSE_BYTES.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(FETCH_TIMEOUT_SECS.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

fn url_hash(url: &str, scope: &PageFetchCacheScope) -> String {
    let mut hasher = Sha256::new();
    hasher.update(scope.vault_id.as_deref().unwrap_or("default").as_bytes());
    hasher.update(b"\0");
    hasher.update(scope.provider_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(scope.provider_kind.as_bytes());
    hasher.update(b"\0");
    hasher.update(scope.provider_config_hash.as_bytes());
    hasher.update(b"\0");
    hasher.update(scope.broker_version.as_bytes());
    hasher.update(b"\0");
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

fn load_cache(
    db: &Database,
    hash: &str,
    scope: &PageFetchCacheScope,
) -> AppResult<Option<CachedRow>> {
    db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT title, body_text, content_hash FROM web_page_cache
             WHERE url_hash = ?1
               AND ((vault_id IS NULL AND ?2 IS NULL) OR vault_id = ?2)
               AND provider_id = ?3
               AND provider_kind = ?4
               AND provider_config_hash = ?5
               AND broker_version = ?6
               AND expires_at > datetime('now')",
        )?;
        let mut rows = stmt.query(rusqlite::params![
            hash,
            scope.vault_id.as_deref(),
            scope.provider_id.as_str(),
            scope.provider_kind.as_str(),
            scope.provider_config_hash.as_str(),
            scope.broker_version.as_str(),
        ])?;
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
    scope: &PageFetchCacheScope,
) -> AppResult<()> {
    let fetched_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let expires_at = (Utc::now() + ChronoDuration::hours(CACHE_TTL_HOURS))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO web_page_cache (
               url_hash,
               url,
               title,
               body_text,
               content_hash,
               fetched_at,
               expires_at,
               vault_id,
               provider_id,
               provider_kind,
               provider_config_hash,
               broker_version
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(url_hash) DO UPDATE SET
               url = excluded.url,
               title = excluded.title,
               body_text = excluded.body_text,
               content_hash = excluded.content_hash,
               fetched_at = excluded.fetched_at,
               expires_at = excluded.expires_at,
               vault_id = excluded.vault_id,
               provider_id = excluded.provider_id,
               provider_kind = excluded.provider_kind,
               provider_config_hash = excluded.provider_config_hash,
               broker_version = excluded.broker_version",
            rusqlite::params![
                hash,
                url,
                title,
                body,
                hash_body,
                fetched_at,
                expires_at,
                scope.vault_id.as_deref(),
                scope.provider_id.as_str(),
                scope.provider_kind.as_str(),
                scope.provider_config_hash.as_str(),
                scope.broker_version.as_str(),
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
    .and_then(|expired| prune_page_cache_lru(db, MAX_WEB_PAGE_CACHE_ROWS).map(|lru| expired + lru))
}

fn prune_page_cache_lru(db: &Database, max_rows: usize) -> AppResult<usize> {
    db.with_conn(|conn| {
        let row_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM web_page_cache", [], |row| row.get(0))?;
        let overflow = row_count.saturating_sub(max_rows as i64);
        if overflow == 0 {
            return Ok(0);
        }
        let deleted = conn.execute(
            "DELETE FROM web_page_cache
             WHERE url_hash IN (
               SELECT url_hash FROM web_page_cache
               ORDER BY datetime(fetched_at) ASC, url_hash ASC
               LIMIT ?1
             )",
            [overflow],
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
    let scope = PageFetchCacheScope::native(None, PAGE_FETCH_CACHE_BROKER_VERSION);
    let hash = url_hash(url, &scope);

    tracing::info!(
        url_hash = %&hash[..8],
        "web_fetch_start"
    );

    if let Some(cached) = load_cache(db, &hash, &scope)? {
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
    store_cache(
        db,
        &hash,
        url,
        title_opt.as_deref(),
        &text,
        &full_hash,
        &scope,
    )?;

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
    fn page_cache_key_is_scoped_by_provider_config_and_vault() {
        let base = PageFetchCacheScope::native(None, PAGE_FETCH_CACHE_BROKER_VERSION);
        let alternate_provider = PageFetchCacheScope {
            provider_id: "native.fetch.alt".into(),
            ..base.clone()
        };
        let alternate_config = PageFetchCacheScope {
            provider_config_hash: "changed-config".into(),
            ..base.clone()
        };
        let alternate_kind = PageFetchCacheScope {
            provider_kind: "mcp".into(),
            ..base.clone()
        };
        let alternate_broker = PageFetchCacheScope {
            broker_version: "web-evidence-broker.v2".into(),
            ..base.clone()
        };
        let alternate_vault = PageFetchCacheScope {
            vault_id: Some("vault-b".into()),
            ..base.clone()
        };

        let base_key = url_hash("https://example.com/private", &base);

        assert_ne!(
            base_key,
            url_hash("https://example.com/private", &alternate_provider)
        );
        assert_ne!(
            base_key,
            url_hash("https://example.com/private", &alternate_config)
        );
        assert_ne!(
            base_key,
            url_hash("https://example.com/private", &alternate_kind)
        );
        assert_ne!(
            base_key,
            url_hash("https://example.com/private", &alternate_broker)
        );
        assert_ne!(
            base_key,
            url_hash("https://example.com/private", &alternate_vault)
        );
        assert!(!base_key.contains("example.com"));
    }

    #[test]
    fn page_cache_reads_only_matching_provider_scope() {
        let db = Database::open_in_memory().expect("mem db");
        let base = PageFetchCacheScope::native(None, PAGE_FETCH_CACHE_BROKER_VERSION);
        let alternate = PageFetchCacheScope {
            provider_config_hash: "changed-config".into(),
            ..base.clone()
        };
        let key = url_hash("https://example.com/private", &base);

        store_cache(
            &db,
            &key,
            "https://example.com/private",
            Some("title"),
            "body",
            "content-hash",
            &base,
        )
        .expect("store scoped page cache");

        assert!(load_cache(&db, &key, &base)
            .expect("read matching cache")
            .is_some());
        assert!(load_cache(&db, &key, &alternate)
            .expect("read alternate cache")
            .is_none());
    }

    #[test]
    fn page_cache_lru_prunes_oldest_rows_over_limit() {
        let db = Database::open_in_memory().expect("mem db");
        let scope = PageFetchCacheScope::native(None, PAGE_FETCH_CACHE_BROKER_VERSION);

        for (hash, url, fetched_at) in [
            ("old", "https://example.com/old", "2026-01-01T00:00:00Z"),
            (
                "middle",
                "https://example.com/middle",
                "2026-01-02T00:00:00Z",
            ),
            ("new", "https://example.com/new", "2026-01-03T00:00:00Z"),
        ] {
            store_cache(&db, hash, url, Some(hash), hash, hash, &scope)
                .expect("store page cache row");
            db.with_conn(|conn| {
                conn.execute(
                    "UPDATE web_page_cache SET fetched_at = ?2 WHERE url_hash = ?1",
                    rusqlite::params![hash, fetched_at],
                )?;
                Ok::<(), crate::error::AppError>(())
            })
            .expect("set fetched_at");
        }

        assert_eq!(prune_page_cache_lru(&db, 2).expect("prune lru"), 1);
        assert!(load_cache(&db, "old", &scope).expect("read old").is_none());
        assert!(load_cache(&db, "middle", &scope)
            .expect("read middle")
            .is_some());
        assert!(load_cache(&db, "new", &scope).expect("read new").is_some());
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
