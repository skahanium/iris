use reqwest::Client;

use crate::ai_types::{FunctionCall, LlmMessage, ProviderConfig, ToolCall};

use super::{messages_for_api, usage_impl::parse_usage};

/// Concrete [`LlmBackend`] implementation that calls an OpenAI-compatible
/// HTTP endpoint via `reqwest`. Used in production; tests can substitute a mock.
pub struct HttpLlmBackend {
    client: Client,
}

impl HttpLlmBackend {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

impl crate::ai_types::LlmBackend for HttpLlmBackend {
    async fn chat(
        &self,
        provider: &ProviderConfig,
        messages: &[LlmMessage],
        tools: &[serde_json::Value],
        max_tokens: Option<u32>,
        temperature: Option<f64>,
    ) -> Result<crate::ai_types::LlmBackendResponse, String> {
        let url = crate::llm::providers::chat_completions_url(&provider.base_url);
        let mut body = serde_json::json!({
            "model": provider.model,
            "messages": messages_for_api(messages),
        });
        if !tools.is_empty() {
            body["tools"] = serde_json::Value::Array(tools.to_vec());
        }
        if let Some(mt) = max_tokens {
            body["max_tokens"] = serde_json::json!(mt);
        }
        if let Some(temp) = temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");
        if let Some(api_key) = &provider.api_key {
            req = req.header("Authorization", format!("Bearer {}", api_key.as_str()));
        }

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("LLM request failed: {e}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format_llm_http_error(status, &text));
        }
        let text = response
            .text()
            .await
            .map_err(|e| format!("Failed to read body: {e}"))?;
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("JSON parse: {e}"))?;

        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string());
        let tool_calls = json["choices"][0]["message"]["tool_calls"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        Some(ToolCall {
                            id: tc["id"].as_str()?.to_string(),
                            call_type: tc["type"].as_str().unwrap_or("function").to_string(),
                            function: FunctionCall {
                                name: tc["function"]["name"].as_str()?.to_string(),
                                arguments: tc["function"]["arguments"]
                                    .as_str()
                                    .unwrap_or("{}")
                                    .to_string(),
                            },
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let usage = parse_usage(&json);
        let finish_reason = json["choices"][0]["finish_reason"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        Ok(crate::ai_types::LlmBackendResponse {
            content,
            tool_calls,
            usage,
            finish_reason,
        })
    }
}

pub(super) fn format_llm_http_error(status: reqwest::StatusCode, text: &str) -> String {
    let lower = text.to_lowercase();
    if status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        || lower.contains("service_unavailable")
        || lower.contains("too busy")
        || lower.contains("overloaded")
    {
        return "模型服务繁忙，请稍后重试或在设置中更换模型。".into();
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || lower.contains("rate limit") {
        return "请求过于频繁，请稍后再试。".into();
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || lower.contains("invalid_api_key") {
        return "API Key 无效或未配置，请在设置中检查。".into();
    }
    format!("模型请求失败（{}）", status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_http_error_does_not_echo_provider_body() {
        let message = format_llm_http_error(
            reqwest::StatusCode::BAD_REQUEST,
            "bad prompt: 用户原文和 provider echo",
        );

        assert_eq!(message, "模型请求失败（400 Bad Request）");
        assert!(!message.contains("用户原文"));
        assert!(!message.contains("provider echo"));
    }
}
