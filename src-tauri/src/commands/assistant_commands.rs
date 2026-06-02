//! Unified assistant IPC facade — routes intents to existing workflows.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::ai_runtime::assistant_facade::AssistantIntent;
use crate::ai_runtime::chapter_workflow::ChapterInfo;
use crate::ai_runtime::document_workflow::DocumentCheckResult;
use crate::ai_runtime::retrieval_scope::ContextScopeDto;
use crate::ai_runtime::writing_workflow::WritingTaskOutput;
use crate::ai_runtime::{CitationCheckResult, OrganizeTaskResult};
use crate::app::AppState;
use crate::error::AppResult;

/// Unified assistant execution request (camelCase for TypeScript IPC).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantExecuteRequest {
    pub intent: AssistantIntent,
    pub message: String,
    #[serde(default)]
    pub note_path: Option<String>,
    #[serde(default)]
    pub note_content: Option<String>,
    #[serde(default)]
    pub web_authorized: bool,
    #[serde(default)]
    pub selection: Option<String>,
    #[serde(default)]
    pub cursor_context: Option<String>,
    #[serde(default)]
    pub paragraph_text: Option<String>,
    #[serde(default)]
    pub context_scope: Option<ContextScopeDto>,
    #[serde(default)]
    pub session_id: Option<i64>,
    #[serde(default)]
    pub selected_packet_ids: Option<Vec<String>>,
    #[serde(default)]
    pub chapter: Option<ChapterInfo>,
    #[serde(default)]
    pub document_check_type: Option<String>,
    #[serde(default)]
    pub organize_task_type: Option<String>,
    #[serde(default)]
    pub base_content_hash: Option<String>,
    /// 为 true 时创建新的 session 线程，不续接同 scene+笔记 的历史消息。
    #[serde(default)]
    pub new_session: bool,
}

/// Task body returned to the frontend (tagged union, wire-compatible).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AssistantExecuteBody {
    Chat {
        payload: serde_json::Value,
    },
    Writing {
        payload: WritingTaskOutput,
    },
    Citation {
        payload: CitationCheckResult,
    },
    Organize {
        payload: OrganizeTaskResult,
    },
    Research {
        payload: serde_json::Value,
    },
    Chapter {
        payload: crate::ai_runtime::chapter_workflow::ChapterWritingResult,
    },
    Document {
        payload: DocumentCheckResult,
    },
}

/// Unified harness metadata + flattened task body for IPC.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantExecuteResponse {
    #[serde(flatten)]
    pub body: AssistantExecuteBody,
    pub request_id: String,
    pub run_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_refresh_notice: Option<String>,
    pub artifacts: Vec<crate::ai_runtime::harness_task::HarnessArtifactWire>,
}

/// Route a unified assistant request through the harness task layer.
pub(crate) async fn route_assistant_execute(
    state: &AppState,
    app_handle: &AppHandle,
    request: AssistantExecuteRequest,
) -> AppResult<AssistantExecuteResponse> {
    let task_result = crate::ai_runtime::harness_task::run_harness_task(
        state,
        app_handle,
        crate::ai_runtime::harness_task::HarnessTaskRequest::from_assistant(request),
    )
    .await?;
    Ok(crate::ai_runtime::harness_task::map_task_result_to_response(
        task_result,
    ))
}

/// Unified assistant entry point for the React frontend.
#[tauri::command]
pub async fn assistant_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    request: AssistantExecuteRequest,
) -> AppResult<AssistantExecuteResponse> {
    route_assistant_execute(state.inner().as_ref(), &app_handle, request).await
}
