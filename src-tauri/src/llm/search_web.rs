use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{Datelike, Local};
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

fn query_hash_key(query: &str, backend: WebSearchEffectiveBackend) -> String {
    let mut hasher = Sha256::new();
    hasher.update(query.as_bytes());
    hasher.update(backend.as_str().as_bytes());
    hex::encode(hasher.finalize())
}

static SEARCH_CACHE: LazyLock<Mutex<HashMap<String, (Instant, String)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static LAST_REQUEST: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

const MAX_CACHE_ENTRIES: usize = 200;

/// 检索结果与所用后端。
#[derive(Debug, Clone)]
pub struct WebSearchFetchResult {
    pub body: String,
    pub backend: WebSearchEffectiveBackend,
}

/// Fetch web search context（MiniMax 优先，失败或无 Key 时降级 DuckDuckGo）。
pub async fn fetch_search_context(
    query: &str,
    prefs: &WebSearchPreferences,
) -> AppResult<WebSearchFetchResult> {
    let mode = prefs.backend_mode;
    let host = prefs.minimax_api_host.as_str();

    if mode == WebSearchBackendMode::Duckduckgo {
        return fetch_duckduckgo_only(query).await;
    }

    if mode == WebSearchBackendMode::Minimax {
        return fetch_minimax_only(query, host).await;
    }

    // auto: MiniMax → DuckDuckGo
    if credentials::has_secret(MINIMAX_CREDENTIAL_SERVICE) {
        match fetch_minimax_cached(query, host).await {
            Ok(r) => return Ok(r),
            Err(e) => {
                tracing::warn!("MiniMax search failed, falling back: {e}");
            }
        }
    }

    fetch_duckduckgo_only(query).await
}

/// 从数据库加载偏好并检索。
pub async fn fetch_search_context_for_db(
    db: &Database,
    query: &str,
) -> AppResult<WebSearchFetchResult> {
    let prefs = load_web_search_preferences(db)?;
    fetch_search_context(query, &prefs).await
}

async fn fetch_minimax_only(query: &str, host: &str) -> AppResult<WebSearchFetchResult> {
    if !credentials::has_secret(MINIMAX_CREDENTIAL_SERVICE) {
        return Err(AppError::msg("未配置 MiniMax API Key"));
    }
    fetch_minimax_cached(query, host).await
}

async fn fetch_minimax_cached(query: &str, host: &str) -> AppResult<WebSearchFetchResult> {
    let backend = WebSearchEffectiveBackend::Minimax;
    if let Some(cached) = cache_get(query, backend) {
        return Ok(WebSearchFetchResult {
            body: cached,
            backend,
        });
    }
    throttle()?;
    let body = minimax_search::search(query, host).await?;
    cache_set(query, backend, &body);
    Ok(WebSearchFetchResult { body, backend })
}

async fn fetch_duckduckgo_only(query: &str) -> AppResult<WebSearchFetchResult> {
    let backend = WebSearchEffectiveBackend::Duckduckgo;
    if let Some(cached) = cache_get(query, backend) {
        return Ok(WebSearchFetchResult {
            body: cached,
            backend,
        });
    }
    throttle()?;
    let body = duckduckgo_search(query).await?;
    cache_set(query, backend, &body);
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
    prepend_web_search_context_with_prefs(user_content, &prefs).await
}

async fn prepend_web_search_context_with_prefs(
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
    let fetched = fetch_search_context(&query, prefs).await?;
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
        "可以联网搜索",
        "请联网搜索",
        "联网搜索",
        "上网查一下",
        "帮我搜一下",
    ] {
        q = q.replace(phrase, "");
    }
    q = q
        .trim_matches(['？', '?', '。', ' ', '\n'])
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
        || t.contains("current date");
    if t.contains("today's date") || t.contains("todays date") {
        return true;
    }
    let asks_date = t.contains("几月")
        || t.contains("几号")
        || t.contains("日期")
        || t.contains("星期")
        || t.contains("what date")
        || t.contains("what day");
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

fn throttle() -> AppResult<()> {
    let mut last = LAST_REQUEST
        .lock()
        .map_err(|_| AppError::msg("Lock error"))?;
    if let Some(t) = *last {
        let elapsed = t.elapsed();
        if elapsed < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_secs(2) - elapsed);
        }
    }
    *last = Some(Instant::now());
    Ok(())
}

fn cache_get(query: &str, backend: WebSearchEffectiveBackend) -> Option<String> {
    let key = query_hash_key(query, backend);
    let cache = SEARCH_CACHE.lock().ok()?;
    if let Some((t, v)) = cache.get(&key) {
        if t.elapsed() < Duration::from_secs(1800) {
            return Some(v.clone());
        }
    }
    None
}

fn cache_set(query: &str, backend: WebSearchEffectiveBackend, value: &str) {
    if let Ok(mut cache) = SEARCH_CACHE.lock() {
        let key = query_hash_key(query, backend);
        cache.insert(key, (Instant::now(), value.to_string()));
        if cache.len() > MAX_CACHE_ENTRIES {
            let mut entries: Vec<_> = cache.iter().collect();
            entries.sort_by_key(|(_, (t, _))| *t);
            let evict_count = MAX_CACHE_ENTRIES / 4;
            let keys_to_remove: Vec<String> = entries
                .iter()
                .take(evict_count)
                .map(|(k, _)| (*k).clone())
                .collect();
            for k in keys_to_remove {
                cache.remove(&k);
            }
        }
    }
}

async fn duckduckgo_search(query: &str) -> AppResult<String> {
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
    let result_sel = Selector::parse(".result").ok();
    let title_sel = Selector::parse(".result__title").ok();
    let snippet_sel = Selector::parse(".result__snippet").ok();

    let mut out = String::from("以下是与问题相关的网页搜索结果：\n\n");
    let mut count = 0;

    if let (Some(rs), Some(ts), Some(ss)) = (result_sel, title_sel, snippet_sel) {
        let link_re = Regex::new(r#"uddg=([^&"]+)"#).ok();
        for result in document.select(&rs).take(5) {
            let title = result
                .select(&ts)
                .next()
                .map(|e| e.text().collect::<String>())
                .unwrap_or_default();
            let snippet = result
                .select(&ss)
                .next()
                .map(|e| e.text().collect::<String>())
                .unwrap_or_default();
            let link = result
                .select(&Selector::parse("a").unwrap())
                .next()
                .and_then(|a| a.value().attr("href"))
                .map(|h| {
                    if let Some(re) = &link_re {
                        re.captures(h)
                            .and_then(|c| c.get(1))
                            .map(|m| {
                                urlencoding::decode(m.as_str())
                                    .unwrap_or_default()
                                    .to_string()
                            })
                            .unwrap_or_else(|| h.to_string())
                    } else {
                        h.to_string()
                    }
                })
                .unwrap_or_default();

            count += 1;
            out.push_str(&format!(
                "[{}] 标题: {}\n    链接: {}\n    摘要: {}\n\n",
                count,
                title.trim(),
                link,
                snippet.trim()
            ));
        }
    }

    if count == 0 {
        out.push_str("(未找到搜索结果)\n");
    }
    Ok(out)
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
    fn detects_today_date_query() {
        assert!(asks_for_today_date("今天是几月几日"));
        assert!(asks_for_today_date("What is today's date?"));
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
