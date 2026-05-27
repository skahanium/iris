use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use super::anthropic;
use super::providers::{api_base, credential_service, uses_anthropic_messages_api};
use super::{ChatMessage, LlmGenerateParams, LlmStreamContext};
use crate::credentials;
use crate::error::{AppError, AppResult};
use crate::llm::search_web::fetch_search_context;

const REQUEST_TIMEOUT_SECS: u64 = 60;

struct AbortFlag(Arc<Mutex<bool>>);

static IN_FLIGHT: Mutex<Option<HashMap<String, AbortFlag>>> = Mutex::new(None);

/// 截断错误响应文本，防止大段 HTML/JSON 错误体泄露到前端
pub(crate) fn truncate_error_text(text: &str) -> String {
    const MAX_LEN: usize = 500;
    if text.len() <= MAX_LEN {
        text.to_string()
    } else {
        format!("{}…(已截断，共 {} 字符)", &text[..MAX_LEN], text.len())
    }
}

fn in_flight() -> std::sync::MutexGuard<'static, Option<HashMap<String, AbortFlag>>> {
    IN_FLIGHT.lock().expect("in_flight lock")
}

fn resolve_model(provider: &str, model: Option<String>) -> String {
    model.unwrap_or_else(|| {
        super::providers::list_providers()
            .into_iter()
            .find(|p| p.id == provider)
            .map(|p| p.default_model)
            .unwrap_or_else(|| "gpt-4o-mini".to_string())
    })
}

async fn apply_web_search(messages: &mut [ChatMessage], enabled: bool) -> AppResult<()> {
    if !enabled {
        return Ok(());
    }
    if let Some(last) = messages.last_mut() {
        if last.role == "user" {
            let ctx = fetch_search_context(&last.content, true).await?;
            last.content = format!("{ctx}\n\n用户问题: {}", last.content);
        }
    }
    Ok(())
}

/// OpenAI 兼容 `POST /chat/completions` 流式。
async fn stream_openai_compatible(ctx: LlmStreamContext<'_>) -> AppResult<()> {
    let mut body = serde_json::json!({
        "model": ctx.model,
        "messages": ctx.messages,
        "stream": true
    });
    if let Some(sys) = &ctx.system {
        let mut msgs = vec![serde_json::json!({"role": "system", "content": sys})];
        for m in &ctx.messages {
            msgs.push(serde_json::json!({"role": m.role, "content": m.content}));
        }
        body["messages"] = serde_json::Value::Array(msgs);
    }

    let url = format!("{}/chat/completions", ctx.base);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::msg(format!("Failed to build HTTP client: {e}")))?;
    let response = client
        .post(&url)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {}", ctx.api_key))
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let truncated = truncate_error_text(&text);
        let _ = ctx.app.emit(
            "llm:error",
            serde_json::json!({
                "request_id": ctx.request_id,
                "error": format!("{status}: {truncated}")
            }),
        );
        return Err(AppError::msg(format!("LLM API error: {status}")));
    }

    let mut stream = response.bytes_stream();
    let mut index = 0u64;

    while let Some(chunk) = stream.next().await {
        if *ctx.abort_flag.lock().expect("abort lock") {
            break;
        }
        let chunk = chunk?;
        let text = String::from_utf8_lossy(&chunk);
        for line in text.lines() {
            let line = line.trim();
            if !line.starts_with("data:") {
                continue;
            }
            let data = line.trim_start_matches("data:").trim();
            if data == "[DONE]" {
                break;
            }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(token) = json["choices"][0]["delta"]["content"].as_str() {
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
    }

    let _ = ctx.app.emit(
        "llm:done",
        serde_json::json!({ "request_id": ctx.request_id }),
    );
    Ok(())
}

/// Stream LLM completion and emit `llm:token` events.
pub async fn llm_generate_stream(app: AppHandle, params: LlmGenerateParams) -> AppResult<String> {
    let request_id = Uuid::new_v4().to_string();
    let abort_flag = Arc::new(Mutex::new(false));
    {
        let mut map = in_flight();
        map.get_or_insert_with(HashMap::new)
            .insert(request_id.clone(), AbortFlag(abort_flag.clone()));
    }

    let api_key = credentials::get_secret(&credential_service(&params.provider))?;
    let base = api_base(&params.provider, params.custom_base_url.as_deref());
    let model = resolve_model(&params.provider, params.model.clone());

    let mut messages = params.messages.clone();
    apply_web_search(&mut messages, params.web_search.unwrap_or(false)).await?;

    let stream_ctx = LlmStreamContext {
        app: &app,
        request_id: &request_id,
        abort_flag: abort_flag.clone(),
        api_key: &api_key,
        base: &base,
        model: &model,
        messages,
        system: params.system.clone(),
    };

    let result = if uses_anthropic_messages_api(&params.provider) {
        anthropic::stream_anthropic_messages(&stream_ctx).await
    } else {
        stream_openai_compatible(stream_ctx).await
    };

    in_flight().as_mut().and_then(|m| m.remove(&request_id));

    result?;
    Ok(request_id)
}

/// Abort an in-flight LLM request.
pub fn llm_abort(request_id: &str) -> AppResult<()> {
    if let Some(map) = in_flight().as_mut() {
        if let Some(flag) = map.get(request_id) {
            *flag.0.lock().expect("abort lock") = true;
        }
    }
    Ok(())
}
