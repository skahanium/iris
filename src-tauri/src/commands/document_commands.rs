//! Chapter and document level writing IPC commands.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::ai_runtime::chapter_workflow::{
    self, ChapterInfo, ChapterWritingInput, ChapterWritingResult,
};
use crate::ai_runtime::document_workflow::{self, DocumentCheckInput, DocumentCheckResult};
use crate::ai_runtime::retrieval_broker::{RetrievalLayers, RetrievalRequest};
use crate::ai_runtime::retrieval_scope::RetrievalScope;
use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
use crate::ai_runtime::TokenUsage;
use crate::app::AppState;
use crate::error::AppResult;

/// Execute a chapter-level writing task.
#[tauri::command]
pub async fn chapter_writing_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: ChapterWritingInput,
) -> AppResult<ChapterWritingResult> {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Start trace
    TraceRecorder::start(
        &state.db,
        &request_id,
        crate::ai_runtime::AiScene::DraftingAssist,
    )?;

    // Detect chapter intent
    let intent = chapter_workflow::detect_chapter_intent(&input.writing_goal);

    // Build chapter suggestion
    let suggestion = chapter_workflow::build_chapter_suggestion(
        intent,
        &input.chapter,
        &format!("针对章节「{}」的写作建议", input.chapter.heading_text),
        0.8,
    );

    // Retrieve local evidence
    let evidence = retrieve_chapter_evidence(&state, &input).await?;

    // Build evidence packet IDs
    let evidence_ids: Vec<String> = evidence.iter().map(|p| p.id.clone()).collect();

    // Build chapter patch
    let patch = chapter_workflow::build_chapter_patch(
        &input.target_path,
        &input.base_content_hash,
        &input.chapter,
        &format!("[AI 章节改写内容: {}]", input.writing_goal),
        evidence_ids,
    );

    // Complete trace
    let _ = TraceRecorder::complete(
        &state.db,
        &request_id,
        TraceStatus::Completed,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );

    // Emit event
    let _ = app_handle.emit("ai:chapter_writing_complete", &request_id);

    Ok(ChapterWritingResult {
        request_id,
        suggestions: vec![suggestion],
        patches: vec![patch],
        evidence_used: evidence,
        total_tokens: TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        },
    })
}

/// Execute a document-level check.
#[tauri::command]
pub async fn document_check_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: DocumentCheckInput,
) -> AppResult<DocumentCheckResult> {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Start trace
    TraceRecorder::start(
        &state.db,
        &request_id,
        crate::ai_runtime::AiScene::KnowledgeLookup,
    )?;

    // Retrieve local evidence
    let evidence = retrieve_document_evidence(&state, &input).await?;

    // Execute document check
    let result = document_workflow::execute_document_check(&input, evidence)?;

    // Complete trace
    let _ = TraceRecorder::complete(
        &state.db,
        &request_id,
        TraceStatus::Completed,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );

    // Emit event
    let _ = app_handle.emit("ai:document_check_complete", &request_id);

    Ok(result)
}

/// Retrieve evidence for chapter writing.
async fn retrieve_chapter_evidence(
    state: &AppState,
    input: &ChapterWritingInput,
) -> AppResult<Vec<crate::ai_runtime::ContextPacket>> {
    let query = format!("{} {}", input.chapter.heading_text, input.writing_goal);

    let request = RetrievalRequest {
        query: query.trim().to_string(),
        max_results: 10,
        layers: RetrievalLayers::default(),
        note_context: Some(input.target_path.clone()),
        file_id_context: None,
        scope: RetrievalScope::default(),
    };

    let packets = state
        .db
        .with_conn(|conn| crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request))?;

    Ok(packets)
}

/// Retrieve evidence for document check.
async fn retrieve_document_evidence(
    state: &AppState,
    input: &DocumentCheckInput,
) -> AppResult<Vec<crate::ai_runtime::ContextPacket>> {
    let request = RetrievalRequest {
        query: input.content[..input.content.len().min(500)].to_string(),
        max_results: 15,
        layers: RetrievalLayers::default(),
        note_context: Some(input.target_path.clone()),
        file_id_context: None,
        scope: RetrievalScope::default(),
    };

    let packets = state
        .db
        .with_conn(|conn| crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request))?;

    Ok(packets)
}

/// Parse chapters from content (exposed for frontend).
#[tauri::command]
pub fn parse_document_chapters(content: String) -> AppResult<Vec<ChapterInfo>> {
    Ok(chapter_workflow::parse_chapters(&content))
}
