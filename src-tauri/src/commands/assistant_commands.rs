//! Unified assistant IPC facade — routes intents to existing workflows.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::ai_runtime::assistant_facade::{
    parse_document_check_type, parse_organize_task_type, AssistantIntent,
};
use crate::ai_runtime::chapter_workflow::{self, ChapterInfo, ChapterWritingInput};
use crate::ai_runtime::document_workflow::DocumentCheckInput;
use crate::ai_runtime::document_workflow::DocumentCheckResult;
use crate::ai_runtime::retrieval_scope::ContextScopeDto;
use crate::ai_runtime::writing_workflow;
use crate::ai_runtime::writing_workflow::WritingTaskOutput;
use crate::ai_runtime::{
    CitationCheckInput, CitationCheckResult, CitationCheckScope, OrganizeTaskInput,
    OrganizeTaskResult, OrganizeTaskScope,
};
use crate::app::AppState;
use crate::commands::writing_commands::WritingTaskInputIpc;
use crate::commands::{
    ai_commands, citation_commands, document_commands, organize_commands, research_commands,
    writing_commands,
};
use crate::error::{AppError, AppResult};

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
}

/// Tagged union returned to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AssistantExecuteResponse {
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

fn resolve_content_hash(note_content: Option<&str>, provided: Option<&str>) -> String {
    if let Some(hash) = provided.filter(|h| !h.is_empty()) {
        return hash.to_string();
    }
    note_content
        .map(writing_workflow::compute_content_hash)
        .unwrap_or_default()
}

fn citation_scope_from_dto(dto: Option<ContextScopeDto>) -> Option<CitationCheckScope> {
    dto.map(|scope| CitationCheckScope {
        paths: scope.paths,
        path_prefixes: scope.path_prefixes,
        corpus_ids: if scope.corpus_ids.is_empty() {
            None
        } else {
            Some(scope.corpus_ids)
        },
    })
}

fn organize_scope_from_dto(dto: Option<ContextScopeDto>) -> Option<OrganizeTaskScope> {
    dto.map(|scope| OrganizeTaskScope {
        paths: scope.paths,
        path_prefixes: scope.path_prefixes,
        corpus_ids: if scope.corpus_ids.is_empty() {
            None
        } else {
            Some(scope.corpus_ids)
        },
    })
}

fn resolve_chapter(
    chapter: Option<ChapterInfo>,
    note_content: Option<&str>,
) -> AppResult<ChapterInfo> {
    if let Some(ch) = chapter {
        return Ok(ch);
    }
    let content = note_content.ok_or_else(|| AppError::msg("章节任务需要笔记正文"))?;
    chapter_workflow::parse_chapters(content)
        .into_iter()
        .next()
        .ok_or_else(|| AppError::msg("当前文档没有可识别的章节结构"))
}

/// Route a unified assistant request to the appropriate workflow.
pub(crate) async fn route_assistant_execute(
    state: &AppState,
    app_handle: &AppHandle,
    request: AssistantExecuteRequest,
) -> AppResult<AssistantExecuteResponse> {
    match request.intent {
        AssistantIntent::Writing => {
            let note_path = request
                .note_path
                .ok_or_else(|| AppError::msg("写作任务需要 notePath"))?;
            let selection = request
                .selection
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::msg("写作任务需要选区"))?;
            let cursor_context = request
                .cursor_context
                .or(request.note_content.clone())
                .unwrap_or_default();
            let input = WritingTaskInputIpc {
                target_path: note_path,
                base_content_hash: resolve_content_hash(
                    request.note_content.as_deref(),
                    request.base_content_hash.as_deref(),
                ),
                selection: Some(selection),
                cursor_context,
                writing_goal: request.message,
                web_authorized: request.web_authorized,
            };
            let payload = writing_commands::execute_writing_task(state, app_handle, input).await?;
            Ok(AssistantExecuteResponse::Writing { payload })
        }
        AssistantIntent::Citation => {
            let note_path = request
                .note_path
                .ok_or_else(|| AppError::msg("引用检查需要 notePath"))?;
            let paragraph_text = request
                .paragraph_text
                .or(request.selection)
                .filter(|t| !t.is_empty())
                .ok_or_else(|| AppError::msg("引用检查需要段落或选区文本"))?;
            let input = CitationCheckInput {
                paragraph_text,
                document_path: note_path,
                scope: citation_scope_from_dto(request.context_scope),
                web_authorized: request.web_authorized,
            };
            let payload =
                citation_commands::execute_citation_check(state, app_handle, input).await?;
            Ok(AssistantExecuteResponse::Citation { payload })
        }
        AssistantIntent::Organize => {
            let input = OrganizeTaskInput {
                scope: organize_scope_from_dto(request.context_scope),
                task_type: parse_organize_task_type(request.organize_task_type.as_deref()),
            };
            let payload =
                organize_commands::execute_organize_task(state, app_handle, input).await?;
            Ok(AssistantExecuteResponse::Organize { payload })
        }
        AssistantIntent::Research => {
            let payload = research_commands::execute_research_task(
                state,
                app_handle,
                request.message,
                Some(request.web_authorized),
            )
            .await?;
            Ok(AssistantExecuteResponse::Research { payload })
        }
        AssistantIntent::Chapter => {
            let note_path = request
                .note_path
                .ok_or_else(|| AppError::msg("章节写作需要 notePath"))?;
            let chapter = resolve_chapter(request.chapter, request.note_content.as_deref())?;
            let input = ChapterWritingInput {
                target_path: note_path,
                base_content_hash: resolve_content_hash(
                    request.note_content.as_deref(),
                    request.base_content_hash.as_deref(),
                ),
                chapter,
                writing_goal: request.message,
                web_authorized: request.web_authorized,
            };
            let payload =
                document_commands::execute_chapter_writing(state, app_handle, input).await?;
            Ok(AssistantExecuteResponse::Chapter { payload })
        }
        AssistantIntent::Document => {
            let note_path = request
                .note_path
                .ok_or_else(|| AppError::msg("文档检查需要 notePath"))?;
            let note_content = request
                .note_content
                .clone()
                .ok_or_else(|| AppError::msg("文档检查需要 noteContent"))?;
            let input = DocumentCheckInput {
                target_path: note_path,
                content: note_content.clone(),
                check_type: parse_document_check_type(request.document_check_type.as_deref()),
                web_authorized: request.web_authorized,
                base_content_hash: resolve_content_hash(
                    Some(note_content.as_str()),
                    request.base_content_hash.as_deref(),
                ),
            };
            let payload =
                document_commands::execute_document_check(state, app_handle, input).await?;
            Ok(AssistantExecuteResponse::Document { payload })
        }
        AssistantIntent::Chat | AssistantIntent::Knowledge => {
            let scene = request.intent.scene().profile().to_string();
            let payload = ai_commands::execute_ai_send_message(
                state,
                app_handle,
                scene,
                request.session_id,
                request.message,
                request.selected_packet_ids,
                request.note_path,
                request.context_scope,
                Some(request.web_authorized),
            )
            .await?;
            Ok(AssistantExecuteResponse::Chat { payload })
        }
    }
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
