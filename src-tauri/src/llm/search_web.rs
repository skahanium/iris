use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{Datelike, Duration as ChronoDuration, Local, Utc};
use regex::Regex;
use scraper::{Html, Selector};
use serde::Serialize;
use std::sync::LazyLock;

use sha2::{Digest, Sha256};

use crate::credentials::{self, MINIMAX_CREDENTIAL_SERVICE};
use crate::error::{AppError, AppResult};
use crate::network::cert_pinning::pinned_client_builder;
use crate::storage::db::Database;

use super::minimax_search;
use super::web_search_config::{
    load as load_web_search_preferences, WebSearchBackendMode, WebSearchEffectiveBackend,
    WebSearchPreferences,
};

fn query_hash_key(query: &str, backend: WebSearchEffectiveBackend, minimax_model: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(query.as_bytes());
    hasher.update(backend.as_str().as_bytes());
    if backend == WebSearchEffectiveBackend::Minimax {
        hasher.update(minimax_model.trim().as_bytes());
    }
    hex::encode(hasher.finalize())
}

static LAST_REQUEST: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

/// 检索结果与所用后端。
#[derive(Debug, Clone)]
pub struct WebSearchFetchResult {
    pub body: String,
    pub backend: WebSearchEffectiveBackend,
}

/// Fetch web search context（MiniMax 优先，失败或无 Key 时降级 DuckDuckGo）。
pub async fn fetch_search_context(
    db: &Database,
    query: &str,
    prefs: &WebSearchPreferences,
) -> AppResult<WebSearchFetchResult> {
    let mode = prefs.backend_mode;
    let host = prefs.minimax_api_host.as_str();
    let model = prefs.minimax_search_model.as_str();

    tracing::info!(
        query_hash = %hex::encode(&Sha256::digest(query.as_bytes())[..8]),
        backend_mode = ?mode,
        "web_search_start"
    );

    if mode == WebSearchBackendMode::Duckduckgo {
        return fetch_duckduckgo_only(db, query).await;
    }

    if mode == WebSearchBackendMode::Minimax {
        return fetch_minimax_only(db, query, host, model).await;
    }

    // auto: MiniMax → DuckDuckGo
    if credentials::has_secret(MINIMAX_CREDENTIAL_SERVICE) {
        match fetch_minimax_cached(db, query, host, model).await {
            Ok(r) => return Ok(r),
            Err(e) => {
                tracing::warn!("MiniMax search failed, falling back: {e}");
            }
        }
    }

    fetch_duckduckgo_only(db, query).await
}

/// 从数据库加载偏好并检索。
pub async fn fetch_search_context_for_db(
    db: &Database,
    query: &str,
) -> AppResult<WebSearchFetchResult> {
    let prefs = load_web_search_preferences(db)?;
    fetch_search_context(db, query, &prefs).await
}

async fn fetch_minimax_only(
    db: &Database,
    query: &str,
    host: &str,
    model: &str,
) -> AppResult<WebSearchFetchResult> {
    if !credentials::has_secret(MINIMAX_CREDENTIAL_SERVICE) {
        return Err(AppError::msg("未配置 MiniMax API Key"));
    }
    fetch_minimax_cached(db, query, host, model).await
}

async fn fetch_minimax_cached(
    db: &Database,
    query: &str,
    host: &str,
    model: &str,
) -> AppResult<WebSearchFetchResult> {
    let backend = WebSearchEffectiveBackend::Minimax;
    let key = query_hash_key(query, backend, model);
    if let Some(cached) = cache_get_db(db, &key)? {
        return Ok(WebSearchFetchResult {
            body: cached,
            backend,
        });
    }
    throttle().await?;
    let body = minimax_search::search(query, host, model).await?;
    cache_set_db(
        db,
        &key,
        &hex::encode(&Sha256::digest(query.as_bytes())[..8]),
        backend.as_str(),
        &body,
    )?;
    Ok(WebSearchFetchResult { body, backend })
}

async fn fetch_duckduckgo_only(db: &Database, query: &str) -> AppResult<WebSearchFetchResult> {
    let backend = WebSearchEffectiveBackend::Duckduckgo;
    let key = query_hash_key(query, backend, "");
    if let Some(cached) = cache_get_db(db, &key)? {
        return Ok(WebSearchFetchResult {
            body: cached,
            backend,
        });
    }
    throttle().await?;
    let body = duckduckgo_search(query).await?;
    cache_set_db(
        db,
        &key,
        &hex::encode(&Sha256::digest(query.as_bytes())[..8]),
        backend.as_str(),
        &body,
    )?;
    Ok(WebSearchFetchResult { body, backend })
}

/// 联网注入元数据（供前端展示与调试）。
#[derive(Debug, Clone, Serialize)]
pub struct WebSearchInjectMeta {
    pub injected: bool,
    pub result_count: usize,
    /// 是否在上下文中附加了本机日历日期（问「今天几号」类问题）。
    pub used_local_date: bool,
    pub backend: String,
}

/// 带联网偏好与用户库设置的拼接。
pub async fn prepend_web_search_context_for_db(
    db: &Database,
    user_content: &str,
) -> AppResult<(String, WebSearchInjectMeta)> {
    let prefs = load_web_search_preferences(db)?;
    prepend_web_search_context_with_prefs(db, user_content, &prefs).await
}

async fn prepend_web_search_context_with_prefs(
    db: &Database,
    user_content: &str,
    prefs: &WebSearchPreferences,
) -> AppResult<(String, WebSearchInjectMeta)> {
    let mut prefix = String::new();
    let used_local_date = if asks_for_today_date(user_content) {
        prefix.push_str(&local_date_line_zh());
        prefix.push('\n');
        prefix.push('\n');
        true
    } else {
        false
    };

    let query = normalize_search_query(user_content);
    let fetched = fetch_search_context(db, &query, prefs).await?;
    let result_count = count_search_results(&fetched.body);
    prefix.push_str(&fetched.body);

    let meta = WebSearchInjectMeta {
        injected: true,
        result_count,
        used_local_date,
        backend: fetched.backend.as_str().to_string(),
    };
    Ok((format!("{prefix}\n\n用户问题: {user_content}"), meta))
}

/// 从用户原文提取更适合检索的查询词。
pub fn normalize_search_query(user_content: &str) -> String {
    let mut q = user_content.trim().to_string();
    for phrase in [
        "请帮我搜索",
        "请帮我搜",
        "可以联网搜索",
        "请联网搜索",
        "联网搜索",
        "上网查一下",
        "帮我搜一下",
        "帮我搜索",
        "帮我查一下",
        "帮我上网搜",
        "请搜索",
        "搜索一下",
        "查一下",
        "please search",
        "look up",
        "search for",
    ] {
        q = q.replace(phrase, "");
    }
    q = q
        .trim_matches(['？', '?', '。', '，', ' ', '\n', '!', '！', '：', ':'])
        .trim()
        .to_string();
    if asks_for_today_date(user_content) {
        return "今天 公历 日期 星期".to_string();
    }
    if q.is_empty() {
        user_content.trim().to_string()
    } else {
        q
    }
}

/// 用户是否在问「今天几月几日」等需实时日期的问题。
pub fn asks_for_today_date(text: &str) -> bool {
    let t = text.to_lowercase();
    let has_today = t.contains("今天")
        || t.contains("今日")
        || t.contains("今儿")
        || t.contains("today")
        || t.contains("current date")
        || t.contains("当前日期")
        || t.contains("现在日期")
        || t.contains("今天的日期");
    if t.contains("today's date") || t.contains("todays date") {
        return true;
    }
    let asks_date = t.contains("几月")
        || t.contains("几号")
        || t.contains("几日")
        || t.contains("日期")
        || t.contains("星期")
        || t.contains("星期几")
        || t.contains("what date")
        || t.contains("what day")
        || t.contains("what's the date")
        || t.contains("whats the date");
    has_today && asks_date
}

fn local_date_line_zh() -> String {
    let now = Local::now();
    let weekday = match now.weekday() {
        chrono::Weekday::Mon => "一",
        chrono::Weekday::Tue => "二",
        chrono::Weekday::Wed => "三",
        chrono::Weekday::Thu => "四",
        chrono::Weekday::Fri => "五",
        chrono::Weekday::Sat => "六",
        chrono::Weekday::Sun => "日",
    };
    format!(
        "【本机日期】{}年{}月{}日（星期{}）。回答「今天几号」类问题时请优先采用此日期，网页摘要仅作补充。",
        now.year(),
        now.month(),
        now.day(),
        weekday
    )
}

pub fn count_search_results(body: &str) -> usize {
    body.lines()
        .filter(|line| {
            let t = line.trim_start();
            t.starts_with('[') && t.contains("] 标题:")
        })
        .count()
}

/// 推断连通性展示用的「预期主后端」（未实际发请求）。
pub fn expected_search_backend_for_connectivity(
    prefs: &WebSearchPreferences,
) -> WebSearchEffectiveBackend {
    match prefs.backend_mode {
        WebSearchBackendMode::Minimax => {
            if credentials::has_secret(MINIMAX_CREDENTIAL_SERVICE) {
                WebSearchEffectiveBackend::Minimax
            } else {
                WebSearchEffectiveBackend::Duckduckgo
            }
        }
        WebSearchBackendMode::Duckduckgo => WebSearchEffectiveBackend::Duckduckgo,
        WebSearchBackendMode::Auto => {
            if credentials::has_secret(MINIMAX_CREDENTIAL_SERVICE) {
                WebSearchEffectiveBackend::Minimax
            } else {
                WebSearchEffectiveBackend::Duckduckgo
            }
        }
    }
}

async fn throttle() -> AppResult<()> {
    let need_wait = {
        let last = LAST_REQUEST
            .lock()
            .map_err(|_| AppError::msg("Lock error"))?;
        if let Some(t) = *last {
            let elapsed = t.elapsed();
            if elapsed < Duration::from_secs(2) {
                Some(Duration::from_secs(2) - elapsed)
            } else {
                None
            }
        } else {
            None
        }
    }; // lock dropped here

    if let Some(wait) = need_wait {
        tokio::time::sleep(tokio::time::Duration::from_millis(wait.as_millis() as u64)).await;
    }

    LAST_REQUEST
        .lock()
        .map_err(|_| AppError::msg("Lock error"))?
        .replace(Instant::now());
    Ok(())
}

fn cache_get_db(db: &Database, key: &str) -> AppResult<Option<String>> {
    db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT body FROM search_cache WHERE cache_key = ?1 AND expires_at > datetime('now')",
        )?;
        let mut rows = stmt.query([key])?;
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
    body: &str,
) -> AppResult<()> {
    let created_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let expires_at = (Utc::now() + ChronoDuration::hours(6))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO search_cache (cache_key, query_hash, backend, body, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(cache_key) DO UPDATE SET
               body = excluded.body,
               created_at = excluded.created_at,
               expires_at = excluded.expires_at",
            rusqlite::params![key, query_hash, backend, body, created_at, expires_at],
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
    let client = pinned_client_builder()
        .user_agent("Iris/1.0")
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
    let client = pinned_client_builder()
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
    fn normalize_strips_web_hint() {
        let q = normalize_search_query("今天是几月几日？可以联网搜索");
        assert_eq!(q, "今天 公历 日期 星期");
    }

    #[test]
    fn normalize_removes_various_hints() {
        assert_eq!(normalize_search_query("帮我查一下最新的法规"), "最新的法规");
        assert_eq!(
            normalize_search_query("请帮我搜索 党纪处分条例"),
            "党纪处分条例"
        );
        assert_eq!(normalize_search_query("search for AI safety"), "AI safety");
    }

    #[test]
    fn detects_today_date_query() {
        assert!(asks_for_today_date("今天是几月几日"));
        assert!(asks_for_today_date("What is today's date?"));
        assert!(asks_for_today_date("当前日期是什么？"));
        assert!(asks_for_today_date("今天星期几"));
        assert!(asks_for_today_date("What's the date today?"));
        assert!(!asks_for_today_date("2020年宪法修订内容"));
    }

    #[test]
    fn counts_result_lines() {
        let body = "以下是与问题相关的网页搜索结果：\n\n\
            [1] 标题: A\n    链接: https://a\n    摘要: x\n\n\
            [2] 标题: B\n    链接: https://b\n    摘要: y\n\n";
        assert_eq!(count_search_results(body), 2);
    }

    #[test]
    fn expected_backend_respects_mode() {
        use crate::llm::web_search_config::WebSearchBackendMode;

        let prefs = WebSearchPreferences {
            backend_mode: WebSearchBackendMode::Duckduckgo,
            ..Default::default()
        };
        assert_eq!(
            expected_search_backend_for_connectivity(&prefs),
            WebSearchEffectiveBackend::Duckduckgo
        );
    }
}
