//! LLM routing configuration IPC.

use std::sync::Arc;

use crate::ai_runtime::AiScene;
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::llm::config::{
    self, deepseek_defaults, load, save, ConnectivityStatusDto, LlmRoutingConfig,
};
use crate::llm::engine::truncate_error_text;
use crate::llm::model_catalog;
use crate::llm::providers::chat_completions_url;
use crate::llm::providers::models_probe_url;
use crate::llm::providers::{
    is_allowed_provider, list_external_providers_from_routing, requires_api_key,
};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigGetResponse {
    pub routing: LlmRoutingConfig,
    pub providers: Vec<crate::llm::providers::LlmProviderInfo>,
    pub catalog: Vec<model_catalog::ModelCatalogEntry>,
}

#[tauri::command]
pub fn llm_config_get(state: State<'_, Arc<AppState>>) -> AppResult<LlmConfigGetResponse> {
    let routing = load(&state.db)?;
    Ok(LlmConfigGetResponse {
        providers: list_external_providers_from_routing(&routing),
        catalog: model_catalog::catalog_for_settings(),
        routing,
    })
}

#[tauri::command]
pub fn llm_config_set(state: State<'_, Arc<AppState>>, routing: LlmRoutingConfig) -> AppResult<()> {
    validate_routing(&routing)?;
    save(&state.db, &routing)
}

#[tauri::command]
pub fn llm_config_apply_deepseek_defaults(
    state: State<'_, Arc<AppState>>,
) -> AppResult<LlmRoutingConfig> {
    let defaults = deepseek_defaults();
    save(&state.db, &defaults)?;
    Ok(defaults)
}

#[tauri::command]
pub fn connectivity_status(
    state: State<'_, Arc<AppState>>,
    scene: Option<String>,
) -> AppResult<ConnectivityStatusDto> {
    let scene = parse_scene(scene)?;
    config::connectivity_status(&state.db, scene)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigTestResult {
    pub ok: bool,
    pub message: String,
}

#[tauri::command]
pub async fn llm_config_test(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
    model: Option<String>,
) -> AppResult<LlmConfigTestResult> {
    let resolved = config::resolve_for_provider(&state.db, &provider_id, model.as_deref())?;
    let api_key = match resolved.api_key {
        Some(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ if !requires_api_key(&provider_id) => String::new(),
        _ => {
            return Ok(LlmConfigTestResult {
                ok: false,
                message: "请先在上方保存该厂商的 API Key".into(),
            });
        }
    };

    let client = crate::network::cert_pinning::https_client_builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::msg(format!("HTTP client: {e}")))?;

    let probe_url = models_probe_url(&provider_id, &resolved.base_url);
    let mut req = client.get(&probe_url);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    match req.send().await {
        Ok(response) if response.status().is_success() => Ok(LlmConfigTestResult {
            ok: true,
            message: "连接成功（模型列表）".into(),
        }),
        Ok(response) if response.status().as_u16() == 401 => Ok(LlmConfigTestResult {
            ok: false,
            message: "API Key 无效或未授权（401）".into(),
        }),
        Ok(response) => {
            let status = response.status();
            let body = truncate_error_text(&response.text().await.unwrap_or_default());
            // /models 探测失败时用最小对话请求复核 Key
            match probe_chat_minimal(
                &client,
                &provider_id,
                &resolved.base_url,
                &resolved.model,
                &api_key,
            )
            .await
            {
                Ok(()) => Ok(LlmConfigTestResult {
                    ok: true,
                    message: format!("连接成功（对话接口；列表探测返回 HTTP {status}）"),
                }),
                Err(chat_err) => Ok(LlmConfigTestResult {
                    ok: false,
                    message: format!("列表探测 HTTP {status}（{body}）；对话探测：{chat_err}"),
                }),
            }
        }
        Err(e) => Ok(LlmConfigTestResult {
            ok: false,
            message: format!("网络错误：{e}"),
        }),
    }
}

async fn probe_chat_minimal(
    client: &reqwest::Client,
    _provider_id: &str,
    base_url: &str,
    model: &str,
    api_key: &str,
) -> AppResult<()> {
    let url = chat_completions_url(base_url);
    let model_name = if model.is_empty() {
        "deepseek-v4-flash".to_string()
    } else {
        model.to_string()
    };
    let body = serde_json::json!({
        "model": model_name,
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 1,
        "stream": false
    });
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::msg(format!("{e}")))?;

    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let text = truncate_error_text(&response.text().await.unwrap_or_default());
    Err(AppError::msg(format!("HTTP {status}: {text}")))
}

fn parse_scene(scene: Option<String>) -> AppResult<AiScene> {
    match scene {
        Some(s) => serde_json::from_str(&format!("\"{s}\""))
            .map_err(|e| AppError::msg(format!("invalid scene: {e}"))),
        None => Ok(AiScene::KnowledgeLookup),
    }
}

fn validate_routing(routing: &LlmRoutingConfig) -> AppResult<()> {
    for route in routing.slots.values() {
        validate_route(&route.provider_id, &route.model, routing)?;
    }
    for route in routing.scenes.values() {
        validate_route(&route.provider_id, &route.model, routing)?;
    }
    for id in routing.providers.keys() {
        if !is_allowed_provider(id) {
            return Err(AppError::msg(format!("未知厂商配置项: {id}")));
        }
    }
    for row in routing.providers.values() {
        if let Some(url) = row.base_url.as_deref() {
            if !url.trim().is_empty() {
                validate_provider_base_url(url)?;
            }
        }
    }
    Ok(())
}

fn validate_route(provider_id: &str, model: &str, routing: &LlmRoutingConfig) -> AppResult<()> {
    if !is_allowed_provider(provider_id) {
        return Err(AppError::msg(format!("未知厂商: {provider_id}")));
    }
    if model.trim().is_empty() {
        return Err(AppError::msg("模型 ID 不能为空"));
    }
    if crate::llm::providers::is_custom_provider(provider_id)
        && !routing.providers.contains_key(provider_id)
    {
        return Err(AppError::msg(format!(
            "路由引用了未配置的自定义端点: {provider_id}"
        )));
    }
    Ok(())
}

fn validate_provider_base_url(url: &str) -> AppResult<()> {
    let trimmed = url.trim();
    if trimmed.starts_with("http://127.0.0.1")
        || trimmed.starts_with("http://localhost")
        || trimmed.starts_with("http://[::1]")
    {
        return Ok(());
    }
    crate::security::ipc_policy::validate_https_url(trimmed)
}
