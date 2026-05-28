//! MiniMax Token Plan 联网检索配置 IPC（Key 仍走 `credential_*`）。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app::AppState;
use crate::credentials::{self, MINIMAX_CREDENTIAL_SERVICE};
use crate::error::AppResult;
use crate::llm::minimax_search;
use crate::llm::web_search_config::{
    self, load as load_web_search_preferences, WebSearchBackendMode, WebSearchPreferences,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimaxConfigGetResponse {
    pub minimax_configured: bool,
    pub minimax_api_host: String,
    pub web_search_backend: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimaxConfigSetRequest {
    pub minimax_api_host: Option<String>,
    pub web_search_backend: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimaxConfigTestResult {
    pub ok: bool,
    pub message: String,
}

#[tauri::command]
pub fn minimax_config_get(state: State<'_, Arc<AppState>>) -> AppResult<MinimaxConfigGetResponse> {
    let prefs = load_web_search_preferences(&state.db)?;
    Ok(prefs_to_response(&prefs))
}

#[tauri::command]
pub fn minimax_config_set(
    state: State<'_, Arc<AppState>>,
    request: MinimaxConfigSetRequest,
) -> AppResult<MinimaxConfigGetResponse> {
    if let Some(host) = request.minimax_api_host {
        web_search_config::save_minimax_api_host(&state.db, &host)?;
    }
    if let Some(mode) = request.web_search_backend {
        web_search_config::save_web_search_backend(&state.db, WebSearchBackendMode::parse(&mode))?;
    }
    let prefs = load_web_search_preferences(&state.db)?;
    Ok(prefs_to_response(&prefs))
}

#[tauri::command]
pub async fn minimax_config_test(
    state: State<'_, Arc<AppState>>,
) -> AppResult<MinimaxConfigTestResult> {
    if !credentials::has_secret(MINIMAX_CREDENTIAL_SERVICE) {
        return Ok(MinimaxConfigTestResult {
            ok: false,
            message: "请先在上方保存 MiniMax Token Plan API Key".into(),
        });
    }
    let prefs = load_web_search_preferences(&state.db)?;
    match minimax_search::probe(prefs.minimax_api_host.as_str()).await {
        Ok(()) => Ok(MinimaxConfigTestResult {
            ok: true,
            message: "MiniMax 联网检索连接成功".into(),
        }),
        Err(e) => Ok(MinimaxConfigTestResult {
            ok: false,
            message: format!("{e}"),
        }),
    }
}

fn prefs_to_response(prefs: &WebSearchPreferences) -> MinimaxConfigGetResponse {
    MinimaxConfigGetResponse {
        minimax_configured: credentials::has_secret(MINIMAX_CREDENTIAL_SERVICE),
        minimax_api_host: prefs.minimax_api_host.clone(),
        web_search_backend: prefs.backend_mode.as_str().to_string(),
    }
}
