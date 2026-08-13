//! Controlled HTTPS page fetch for AI evidence (single-page, read-only).

use std::collections::HashSet;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use futures_util::StreamExt;
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::llm::http_politeness::throttle_host;
use crate::network::safe_https::{host_of, validate_https_url, ProdSafeHttpsGate, SafeHttpsGate};
use crate::storage::db::Database;

pub const HARD_MAX_CHARS: usize = 64_000;
const MAX_RESPONSE_BYTES: usize = 2_000_000;
const FETCH_TIMEOUT_SECS: u64 = 15;
const MAX_REDIRECTS: usize = 5;
const CACHE_TTL_HOURS: i64 = 24;
const MAX_WEB_PAGE_CACHE_ROWS: usize = 256;
pub const PAGE_FETCH_CACHE_BROKER_VERSION: &str = "web-evidence-broker.v1";

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Iris/1.2.21 (+https://github.com/skahanium/iris)",
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
    pub title: String,
    pub text: String,
}

#[derive(Debug)]
struct PageBytesResult {
    final_url: String,
    content_type: String,
    bytes: Vec<u8>,
}

async fn fetch_page_bytes_with_gate<G: SafeHttpsGate>(
    gate: &G,
    url: &str,
    user_agent: &str,
    timeout: Duration,
) -> AppResult<PageBytesResult> {
    tokio::time::timeout(timeout, async {
        let mut current = url.trim().to_string();
        let mut visited = HashSet::new();

        for _ in 0..=MAX_REDIRECTS {
            gate.validate_url(&current)
                .map_err(|_| AppError::msg("web_url_rejected"))?;
            if !visited.insert(current.clone()) {
                return Err(AppError::msg("web_redirect_loop"));
            }

            let (host, port) = host_port_of(&current)?;
            let addrs = gate
                .resolve_public_addrs(&host)
                .await
                .map_err(|_| AppError::msg("web_dns_failed"))?;
            if gate.uses_fixed_proxy_transport() {
                let response = crate::network::safe_https::fixed_https_get(
                    &current,
                    &addrs,
                    user_agent,
                    &[],
                    MAX_RESPONSE_BYTES,
                )
                .await
                .map_err(|error| {
                    AppError::msg(match error.to_string().as_str() {
                        "https_proxy_unreachable" => "web_proxy_unreachable",
                        "https_proxy_unsupported" => "web_proxy_unsupported",
                        "https_proxy_auth_unsupported" => "web_proxy_auth_unsupported",
                        "https_proxy_connect_failed" => "web_proxy_connect_failed",
                        "https_response_headers_too_large" => "web_response_headers_too_large",
                        "https_response_too_large" => "web_response_too_large",
                        "https_stream_failed" => "web_stream_failed",
                        _ => "web_fetch_failed",
                    })
                })?;
                if (300..400).contains(&response.status) {
                    let location = response
                        .headers
                        .get(reqwest::header::LOCATION)
                        .and_then(|value| value.to_str().ok())
                        .ok_or_else(|| AppError::msg("web_redirect_missing_location"))?;
                    current = reqwest::Url::parse(&current)
                        .map_err(|_| AppError::msg("web_url_invalid"))?
                        .join(location)
                        .map_err(|_| AppError::msg("web_redirect_invalid_target"))?
                        .to_string();
                    continue;
                }
                if !(200..300).contains(&response.status) {
                    return Err(AppError::msg(format!("web_http_error_{}", response.status)));
                }
                let content_type = response
                    .headers
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("")
                    .to_lowercase();
                if !content_type.is_empty()
                    && !content_type.contains("text/html")
                    && !content_type.contains("text/plain")
                    && !content_type.contains("application/xhtml")
                {
                    return Err(AppError::msg("web_content_type_unsupported"));
                }
                return Ok(PageBytesResult {
                    final_url: current,
                    content_type,
                    bytes: response.bytes,
                });
            }
            let client = gate
                .build_client(&host, port, &addrs, timeout)
                .map_err(|_| AppError::msg("web_client_build_failed"))?;
            let response = client
                .get(&current)
                .header(reqwest::header::USER_AGENT, user_agent)
                .send()
                .await
                .map_err(|error| {
                    if error.is_timeout() {
                        AppError::msg("web_fetch_timeout")
                    } else {
                        AppError::msg("web_fetch_failed")
                    }
                })?;
            crate::network::safe_https::validate_response_headers(response.headers())
                .map_err(|_| AppError::msg("web_response_headers_too_large"))?;

            if response.status().is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| AppError::msg("web_redirect_missing_location"))?;
                current = reqwest::Url::parse(&current)
                    .map_err(|_| AppError::msg("web_url_invalid"))?
                    .join(location)
                    .map_err(|_| AppError::msg("web_redirect_invalid_target"))?
                    .to_string();
                continue;
            }
            if !response.status().is_success() {
                return Err(AppError::msg(format!(
                    "web_http_error_{}",
                    response.status().as_u16()
                )));
            }

            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .to_lowercase();
            if !content_type.is_empty()
                && !content_type.contains("text/html")
                && !content_type.contains("text/plain")
                && !content_type.contains("application/xhtml")
            {
                return Err(AppError::msg("web_content_type_unsupported"));
            }
            if response
                .content_length()
                .is_some_and(|length| length as usize > MAX_RESPONSE_BYTES)
            {
                return Err(AppError::msg("web_response_too_large"));
            }

            let mut bytes = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| AppError::msg("web_stream_failed"))?;
                if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
                    return Err(AppError::msg("web_response_too_large"));
                }
                bytes.extend_from_slice(&chunk);
            }
            return Ok(PageBytesResult {
                final_url: current,
                content_type,
                bytes,
            });
        }
        Err(AppError::msg("web_too_many_redirects"))
    })
    .await
    .map_err(|_| AppError::msg("web_fetch_timeout"))?
}

fn host_port_of(url: &str) -> AppResult<(String, u16)> {
    let parsed = reqwest::Url::parse(url).map_err(|_| AppError::msg("web_url_invalid"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::msg("web_url_invalid"))?
        .to_string();
    Ok((host, parsed.port_or_known_default().unwrap_or(443)))
}

struct CachedRow {
    title: Option<String>,
    body_text: String,
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
///
/// 地址判断（localhost、私网段、DNS rebinding 提示等）统一由
/// `network::safe_https::validate_https_url` 承担；此处仅保留网页抓取
/// 专属的路径检查，避免复制第二套地址逻辑。
pub fn validate_fetch_url(url: &str) -> AppResult<()> {
    validate_https_url(url)?;
    if url.trim().contains("..") {
        return Err(AppError::msg("web_url_invalid"));
    }
    Ok(())
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
            "SELECT title, body_text FROM web_page_cache
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
        let text: String = cached.body_text.chars().take(max_chars).collect();
        return Ok(PageFetchResult {
            title: cached.title.unwrap_or_default(),
            text,
        });
    }

    let host = host_of(url).unwrap_or_default();
    throttle_host(&host).await?;

    let fetched = fetch_page_bytes_with_gate(
        &ProdSafeHttpsGate,
        url,
        random_user_agent(),
        Duration::from_secs(FETCH_TIMEOUT_SECS),
    )
    .await?;
    let content_type = fetched.content_type;
    let bytes = fetched.bytes;

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
        &fetched.final_url,
        title_opt.as_deref(),
        &text,
        &full_hash,
        &scope,
    )?;

    if text.chars().count() > max_chars {
        text = text.chars().take(max_chars).collect();
    }

    tracing::info!(
        url_hash = %&hash[..8],
        from_cache = false,
        char_count = text.chars().count(),
        "web_fetch_complete"
    );

    Ok(PageFetchResult {
        title: title_opt.unwrap_or_default(),
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::{IpAddr, SocketAddr};
    use std::sync::{Arc, Mutex};

    use crate::feed::test_http::{TestResponse, TestServer};

    #[derive(Default)]
    struct RecordingWebGate {
        validated: Arc<Mutex<Vec<String>>>,
        resolved: Arc<Mutex<Vec<String>>>,
    }

    impl SafeHttpsGate for RecordingWebGate {
        fn validate_url(&self, url: &str) -> AppResult<()> {
            self.validated
                .lock()
                .expect("validated lock")
                .push(url.into());
            Ok(())
        }

        fn resolve_public_addrs(
            &self,
            host: &str,
        ) -> impl std::future::Future<Output = AppResult<Vec<IpAddr>>> + Send {
            let resolved = self.resolved.clone();
            let host = host.to_string();
            async move {
                resolved.lock().expect("resolved lock").push(host);
                Ok(vec!["127.0.0.1".parse().expect("loopback")])
            }
        }

        fn build_client(
            &self,
            host: &str,
            port: u16,
            addrs: &[IpAddr],
            timeout: Duration,
        ) -> AppResult<reqwest::Client> {
            let mut builder = reqwest::Client::builder()
                .https_only(false)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(timeout);
            for addr in addrs {
                builder = builder.resolve(host, SocketAddr::new(*addr, port));
            }
            builder
                .build()
                .map_err(|_| AppError::msg("test_client_failed"))
        }
    }

    #[tokio::test]
    async fn web_transport_revalidates_and_repins_each_redirect() {
        let server = TestServer::start().await;
        server.queue(TestResponse::new(302, "").header("Location", "/final"));
        server.queue(
            TestResponse::new(200, "<main>safe body</main>").header("Content-Type", "text/html"),
        );
        let gate = RecordingWebGate::default();

        let result = fetch_page_bytes_with_gate(
            &gate,
            &server.url("/start"),
            "Iris/test",
            Duration::from_secs(2),
        )
        .await
        .expect("redirect fetch succeeds");

        assert_eq!(result.final_url, server.url("/final"));
        assert_eq!(gate.validated.lock().expect("validated").len(), 2);
        assert_eq!(gate.resolved.lock().expect("resolved").len(), 2);
        assert_eq!(server.requests_snapshot().len(), 2);
    }

    #[tokio::test]
    async fn web_transport_streams_with_a_hard_body_limit() {
        let server = TestServer::start().await;
        server.queue(TestResponse::new(200, vec![b'a'; MAX_RESPONSE_BYTES + 1]));

        let error = fetch_page_bytes_with_gate(
            &RecordingWebGate::default(),
            &server.url("/large"),
            "Iris/test",
            Duration::from_secs(2),
        )
        .await
        .expect_err("stream must abort when it crosses the web response limit");
        assert_eq!(error.to_string(), "web_response_too_large");
    }

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
