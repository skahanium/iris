use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{Duration as ChronoDuration, Utc};
use regex::Regex;
use scraper::{Html, Selector};
use std::sync::LazyLock;

use sha2::{Digest, Sha256};

use crate::credentials::{self, MINIMAX_CREDENTIAL_SERVICE};
use crate::error::{AppError, AppResult};
use crate::network::cert_pinning::https_client_builder;
use crate::storage::db::Database;

use super::minimax_search;
use super::web_search_config::{
    load as load_web_search_preferences, WebSearchBackendMode, WebSearchEffectiveBackend,
    WebSearchPreferences,
};

pub const SEARCH_CACHE_BROKER_VERSION: &str = "web-evidence-broker.v1";
const MAX_SEARCH_CACHE_ROWS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchCacheScope {
    pub vault_id: Option<String>,
    pub provider_id: String,
    pub provider_kind: String,
    pub provider_config_hash: String,
    pub broker_version: String,
}

impl SearchCacheScope {
    #[cfg(test)]
    pub fn native(
        backend: WebSearchEffectiveBackend,
        vault_id: Option<String>,
        broker_version: &str,
    ) -> Self {
        let provider_id = match backend {
            WebSearchEffectiveBackend::Minimax => "native.minimax",
            WebSearchEffectiveBackend::Duckduckgo => "native.duckduckgo",
        };
        Self {
            vault_id,
            provider_id: provider_id.into(),
            provider_kind: "native".into(),
            provider_config_hash: native_provider_config_hash(backend, "", ""),
            broker_version: broker_version.into(),
        }
    }

    fn native_minimax(vault_id: Option<String>, host: &str, model: &str) -> Self {
        Self {
            vault_id,
            provider_id: "native.minimax".into(),
            provider_kind: "native".into(),
            provider_config_hash: native_provider_config_hash(
                WebSearchEffectiveBackend::Minimax,
                host,
                model,
            ),
            broker_version: SEARCH_CACHE_BROKER_VERSION.into(),
        }
    }

    fn native_duckduckgo(vault_id: Option<String>) -> Self {
        Self {
            vault_id,
            provider_id: "native.duckduckgo".into(),
            provider_kind: "native".into(),
            provider_config_hash: native_provider_config_hash(
                WebSearchEffectiveBackend::Duckduckgo,
                "",
                "",
            ),
            broker_version: SEARCH_CACHE_BROKER_VERSION.into(),
        }
    }
}

fn native_provider_config_hash(
    backend: WebSearchEffectiveBackend,
    host: &str,
    model: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(backend.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(host.trim().as_bytes());
    hasher.update(b"\0");
    hasher.update(model.trim().as_bytes());
    hex::encode(hasher.finalize())
}

fn query_hash_key(
    query: &str,
    backend: WebSearchEffectiveBackend,
    minimax_model: &str,
    scope: &SearchCacheScope,
) -> String {
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
    hasher.update(query.as_bytes());
    hasher.update(b"\0");
    hasher.update(backend.as_str().as_bytes());
    hasher.update(b"\0");
    if backend == WebSearchEffectiveBackend::Minimax {
        hasher.update(minimax_model.trim().as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Per-backend throttle (MiniMax / DuckDuckGo) to avoid cross-contention.
static LAST_MINIMAX: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));
static LAST_DDG: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

/// 检索结果与所用后端。
#[derive(Debug, Clone)]
pub struct WebSearchFetchResult {
    pub body: String,
    pub backend: WebSearchEffectiveBackend,
}

/// Fetch search context for an explicit native provider.
pub async fn fetch_native_provider_context(
    db: &Database,
    query: &str,
    backend: WebSearchEffectiveBackend,
) -> AppResult<WebSearchFetchResult> {
    let prefs = load_web_search_preferences(db)?;
    match backend {
        WebSearchEffectiveBackend::Minimax => {
            let scope = SearchCacheScope::native_minimax(
                None,
                &prefs.minimax_api_host,
                &prefs.minimax_search_model,
            );
            fetch_minimax_only(
                db,
                query,
                &prefs.minimax_api_host,
                &prefs.minimax_search_model,
                &scope,
            )
            .await
        }
        WebSearchEffectiveBackend::Duckduckgo => {
            let scope = SearchCacheScope::native_duckduckgo(None);
            fetch_duckduckgo_only(db, query, &scope).await
        }
    }
}

async fn fetch_minimax_only(
    db: &Database,
    query: &str,
    host: &str,
    model: &str,
    scope: &SearchCacheScope,
) -> AppResult<WebSearchFetchResult> {
    if !credentials::api_key_configured(db, MINIMAX_CREDENTIAL_SERVICE)? {
        return Err(AppError::msg("未配置 MiniMax API Key"));
    }
    fetch_minimax_cached(db, query, host, model, scope).await
}

async fn fetch_minimax_cached(
    db: &Database,
    query: &str,
    host: &str,
    model: &str,
    scope: &SearchCacheScope,
) -> AppResult<WebSearchFetchResult> {
    let backend = WebSearchEffectiveBackend::Minimax;
    let key = query_hash_key(query, backend, model, scope);
    if let Some(cached) = cache_get_db(db, &key, scope)? {
        return Ok(WebSearchFetchResult {
            body: cached,
            backend,
        });
    }
    throttle_minimax().await?;
    let body = minimax_search::search(db, query, host, model).await?;
    cache_set_db(
        db,
        &key,
        &hex::encode(&Sha256::digest(query.as_bytes())[..8]),
        backend.as_str(),
        scope,
        &body,
    )?;
    Ok(WebSearchFetchResult { body, backend })
}

async fn fetch_duckduckgo_only(
    db: &Database,
    query: &str,
    scope: &SearchCacheScope,
) -> AppResult<WebSearchFetchResult> {
    let backend = WebSearchEffectiveBackend::Duckduckgo;
    let key = query_hash_key(query, backend, "", scope);
    if let Some(cached) = cache_get_db(db, &key, scope)? {
        return Ok(WebSearchFetchResult {
            body: cached,
            backend,
        });
    }
    throttle_duckduckgo().await?;
    let body = duckduckgo_search(query).await?;
    cache_set_db(
        db,
        &key,
        &hex::encode(&Sha256::digest(query.as_bytes())[..8]),
        backend.as_str(),
        scope,
        &body,
    )?;
    Ok(WebSearchFetchResult { body, backend })
}

/// 推断连通性展示用的「预期主后端」（未实际发请求）。
pub fn expected_search_backend_for_connectivity(
    db: &Database,
    prefs: &WebSearchPreferences,
) -> WebSearchEffectiveBackend {
    match prefs.backend_mode {
        WebSearchBackendMode::Minimax => {
            if credentials::api_key_configured(db, MINIMAX_CREDENTIAL_SERVICE).unwrap_or(false) {
                WebSearchEffectiveBackend::Minimax
            } else {
                WebSearchEffectiveBackend::Duckduckgo
            }
        }
        WebSearchBackendMode::Duckduckgo => WebSearchEffectiveBackend::Duckduckgo,
        WebSearchBackendMode::Auto => {
            if credentials::api_key_configured(db, MINIMAX_CREDENTIAL_SERVICE).unwrap_or(false) {
                WebSearchEffectiveBackend::Minimax
            } else {
                WebSearchEffectiveBackend::Duckduckgo
            }
        }
    }
}

async fn throttle_minimax() -> AppResult<()> {
    throttle_backend(&LAST_MINIMAX).await
}

async fn throttle_duckduckgo() -> AppResult<()> {
    throttle_backend(&LAST_DDG).await
}

async fn throttle_backend(last: &'static LazyLock<Mutex<Option<Instant>>>) -> AppResult<()> {
    let need_wait = {
        let last_guard = last.lock().map_err(|_| AppError::msg("Lock error"))?;
        if let Some(t) = *last_guard {
            let elapsed = t.elapsed();
            if elapsed < Duration::from_secs(2) {
                Some(Duration::from_secs(2) - elapsed)
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some(wait) = need_wait {
        tokio::time::sleep(tokio::time::Duration::from_millis(wait.as_millis() as u64)).await;
    }

    last.lock()
        .map_err(|_| AppError::msg("Lock error"))?
        .replace(Instant::now());
    Ok(())
}

fn cache_get_db(db: &Database, key: &str, scope: &SearchCacheScope) -> AppResult<Option<String>> {
    db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT body FROM search_cache
             WHERE cache_key = ?1
               AND ((vault_id IS NULL AND ?2 IS NULL) OR vault_id = ?2)
               AND provider_id = ?3
               AND provider_kind = ?4
               AND provider_config_hash = ?5
               AND broker_version = ?6
               AND expires_at > datetime('now')",
        )?;
        let mut rows = stmt.query(rusqlite::params![
            key,
            scope.vault_id.as_deref(),
            scope.provider_id,
            scope.provider_kind,
            scope.provider_config_hash,
            scope.broker_version,
        ])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    })
}

fn cache_set_db(
    db: &Database,
    key: &str,
    query_hash: &str,
    backend: &str,
    scope: &SearchCacheScope,
    body: &str,
) -> AppResult<()> {
    let created_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let expires_at = (Utc::now() + ChronoDuration::hours(6))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO search_cache (
               cache_key,
               query_hash,
               backend,
               body,
               created_at,
               expires_at,
               vault_id,
               provider_id,
               provider_kind,
               provider_config_hash,
               broker_version
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(cache_key) DO UPDATE SET
               body = excluded.body,
               created_at = excluded.created_at,
               expires_at = excluded.expires_at,
               vault_id = excluded.vault_id,
               provider_id = excluded.provider_id,
               provider_kind = excluded.provider_kind,
               provider_config_hash = excluded.provider_config_hash,
               broker_version = excluded.broker_version",
            rusqlite::params![
                key,
                query_hash,
                backend,
                body,
                created_at,
                expires_at,
                scope.vault_id.as_deref(),
                scope.provider_id,
                scope.provider_kind,
                scope.provider_config_hash,
                scope.broker_version,
            ],
        )?;
        Ok(())
    })
}

/// 清理过期搜索缓存。
pub fn cleanup_expired_search_cache(db: &Database) -> AppResult<usize> {
    db.with_conn(|conn| {
        let deleted = conn.execute(
            "DELETE FROM search_cache WHERE expires_at < datetime('now')",
            [],
        )?;
        Ok(deleted)
    })
    .and_then(|expired| prune_search_cache_lru(db, MAX_SEARCH_CACHE_ROWS).map(|lru| expired + lru))
}

fn prune_search_cache_lru(db: &Database, max_rows: usize) -> AppResult<usize> {
    db.with_conn(|conn| {
        let row_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM search_cache", [], |row| row.get(0))?;
        let overflow = row_count.saturating_sub(max_rows as i64);
        if overflow == 0 {
            return Ok(0);
        }
        let deleted = conn.execute(
            "DELETE FROM search_cache
             WHERE cache_key IN (
               SELECT cache_key FROM search_cache
               ORDER BY datetime(created_at) ASC, cache_key ASC
               LIMIT ?1
             )",
            [overflow],
        )?;
        Ok(deleted)
    })
}

async fn duckduckgo_search(query: &str) -> AppResult<String> {
    // Preferred: Instant Answer API (structured JSON, stable)
    match duckduckgo_instant_answer(query).await {
        Ok(body) if !body.contains("(未找到搜索结果)") => return Ok(body),
        Ok(_) => {} // empty result, fall through to HTML
        Err(e) => tracing::debug!("DDG Instant Answer API failed, falling back to HTML: {e}"),
    }
    // Fallback: HTML page parsing
    duckduckgo_html_search(query).await
}

/// DuckDuckGo Instant Answer API — structured, no scraping needed.
async fn duckduckgo_instant_answer(query: &str) -> AppResult<String> {
    let url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
        urlencoding::encode(query)
    );
    let client = https_client_builder()
        .user_agent("Iris/1.2.1")
        .build()
        .map_err(|e| AppError::msg(format!("HTTP client build failed: {e}")))?;

    let resp = client.get(&url).send().await?;
    let data: serde_json::Value = resp.json().await?;

    let mut out = String::from("以下是与问题相关的网页搜索结果：\n\n");
    let mut count = 0;

    // Abstract
    if let Some(abstract_text) = data["AbstractText"].as_str() {
        if !abstract_text.is_empty() {
            count += 1;
            let source = data["AbstractSource"].as_str().unwrap_or("DuckDuckGo");
            let url = data["AbstractURL"].as_str().unwrap_or("");
            out.push_str(&format!(
                "[{}] 来源: {}\n    链接: {}\n    摘要: {}\n\n",
                count, source, url, abstract_text
            ));
        }
    }

    // RelatedTopics
    if let Some(topics) = data["RelatedTopics"].as_array() {
        for topic in topics.iter().take(5) {
            if let Some(text) = topic["Text"].as_str() {
                if text.is_empty() {
                    continue;
                }
                count += 1;
                let first_url = topic["FirstURL"].as_str().unwrap_or("");
                out.push_str(&format!(
                    "[{}] 摘要: {}\n    链接: {}\n\n",
                    count, text, first_url
                ));
            }
        }
    }

    if count == 0 {
        return Err(AppError::msg("no instant answer results"));
    }
    Ok(out)
}

/// Fallback: DuckDuckGo HTML search with parsing.
async fn duckduckgo_html_search(query: &str) -> AppResult<String> {
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlencoding::encode(query)
    );
    let client = https_client_builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| AppError::msg(format!("Failed to build HTTP client: {e}")))?;
    let html = client.get(&url).send().await?.text().await?;
    parse_ddg_html(&html)
}

fn parse_ddg_html(html: &str) -> AppResult<String> {
    let document = Html::parse_document(html);
    let mut out = String::from("以下是与问题相关的网页搜索结果：\n\n");
    let mut count = 0;

    let result_sels = [".result", ".results_links", ".web-result"];
    let title_sels = [".result__title a", ".result__a", "a[href]"];
    let snippet_sels = [
        ".result__snippet",
        ".result__snippet.js-result-snippet",
        ".snippet",
    ];
    let link_re = Regex::new(r#"uddg=([^&"]+)"#).ok();

    for result_sel_str in &result_sels {
        if let Ok(rs) = Selector::parse(result_sel_str) {
            for result in document.select(&rs).take(5) {
                let title = find_text_with_fallback(&result, &title_sels);
                let snippet = find_text_with_fallback(&result, &snippet_sels);
                let link = find_link_with_fallback(&result, &link_re);

                if title.is_empty() && snippet.is_empty() {
                    continue;
                }
                count += 1;
                out.push_str(&format!(
                    "[{}] 标题: {}\n    链接: {}\n    摘要: {}\n\n",
                    count,
                    title.trim(),
                    link,
                    snippet.trim(),
                ));
            }
            if count > 0 {
                break;
            }
        }
    }

    // Generic heuristic: any <a href> with enough text — last resort
    if count == 0 {
        count = parse_generic_search_results(&document, &mut out);
    }

    tracing::debug!(count, "ddg_parse_complete");

    if count == 0 {
        out.push_str("(未找到搜索结果)\n");
    }
    Ok(out)
}

fn find_text_with_fallback(element: &scraper::ElementRef, selectors: &[&str]) -> String {
    for sel_str in selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(el) = element.select(&sel).next() {
                let text = el.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    return text;
                }
            }
        }
    }
    String::new()
}

fn find_link_with_fallback(element: &scraper::ElementRef, link_re: &Option<Regex>) -> String {
    if let Ok(sel) = Selector::parse("a") {
        if let Some(a) = element.select(&sel).next() {
            if let Some(href) = a.value().attr("href") {
                if let Some(re) = link_re {
                    if let Some(caps) = re.captures(href) {
                        if let Some(m) = caps.get(1) {
                            return urlencoding::decode(m.as_str())
                                .unwrap_or_default()
                                .to_string();
                        }
                    }
                }
                return href.to_string();
            }
        }
    }
    String::new()
}

/// Generic heuristic: scan <a href> elements for likely search result links.
fn parse_generic_search_results(document: &Html, out: &mut String) -> usize {
    let mut count = 0;
    if let Ok(sel) = Selector::parse("a[href]") {
        for el in document.select(&sel) {
            if count >= 5 {
                break;
            }
            let href = el.value().attr("href").unwrap_or("");
            let text = el.text().collect::<String>().trim().to_string();
            if text.len() < 10
                || href.starts_with("javascript:")
                || href.starts_with('#')
                || href.contains("duckduckgo.com")
            {
                continue;
            }
            count += 1;
            out.push_str(&format!(
                "[{}] 标题: {}\n    链接: {}\n\n",
                count, text, href
            ));
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_backend_respects_mode() {
        use crate::llm::web_search_config::WebSearchBackendMode;

        let db = Database::open_in_memory().expect("mem db");
        let prefs = WebSearchPreferences {
            backend_mode: WebSearchBackendMode::Duckduckgo,
            ..Default::default()
        };
        assert_eq!(
            expected_search_backend_for_connectivity(&db, &prefs),
            WebSearchEffectiveBackend::Duckduckgo
        );
    }

    #[test]
    fn expected_backend_uses_minimax_configured_marker() {
        let db = Database::open_in_memory().expect("mem db");
        let prefs = WebSearchPreferences {
            backend_mode: WebSearchBackendMode::Auto,
            ..Default::default()
        };

        assert_eq!(
            expected_search_backend_for_connectivity(&db, &prefs),
            WebSearchEffectiveBackend::Duckduckgo
        );

        crate::credentials::mark_api_key_configured(&db, MINIMAX_CREDENTIAL_SERVICE)
            .expect("mark minimax");

        assert_eq!(
            expected_search_backend_for_connectivity(&db, &prefs),
            WebSearchEffectiveBackend::Minimax
        );
    }

    #[test]
    fn search_cache_key_is_scoped_by_provider_config_and_vault() {
        let base = SearchCacheScope::native(
            WebSearchEffectiveBackend::Duckduckgo,
            None,
            SEARCH_CACHE_BROKER_VERSION,
        );
        let alternate_provider = SearchCacheScope {
            provider_id: "native.duckduckgo.alt".into(),
            ..base.clone()
        };
        let alternate_config = SearchCacheScope {
            provider_config_hash: "changed-config".into(),
            ..base.clone()
        };
        let alternate_vault = SearchCacheScope {
            vault_id: Some("vault-b".into()),
            ..base.clone()
        };

        let base_key = query_hash_key(
            "private query",
            WebSearchEffectiveBackend::Duckduckgo,
            "",
            &base,
        );

        assert_ne!(
            base_key,
            query_hash_key(
                "private query",
                WebSearchEffectiveBackend::Duckduckgo,
                "",
                &alternate_provider
            )
        );
        assert_ne!(
            base_key,
            query_hash_key(
                "private query",
                WebSearchEffectiveBackend::Duckduckgo,
                "",
                &alternate_config
            )
        );
        assert_ne!(
            base_key,
            query_hash_key(
                "private query",
                WebSearchEffectiveBackend::Duckduckgo,
                "",
                &alternate_vault
            )
        );
        assert!(!base_key.contains("private query"));
    }

    #[test]
    fn search_cache_reads_only_matching_provider_scope() {
        let db = Database::open_in_memory().expect("mem db");
        let base = SearchCacheScope::native(
            WebSearchEffectiveBackend::Duckduckgo,
            None,
            SEARCH_CACHE_BROKER_VERSION,
        );
        let alternate = SearchCacheScope {
            provider_config_hash: "changed-config".into(),
            ..base.clone()
        };
        let key = query_hash_key(
            "same query",
            WebSearchEffectiveBackend::Duckduckgo,
            "",
            &base,
        );

        cache_set_db(
            &db,
            &key,
            &hex::encode(&Sha256::digest("same query".as_bytes())[..8]),
            WebSearchEffectiveBackend::Duckduckgo.as_str(),
            &base,
            "base body",
        )
        .expect("store scoped cache");

        assert_eq!(
            cache_get_db(&db, &key, &base).expect("read base"),
            Some("base body".into())
        );
        assert_eq!(
            cache_get_db(&db, &key, &alternate).expect("read alternate"),
            None
        );
    }

    #[test]
    fn search_cache_lru_prunes_oldest_rows_over_limit() {
        let db = Database::open_in_memory().expect("mem db");
        let scope = SearchCacheScope::native(
            WebSearchEffectiveBackend::Duckduckgo,
            None,
            SEARCH_CACHE_BROKER_VERSION,
        );

        for (key, created_at) in [
            ("old", "2026-01-01T00:00:00Z"),
            ("middle", "2026-01-02T00:00:00Z"),
            ("new", "2026-01-03T00:00:00Z"),
        ] {
            cache_set_db(
                &db,
                key,
                "query-hash",
                WebSearchEffectiveBackend::Duckduckgo.as_str(),
                &scope,
                key,
            )
            .expect("store cache row");
            db.with_conn(|conn| {
                conn.execute(
                    "UPDATE search_cache SET created_at = ?2 WHERE cache_key = ?1",
                    rusqlite::params![key, created_at],
                )?;
                Ok::<(), crate::error::AppError>(())
            })
            .expect("set created_at");
        }

        assert_eq!(prune_search_cache_lru(&db, 2).expect("prune lru"), 1);
        assert_eq!(cache_get_db(&db, "old", &scope).expect("read old"), None);
        assert_eq!(
            cache_get_db(&db, "middle", &scope).expect("read middle"),
            Some("middle".into())
        );
        assert_eq!(
            cache_get_db(&db, "new", &scope).expect("read new"),
            Some("new".into())
        );
    }
}
