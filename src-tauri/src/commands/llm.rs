use tauri::AppHandle;

use crate::error::AppResult;
use crate::llm::engine::{llm_abort, llm_generate_stream};
use crate::llm::providers::{list_providers, LlmProviderInfo};
use crate::llm::LlmGenerateParams;

#[tauri::command]
pub fn llm_providers() -> Vec<LlmProviderInfo> {
    list_providers()
}

#[tauri::command]
pub async fn llm_generate(app: AppHandle, params: LlmGenerateParams) -> AppResult<String> {
    llm_generate_stream(app, params).await
}

#[tauri::command]
pub async fn llm_chat(app: AppHandle, params: LlmGenerateParams) -> AppResult<String> {
    llm_generate_stream(app, params).await
}

#[tauri::command]
pub fn llm_abort_cmd(request_id: String) -> AppResult<()> {
    llm_abort(&request_id)
}
