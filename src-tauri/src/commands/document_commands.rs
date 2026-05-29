//! Chapter and document level writing IPC commands.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::ai_runtime::chapter_workflow::{
    self, ChapterInfo, ChapterWritingInput, ChapterWritingResult,
};
use crate::ai_runtime::document_workflow::{self, DocumentCheckInput, DocumentCheckResult};
use crate::ai_runtime::evidence_mixer;
use crate::ai_runtime::model_gateway::ProviderConfig;
use crate::ai_runtime::retrieval_broker::{RetrievalLayers, RetrievalRequest};
use crate::ai_runtime::retrieval_scope::RetrievalScope;
use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
use crate::ai_runtime::writing_workflow;
use crate::ai_runtime::{AiScene, TokenUsage, WritingIntent};
use crate::app::AppState;
use crate::error::AppResult;
use crate::llm::search_web;

/// Execute a chapter-level writing task (shared by IPC and assistant facade).
pub(crate) async fn execute_chapter_writing(
    state: &AppState,
    app_handle: &AppHandle,
    input: ChapterWritingInput,
) -> AppResult<ChapterWritingResult> {
    let request_id = uuid::Uuid::new_v4().to_string();

    TraceRecorder::start(&state.db, &request_id, AiScene::DraftingAssist)?;

    let intent = chapter_workflow::detect_chapter_intent(&input.writing_goal);

    let mut evidence = retrieve_chapter_evidence(state, &input).await?;

    if input.web_authorized {
        let query = format!("{} {}", input.chapter.heading_text, input.writing_goal);
        if let Ok(fetch) = search_web::fetch_search_context_for_db(&state.db, query.trim()).await {
            let web = evidence_mixer::web_packets_from_fetch(&fetch, &input.writing_goal, None);
            evidence = evidence_mixer::mix_and_rank(evidence, web, 20);
        }
    }

    let resolved = crate::llm::config::resolve_for_scene(&state.db, AiScene::DraftingAssist)?;
    let provider_config = resolved.to_provider_config(AiScene::DraftingAssist);

    let full_content = read_note_content(state, &input.target_path)?;

    let (replacement, usage, suggestion_note) = resolve_chapter_replacement(
        state,
        app_handle,
        &provider_config,
        &intent,
        &input,
        &full_content,
        &evidence,
    )
    .await;

    let suggestion = chapter_workflow::build_chapter_suggestion(
        intent,
        &input.chapter,
        &suggestion_note,
        if suggestion_note.contains("回退") {
            0.55
        } else {
            0.85
        },
    );

    let evidence_ids: Vec<String> = evidence.iter().map(|p| p.id.clone()).collect();
    let patch = chapter_workflow::build_chapter_patch(
        &input.target_path,
        &input.base_content_hash,
        &input.chapter,
        &replacement,
        evidence_ids,
    );

    let packet_ids: Vec<String> = evidence.iter().map(|p| p.id.clone()).collect();
    let _ = TraceRecorder::complete(
        &state.db,
        &request_id,
        TraceStatus::Completed,
        None,
        None,
        None,
        Some(&packet_ids),
        None,
        None,
        None,
        None,
    );

    let _ = app_handle.emit("ai:chapter_writing_complete", &request_id);

    Ok(ChapterWritingResult {
        request_id,
        suggestions: vec![suggestion],
        patches: vec![patch],
        evidence_used: evidence,
        total_tokens: usage,
    })
}

/// Execute a chapter-level writing task.
#[tauri::command]
pub async fn chapter_writing_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: ChapterWritingInput,
) -> AppResult<ChapterWritingResult> {
    execute_chapter_writing(state.inner().as_ref(), &app_handle, input).await
}

/// Execute a document-level check (shared by IPC and assistant facade).
pub(crate) async fn execute_document_check(
    state: &AppState,
    app_handle: &AppHandle,
    input: DocumentCheckInput,
) -> AppResult<DocumentCheckResult> {
    let request_id = uuid::Uuid::new_v4().to_string();

    TraceRecorder::start(&state.db, &request_id, AiScene::KnowledgeLookup)?;

    let mut evidence = retrieve_document_evidence(state, &input).await?;

    if input.web_authorized {
        let query = input.content[..input.content.len().min(200)].to_string();
        if let Ok(fetch) = search_web::fetch_search_context_for_db(&state.db, &query).await {
            let web = evidence_mixer::web_packets_from_fetch(&fetch, &input.target_path, None);
            evidence = evidence_mixer::mix_and_rank(evidence, web, 20);
        }
    }

    let resolved = crate::llm::config::resolve_for_scene(&state.db, AiScene::DraftingAssist)?;
    let provider_config = resolved.to_provider_config(AiScene::DraftingAssist);

    let mut result = document_workflow::execute_document_check(&input, evidence.clone())?;
    let heuristic = result.clone();

    match document_workflow::enhance_document_check_with_llm(
        &state.db,
        app_handle,
        &provider_config,
        &input,
        result,
        &evidence,
    )
    .await
    {
        Ok(enhanced) => result = enhanced,
        Err(e) => {
            tracing::warn!("Document check LLM failed: {e}");
            result = heuristic;
            result.analysis_summary = Some(format!("启发式检查已完成；LLM 综合分析暂不可用：{e}"));
        }
    }

    result.request_id = request_id.clone();

    let packet_ids: Vec<String> = result.evidence_used.iter().map(|p| p.id.clone()).collect();
    let _ = TraceRecorder::complete(
        &state.db,
        &request_id,
        TraceStatus::Completed,
        None,
        None,
        None,
        Some(&packet_ids),
        None,
        None,
        None,
        None,
    );

    let _ = app_handle.emit("ai:document_check_complete", &request_id);

    Ok(result)
}

/// Execute a document-level check.
#[tauri::command]
pub async fn document_check_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: DocumentCheckInput,
) -> AppResult<DocumentCheckResult> {
    execute_document_check(state.inner().as_ref(), &app_handle, input).await
}

fn writing_intent_for_chapter(intent: &WritingIntent) -> WritingIntent {
    match *intent {
        WritingIntent::ChapterContinue => WritingIntent::Continue,
        WritingIntent::ChapterRestructure => WritingIntent::Outline,
        _ => WritingIntent::Rewrite,
    }
}

async fn resolve_chapter_replacement(
    state: &AppState,
    app_handle: &AppHandle,
    provider_config: &ProviderConfig,
    intent: &WritingIntent,
    input: &ChapterWritingInput,
    full_content: &str,
    evidence: &[crate::ai_runtime::ContextPacket],
) -> (String, TokenUsage, String) {
    match chapter_workflow::generate_chapter_content_with_llm(
        &state.db,
        app_handle,
        provider_config,
        intent,
        &input.chapter,
        full_content,
        &input.writing_goal,
        evidence,
    )
    .await
    {
        Ok((text, usage)) => {
            return (text, usage, "已根据目标生成章节改写建议".to_string());
        }
        Err(e) => tracing::warn!("Chapter LLM failed: {e}"),
    }

    let selection_intent = writing_intent_for_chapter(intent);
    match writing_workflow::generate_replacement_with_llm(
        &state.db,
        app_handle,
        provider_config,
        &selection_intent,
        &input.chapter.content,
        full_content,
        &input.writing_goal,
        evidence,
    )
    .await
    {
        Ok((text, usage)) => {
            return (
                text,
                TokenUsage {
                    prompt_tokens: usage.prompt_tokens,
                    completion_tokens: usage.completion_tokens,
                    total_tokens: usage.total_tokens,
                    ..Default::default()
                },
                "章节模型不可用，已用选区级写作回退生成建议".to_string(),
            );
        }
        Err(e) => tracing::warn!("Chapter selection-level LLM failed: {e}"),
    }

    (
        chapter_workflow::chapter_heuristic_fallback(intent, &input.chapter, &input.writing_goal),
        TokenUsage::default(),
        "模型暂不可用，已应用结构化启发式回退（请人工复核）".to_string(),
    )
}

fn read_note_content(state: &AppState, path: &str) -> AppResult<String> {
    let vault = state.vault_path()?;
    let abs = crate::storage::paths::resolve_vault_path(&vault, path)?;
    Ok(std::fs::read_to_string(abs)?)
}

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

    state
        .db
        .with_conn(|conn| crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request))
}

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

    state
        .db
        .with_conn(|conn| crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request))
}

/// Parse chapters from content (exposed for frontend).
#[tauri::command]
pub fn parse_document_chapters(content: String) -> AppResult<Vec<ChapterInfo>> {
    Ok(chapter_workflow::parse_chapters(&content))
}
