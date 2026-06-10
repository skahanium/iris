//! MiniMax Token Plan web search (`POST /v1/coding_plan/search`).

use serde::Deserialize;

use crate::credentials::{self, MINIMAX_CREDENTIAL_SERVICE};
use crate::error::{AppError, AppResult};
use crate::network::cert_pinning::create_https_client;

const SEARCH_PATH: &str = "/v1/coding_plan/search";
const MAX_RESULTS: usize = 5;

#[derive(Debug, Deserialize)]
struct MiniMaxBaseResp {
    status_code: i64,
    #[serde(default)]
    status_msg: String,
}

#[derive(Debug, Deserialize)]
struct MiniMaxSearchResponse {
    #[serde(default)]
    organic: Vec<MiniMaxOrganic>,
    #[serde(default)]
    related_searches: Vec<MiniMaxRelatedSearch>,
    #[serde(default)]
    base_resp: Option<MiniMaxBaseResp>,
}

#[derive(Debug, Deserialize)]
struct MiniMaxOrganic {
    #[serde(default)]
    title: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    date: String,
}

#[derive(Debug, Deserialize)]
struct MiniMaxRelatedSearch {
    #[serde(default)]
    query: String,
}

/// 构建 Coding Plan 搜索请求体（`model` 为空时不包含该字段）。
pub(crate) fn search_request_body(query: &str, model: &str) -> serde_json::Value {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        serde_json::json!({ "q": query })
    } else {
        serde_json::json!({ "q": query, "model": trimmed })
    }
}

/// 调用 MiniMax Coding Plan 搜索 API，返回与 [`crate::llm::search_web`] 统一的摘要文本块。
pub async fn search(query: &str, api_host: &str, model: &str) -> AppResult<String> {
    let api_key = credentials::get_secret(MINIMAX_CREDENTIAL_SERVICE)?;
    let host = normalize_api_host(api_host);

    let client = create_https_client()?;
    let url = format!("{host}{SEARCH_PATH}");

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .header("MM-API-Source", "Iris")
        .json(&search_request_body(query, model))
        .send()
        .await
        .map_err(|e| AppError::msg(format!("MiniMax 搜索请求失败: {e}")))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| AppError::msg(format!("MiniMax 搜索响应读取失败: {e}")))?;

    if !status.is_success() {
        return Err(AppError::msg(format!(
            "MiniMax 搜索 HTTP {}（响应已截断）",
            status
        )));
    }

    let data: MiniMaxSearchResponse = serde_json::from_str(&text)
        .map_err(|e| AppError::msg(format!("MiniMax 搜索响应解析失败: {e}")))?;

    if let Some(base) = &data.base_resp {
        if base.status_code != 0 {
            return Err(AppError::msg(format!(
                "MiniMax 搜索 API 错误 {}: {}",
                base.status_code, base.status_msg
            )));
        }
    }

    Ok(format_search_results(&data))
}

/// 探测 Key、Host 与模型配置是否可用（极简查询）。
pub async fn probe(api_host: &str, model: &str) -> AppResult<()> {
    let body = search("test", api_host, model).await?;
    if body.contains("(未找到搜索结果)") {
        // API 可达但无结果仍视为连通成功
        return Ok(());
    }
    Ok(())
}

fn normalize_api_host(host: &str) -> String {
    host.trim().trim_end_matches('/').to_string()
}

fn format_search_results(data: &MiniMaxSearchResponse) -> String {
    let mut out = String::from("以下是与问题相关的网页搜索结果：\n\n");
    let mut count = 0usize;

    for item in data.organic.iter().take(MAX_RESULTS) {
        count += 1;
        out.push_str(&format!(
            "[{count}] 标题: {}\n    链接: {}\n    摘要: {}",
            item.title.trim(),
            item.link.trim(),
            item.snippet.trim(),
        ));
        if !item.date.is_empty() {
            out.push_str(&format!("\n    日期: {}", item.date.trim()));
        }
        out.push_str("\n\n");
    }

    if count == 0 {
        out.push_str("(未找到搜索结果)\n");
    } else if !data.related_searches.is_empty() {
        out.push_str("相关搜索建议: ");
        let hints: Vec<_> = data
            .related_searches
            .iter()
            .take(3)
            .map(|r| r.query.trim())
            .filter(|s| !s.is_empty())
            .collect();
        out.push_str(&hints.join("；"));
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_organic_results() {
        let data = MiniMaxSearchResponse {
            organic: vec![MiniMaxOrganic {
                title: "示例".into(),
                link: "https://example.com".into(),
                snippet: "摘要".into(),
                date: "2025-01-01".into(),
            }],
            related_searches: vec![MiniMaxRelatedSearch {
                query: "相关词".into(),
            }],
            base_resp: Some(MiniMaxBaseResp {
                status_code: 0,
                status_msg: String::new(),
            }),
        };
        let out = format_search_results(&data);
        assert!(out.contains("[1] 标题: 示例"));
        assert!(out.contains("https://example.com"));
        assert!(out.contains("相关搜索建议"));
    }

    #[test]
    fn search_body_omits_model_when_empty() {
        let body = search_request_body("hello", "");
        assert_eq!(body, serde_json::json!({ "q": "hello" }));
    }

    #[test]
    fn search_body_includes_model_when_set() {
        let body = search_request_body("hello", "MiniMax-M2.5");
        assert_eq!(
            body,
            serde_json::json!({ "q": "hello", "model": "MiniMax-M2.5" })
        );
    }

    #[test]
    fn normalizes_host_trailing_slash() {
        assert_eq!(
            normalize_api_host("https://api.minimaxi.com/"),
            "https://api.minimaxi.com"
        );
    }
}
