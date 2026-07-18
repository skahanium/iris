use crate::llm::providers::{list_providers, LlmProviderInfo};

#[tauri::command]
pub fn llm_providers() -> Vec<LlmProviderInfo> {
    list_providers()
}
