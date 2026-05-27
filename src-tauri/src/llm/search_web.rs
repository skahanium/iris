use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use regex::Regex;
use scraper::{Html, Selector};
use std::sync::LazyLock;

use sha2::{Digest, Sha256};

use crate::credentials;
use crate::error::{AppError, AppResult};

/// Keyring 服务名，与前端 `BING_SEARCH_CREDENTIAL_SERVICE` 一致。
pub const BING_SEARCH_CREDENTIAL_SERVICE: &str = "iris/bing-search";

fn query_hash_key(query: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(query.as_bytes());
    hex::encode(hasher.finalize())
}

static SEARCH_CACHE: LazyLock<Mutex<HashMap<String, (Instant, String)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static LAST_REQUEST: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

const MAX_CACHE_ENTRIES: usize = 200;

/// Fetch web search context (DuckDuckGo default, Bing optional).
pub async fn fetch_search_context(query: &str, use_bing: bool) -> AppResult<String> {
    if let Some(cached) = cache_get(query) {
        return Ok(cached);
    }

    throttle()?;

    let body = if use_bing && credentials::has_secret(BING_SEARCH_CREDENTIAL_SERVICE) {
        match bing_search(query).await {
            Ok(b) => b,
            Err(_) => duckduckgo_search(query).await?,
        }
    } else {
        duckduckgo_search(query).await?
    };

    cache_set(query, &body);
    Ok(body)
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

fn cache_get(query: &str) -> Option<String> {
    let key = query_hash_key(query);
    let cache = SEARCH_CACHE.lock().ok()?;
    if let Some((t, v)) = cache.get(&key) {
        if t.elapsed() < Duration::from_secs(1800) {
            return Some(v.clone());
        }
    }
    None
}

fn cache_set(query: &str, value: &str) {
    if let Ok(mut cache) = SEARCH_CACHE.lock() {
        let key = query_hash_key(query);
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
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;
    let html = client.get(&url).send().await?.text().await?;
    parse_ddg_html(&html)
}

async fn bing_search(query: &str) -> AppResult<String> {
    let key = credentials::get_secret(BING_SEARCH_CREDENTIAL_SERVICE)?;
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.bing.microsoft.com/v7.0/search?q={}&count=5&mkt=zh-CN",
        urlencoding::encode(query)
    );
    let resp = client
        .get(&url)
        .header("Ocp-Apim-Subscription-Key", key)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let mut out = String::from("以下是与问题相关的网页搜索结果：\n\n");
    if let Some(items) = resp["webPages"]["value"].as_array() {
        for (i, item) in items.iter().take(5).enumerate() {
            let title = item["name"].as_str().unwrap_or("");
            let link = item["url"].as_str().unwrap_or("");
            let snippet = item["snippet"].as_str().unwrap_or("");
            out.push_str(&format!(
                "[{}] 标题: {}\n    链接: {}\n    摘要: {}\n\n",
                i + 1,
                title,
                link,
                snippet
            ));
        }
    }
    Ok(out)
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
