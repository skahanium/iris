use tauri::{AppHandle, State};

use crate::app::AppState;
use crate::error::AppResult;
use crate::llm::config;
use crate::llm::engine::{llm_abort, llm_generate_stream};
use crate::llm::providers::{list_providers, LlmProviderInfo};
use crate::llm::LlmGenerateParams;

#[tauri::command]
pub fn llm_providers() -> Vec<LlmProviderInfo> {
    list_providers()
}

#[tauri::command]
pub async fn llm_generate(
    app: AppHandle,
    state: State<'_, AppState>,
    params: LlmGenerateParams,
) -> AppResult<String> {
    let resolved =
        config::resolve_for_provider(&state.db, &params.provider, params.model.as_deref())?;
    let mut merged = params;
    merged.provider = resolved.provider_id;
    merged.model = Some(resolved.model);
    if merged.custom_base_url.is_none() {
        merged.custom_base_url = Some(resolved.base_url);
    }
    llm_generate_stream(app, &state.db, merged).await
}

#[tauri::command]
pub async fn llm_chat(
    app: AppHandle,
    state: State<'_, AppState>,
    params: LlmGenerateParams,
) -> AppResult<String> {
    llm_generate(app, state, params).await
}

#[tauri::command]
pub fn llm_abort_cmd(request_id: String) -> AppResult<()> {
    llm_abort(&request_id)
}
