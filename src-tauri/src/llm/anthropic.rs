use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use serde_json::Value;
use tauri::Emitter;

use super::providers::{ANTHROPIC_API_VERSION, ANTHROPIC_DEFAULT_MAX_TOKENS};
use super::{ChatMessage, LlmStreamContext};
use crate::error::{AppError, AppResult};

/// 将聊天消息拆为 Anthropic `system` + `messages`（仅 user/assistant）。
pub fn split_anthropic_messages(
    messages: &[ChatMessage],
    system: Option<String>,
) -> (Option<String>, Vec<Value>) {
    let mut sys = system;
    let mut out = Vec::new();
    for m in messages {
        if m.role == "system" {
            sys = Some(match sys {
                Some(s) => format!("{s}\n\n{}", m.content),
                None => m.content.clone(),
            });
            continue;
        }
        let role = if m.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        out.push(serde_json::json!({
            "role": role,
            "content": m.content,
        }));
    }
    (sys, out)
}

/// 从 Anthropic SSE `data:` JSON 提取文本 token。
pub fn extract_text_delta_from_data(data: &str) -> Option<String> {
    let json: Value = serde_json::from_str(data).ok()?;
    if json["type"].as_str() != Some("content_block_delta") {
        return None;
    }
    if json["delta"]["type"].as_str() != Some("text_delta") {
        return None;
    }
    json["delta"]["text"].as_str().map(|s| s.to_string())
}

/// 从 SSE `data:` JSON 提取流式错误描述。
pub fn extract_error_from_data(data: &str) -> Option<String> {
    let json: Value = serde_json::from_str(data).ok()?;
    if json["type"].as_str() == Some("error") {
        return json["error"]["message"]
            .as_str()
            .or_else(|| json["message"].as_str())
            .map(|s| s.to_string());
    }
    None
}

/// 解析 Anthropic SSE 文本块中的 `data:` 行。
pub fn parse_anthropic_sse_lines(lines: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for line in lines.lines() {
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        if let Some(err) = extract_error_from_data(data) {
            tokens.push(format!("[error:{err}]"));
            continue;
        }
        if let Some(token) = extract_text_delta_from_data(data) {
            tokens.push(token);
        }
    }
    tokens
}

/// `POST /v1/messages`，SSE 流式，发出 `llm:token` / `llm:done` / `llm:error`。
pub async fn stream_anthropic_messages(ctx: &LlmStreamContext<'_>) -> AppResult<()> {
    let (system_prompt, anthropic_messages) =
        split_anthropic_messages(&ctx.messages, ctx.system.clone());
    if anthropic_messages.is_empty() {
        return Err(AppError::msg("Anthropic 需要至少一条 user/assistant 消息"));
    }

    let mut body = serde_json::json!({
        "model": ctx.model,
        "max_tokens": ANTHROPIC_DEFAULT_MAX_TOKENS,
        "stream": true,
        "messages": anthropic_messages,
    });
    if let Some(sys) = system_prompt {
        body["system"] = serde_json::Value::String(sys);
    }

    let url = format!("{}/messages", ctx.base);
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        HeaderName::from_static("x-api-key"),
        HeaderValue::from_str(ctx.api_key).map_err(|e| AppError::msg(e.to_string()))?,
    );
    headers.insert(
        HeaderName::from_static("anthropic-version"),
        HeaderValue::from_static(ANTHROPIC_API_VERSION),
    );

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let _ = ctx.app.emit(
            "llm:error",
            serde_json::json!({
                "request_id": ctx.request_id,
                "error": format!("{status}: {text}")
            }),
        );
        return Err(AppError::msg(format!("Anthropic API error: {status}")));
    }

    let mut stream = response.bytes_stream();
    let mut index = 0u64;
    let mut carry = String::new();

    while let Some(chunk) = stream.next().await {
        if *ctx.abort_flag.lock().expect("abort lock") {
            break;
        }
        let chunk = chunk?;
        carry.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = carry.find('\n') {
            let line: String = carry.drain(..=pos).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            if let Some(err) = extract_error_from_data(data) {
                let _ = ctx.app.emit(
                    "llm:error",
                    serde_json::json!({
                        "request_id": ctx.request_id,
                        "error": err
                    }),
                );
                continue;
            }
            if let Some(token) = extract_text_delta_from_data(data) {
                let _ = ctx.app.emit(
                    "llm:token",
                    serde_json::json!({
                        "request_id": ctx.request_id,
                        "token": token,
                        "index": index
                    }),
                );
                index += 1;
            }
        }
    }

    if !carry.trim().is_empty() {
        for token in parse_anthropic_sse_lines(&carry) {
            if token.starts_with("[error:") {
                continue;
            }
            let _ = ctx.app.emit(
                "llm:token",
                serde_json::json!({
                    "request_id": ctx.request_id,
                    "token": token,
                    "index": index
                }),
            );
            index += 1;
        }
    }

    let _ = ctx.app.emit(
        "llm:done",
        serde_json::json!({ "request_id": ctx.request_id }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_text_delta_from_content_block_delta() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        assert_eq!(extract_text_delta_from_data(data).as_deref(), Some("Hello"));
    }

    #[test]
    fn parse_sse_data_lines() {
        let block = "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n";
        let tokens = parse_anthropic_sse_lines(block);
        assert_eq!(tokens, vec!["Hi".to_string()]);
    }

    #[test]
    fn split_system_role_into_system_field() {
        let msgs = vec![
            ChatMessage {
                role: "system".into(),
                content: "sys".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: "hi".into(),
            },
        ];
        let (sys, out) = split_anthropic_messages(&msgs, None);
        assert_eq!(sys.as_deref(), Some("sys"));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["role"], "user");
    }
}
