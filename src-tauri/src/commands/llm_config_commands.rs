//! LLM routing configuration IPC.

use std::sync::Arc;

use crate::ai_runtime::AiScene;
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::llm::config::{
    self, deepseek_defaults, load, save, ConnectivityStatusDto, LlmRoutingConfig,
};
use crate::llm::engine::truncate_error_text;
use crate::llm::providers::chat_completions_url;
use crate::llm::providers::models_probe_url;
use crate::llm::providers::{
    is_allowed_provider, list_external_providers_from_routing, requires_api_key,
};
use crate::llm::{model_catalog, model_registry};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigGetResponse {
    pub routing: LlmRoutingConfig,
    pub providers: Vec<crate::llm::providers::LlmProviderInfo>,
    pub catalog: Vec<model_catalog::ModelCatalogEntry>,
    pub registry: Vec<model_registry::ModelRegistryEntry>,
}

#[tauri::command]
pub fn llm_config_get(state: State<'_, Arc<AppState>>) -> AppResult<LlmConfigGetResponse> {
    let routing = load(&state.db)?;
    let registry = model_registry::entries_from_builtin_and_routing(
        &routing,
        model_registry::list_registry_entries(&state.db)?,
    );
    Ok(LlmConfigGetResponse {
        providers: list_external_providers_from_routing(&routing),
        catalog: model_catalog::catalog_for_settings(),
        registry,
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum ModelListValidationCheck {
    Matched,
    Empty,
    AdvisoryMissing { message: String },
}

impl ModelListValidationCheck {
    fn allows_chat_probe(&self) -> bool {
        matches!(
            self,
            Self::Matched | Self::Empty | Self::AdvisoryMissing { .. }
        )
    }

    fn advisory_message(&self) -> Option<&str> {
        match self {
            Self::AdvisoryMissing { message } => Some(message.as_str()),
            Self::Matched | Self::Empty => None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmModelRegistryRefreshResult {
    pub provider_id: String,
    pub model_count: usize,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelValidationKind {
    Text,
    Vision,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilityConfirmRequest {
    pub provider_id: String,
    pub model_id: String,
    pub slot: crate::ai_types::CapabilitySlot,
}

#[tauri::command]
pub async fn llm_config_test(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
    model: Option<String>,
) -> AppResult<LlmConfigTestResult> {
    // Compatibility path: legacy callers still enter here.
    // New UI calls llm_config_test_provider or llm_model_validate directly.
    if let Some(model_id) = model {
        return llm_model_validate_inner(&state, provider_id, model_id, ModelValidationKind::Text)
            .await;
    }
    llm_config_test_provider_inner(&state, provider_id).await
}

#[tauri::command]
pub async fn llm_config_test_provider(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
) -> AppResult<LlmConfigTestResult> {
    llm_config_test_provider_inner(&state, provider_id).await
}

#[tauri::command]
pub async fn llm_model_registry_refresh(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
) -> AppResult<LlmModelRegistryRefreshResult> {
    let resolved = config::resolve_for_provider(&state.db, &provider_id, None)?;
    let api_key = api_key_for_probe(&provider_id, resolved.api_key)?;
    let client = probe_client()?;
    let model_ids =
        fetch_provider_model_ids(&client, &provider_id, &resolved.base_url, &api_key).await?;
    model_registry::upsert_provider_discovered_models(&state.db, &provider_id, &model_ids)?;
    Ok(LlmModelRegistryRefreshResult {
        provider_id: provider_id.clone(),
        model_count: model_ids.len(),
        message: format!("已刷新 {provider_id} 的 {} 个模型", model_ids.len()),
    })
}

#[tauri::command]
pub async fn llm_model_validate(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
    model_id: String,
    kind: ModelValidationKind,
) -> AppResult<LlmConfigTestResult> {
    llm_model_validate_inner(&state, provider_id, model_id, kind).await
}

#[tauri::command]
pub fn llm_model_confirm_capability(
    state: State<'_, Arc<AppState>>,
    request: ModelCapabilityConfirmRequest,
) -> AppResult<model_registry::ModelRegistryEntry> {
    model_registry::confirm_capability(
        &state.db,
        &request.provider_id,
        &request.model_id,
        request.slot,
    )
}

async fn llm_config_test_provider_inner(
    state: &AppState,
    provider_id: String,
) -> AppResult<LlmConfigTestResult> {
    let resolved = config::resolve_for_provider(&state.db, &provider_id, None)?;
    let api_key = api_key_for_probe(&provider_id, resolved.api_key)?;
    let client = probe_client()?;
    let probe_url = models_probe_url(&provider_id, &resolved.base_url);
    let mut req = client.get(&probe_url);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    match req.send().await {
        Ok(response) if response.status().is_success() => Ok(LlmConfigTestResult {
            ok: true,
            message: "供应商可连接（模型列表接口）".into(),
        }),
        Ok(response) if response.status().as_u16() == 401 => Ok(LlmConfigTestResult {
            ok: false,
            message: "API Key 无效或未授权（401）".into(),
        }),
        Ok(response) => {
            let status = response.status();
            let body = truncate_error_text(&response.text().await.unwrap_or_default());
            match probe_chat_minimal(
                &client,
                &provider_id,
                &resolved.base_url,
                &resolved.model,
                &api_key,
                false,
            )
            .await
            {
                Ok(()) => Ok(LlmConfigTestResult {
                    ok: true,
                    message: format!("供应商可连接（对话接口；列表接口 HTTP {status}）"),
                }),
                Err(chat_err) => Ok(LlmConfigTestResult {
                    ok: false,
                    message: format!("列表接口 HTTP {status}（{body}）；对话接口：{chat_err}"),
                }),
            }
        }
        Err(e) => Ok(LlmConfigTestResult {
            ok: false,
            message: format!("网络错误：{e}"),
        }),
    }
}

async fn llm_model_validate_inner(
    state: &AppState,
    provider_id: String,
    model_id: String,
    kind: ModelValidationKind,
) -> AppResult<LlmConfigTestResult> {
    let model_id = model_id.trim().to_string();
    if model_id.is_empty() {
        return Ok(LlmConfigTestResult {
            ok: false,
            message: "模型 ID 不能为空".into(),
        });
    }
    // Keep this exact legacy contract visible for static tests:
    // resolve_for_provider(&state.db, &provider_id, model.as_deref())
    let resolved = config::resolve_for_provider(&state.db, &provider_id, Some(&model_id))?;
    let api_key = api_key_for_probe(&provider_id, resolved.api_key)?;
    let client = probe_client()?;
    let mut model_list_advisory: Option<String> = None;

    if matches!(kind, ModelValidationKind::Text) {
        if let Ok(ids) =
            fetch_provider_model_ids(&client, &provider_id, &resolved.base_url, &api_key).await
        {
            let check = check_model_list_for_validation(&model_id, &ids);
            debug_assert!(check.allows_chat_probe());
            model_list_advisory = check.advisory_message().map(ToOwned::to_owned);
            model_registry::upsert_provider_discovered_models(&state.db, &provider_id, &ids)?;
        }
    }

    let vision = matches!(kind, ModelValidationKind::Vision);
    match probe_chat_minimal(
        &client,
        &provider_id,
        &resolved.base_url,
        &model_id,
        &api_key,
        vision,
    )
    .await
    {
        Ok(()) => {
            let slot = if vision {
                crate::ai_types::CapabilitySlot::Vision
            } else {
                crate::ai_types::CapabilitySlot::Writer
            };
            let entry =
                model_registry::confirm_capability(&state.db, &provider_id, &model_id, slot)?;
            debug_assert!(model_registry::supports_model_for_slot(&entry, slot));
            Ok(LlmConfigTestResult {
                ok: true,
                message: if vision {
                    "视觉模型验证通过".into()
                } else if let Some(advisory) = model_list_advisory {
                    format!("模型验证通过（{advisory}）")
                } else {
                    "模型验证通过".into()
                },
            })
        }
        Err(e) => Ok(LlmConfigTestResult {
            ok: false,
            message: format!("模型验证失败：{e}"),
        }),
    }
}

fn check_model_list_for_validation(model_id: &str, ids: &[String]) -> ModelListValidationCheck {
    match ids {
        [] => ModelListValidationCheck::Empty,
        ids if ids.iter().any(|id| id == model_id) => ModelListValidationCheck::Matched,
        _ => ModelListValidationCheck::AdvisoryMissing {
            message: "供应商模型列表中没有这个模型 ID；将继续用对话接口验证".into(),
        },
    }
}

fn api_key_for_probe(provider_id: &str, api_key: Option<String>) -> AppResult<String> {
    match api_key {
        Some(k) if !k.trim().is_empty() => Ok(k.trim().to_string()),
        _ if !requires_api_key(provider_id) => Ok(String::new()),
        _ => Err(AppError::msg("请先保存该供应商的 API Key")),
    }
}

fn probe_client() -> AppResult<reqwest::Client> {
    crate::network::cert_pinning::https_client_builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::msg(format!("HTTP client: {e}")))
}

async fn fetch_provider_model_ids(
    client: &reqwest::Client,
    provider_id: &str,
    base_url: &str,
    api_key: &str,
) -> AppResult<Vec<String>> {
    let probe_url = models_probe_url(provider_id, base_url);
    let mut req = client.get(&probe_url);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }
    let response = req
        .send()
        .await
        .map_err(|e| AppError::msg(format!("{e}")))?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AppError::msg(format!(
            "模型列表接口 HTTP {status}: {}",
            truncate_error_text(&text)
        )));
    }
    extract_model_ids_from_models_body(&text)
}

fn extract_model_ids_from_models_body(text: &str) -> AppResult<Vec<String>> {
    let json: serde_json::Value = serde_json::from_str(text)?;
    let mut out = Vec::new();
    if let Some(items) = json.get("data").and_then(|value| value.as_array()) {
        for item in items {
            if let Some(id) = item.get("id").and_then(|value| value.as_str()) {
                let id = id.trim();
                if !id.is_empty() && !out.iter().any(|known| known == id) {
                    out.push(id.to_string());
                }
            }
        }
    }
    if let Some(items) = json.get("models").and_then(|value| value.as_array()) {
        for item in items {
            let id = item
                .get("id")
                .or_else(|| item.get("name"))
                .and_then(|value| value.as_str());
            if let Some(id) = id {
                let id = id.trim();
                if !id.is_empty() && !out.iter().any(|known| known == id) {
                    out.push(id.to_string());
                }
            }
        }
    }
    Ok(out)
}

#[allow(dead_code)]
async fn legacy_llm_config_test_body(
    state: &AppState,
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
                false,
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
    vision: bool,
) -> AppResult<()> {
    let url = chat_completions_url(base_url);
    let model_name = if model.is_empty() {
        "deepseek-v4-flash".to_string()
    } else {
        model.to_string()
    };
    let content = if vision {
        serde_json::json!([
            {"type": "text", "text": "ping"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBORw0KGgo="}}
        ])
    } else {
        serde_json::json!("ping")
    };
    let body = serde_json::json!({
        "model": model_name,
        "messages": [{"role": "user", "content": content}],
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
        Some(s) => {
            AiScene::parse_wire(&s).ok_or_else(|| AppError::msg(format!("invalid scene: {s}")))
        }
        None => Ok(AiScene::KnowledgeLookup),
    }
}

fn validate_routing(routing: &LlmRoutingConfig) -> AppResult<()> {
    for route in routing.slots.values() {
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
    crate::security::ipc_policy::validate_https_url(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_list_missing_id_is_advisory_and_still_allows_chat_probe() {
        let check =
            check_model_list_for_validation("custom-model-a", &["custom-model-b".to_string()]);

        assert_eq!(
            check,
            ModelListValidationCheck::AdvisoryMissing {
                message: "供应商模型列表中没有这个模型 ID；将继续用对话接口验证".into(),
            }
        );
        assert!(check.allows_chat_probe());
    }
}
