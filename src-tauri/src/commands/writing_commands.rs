//! Writing workflow IPC commands.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::ai_runtime::evidence_mixer;
use crate::ai_runtime::retrieval_broker::{RetrievalLayers, RetrievalRequest};
use crate::ai_runtime::retrieval_scope::RetrievalScope;
use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
use crate::ai_runtime::writing_workflow::{self, WritingTaskOutput};
use crate::ai_runtime::{AiScene, PatchApplyResult, PatchProposal, TokenUsage, WritingIntent};
use crate::app::AppState;
use crate::error::AppResult;
use crate::llm::search_web;

/// Writing task input from frontend.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct WritingTaskInputIpc {
    /// Target file relative path
    pub target_path: String,
    /// Base content hash (SHA-256)
    pub base_content_hash: String,
    /// Selected text (optional)
    pub selection: Option<String>,
    /// Cursor context (surrounding text)
    pub cursor_context: String,
    /// Writing goal
    pub writing_goal: String,
    /// Whether web search is authorized
    pub web_authorized: bool,
}

/// Apply a validated patch to a file (read → validate → write).
#[tauri::command]
pub fn patch_apply(
    state: State<'_, Arc<AppState>>,
    patch: PatchProposal,
) -> AppResult<PatchApplyResult> {
    use crate::storage::paths::is_user_note_path;

    if !is_user_note_path(&patch.target_path) {
        return Ok(PatchApplyResult {
            success: false,
            new_content_hash: None,
            error: Some("只能修改用户笔记".into()),
            warnings: vec![],
        });
    }
    let content = file_read_inner(state.inner(), &patch.target_path)?;
    let applied = match writing_workflow::apply_patch(&patch, &content) {
        Ok(c) => c,
        Err(e) => {
            return Ok(PatchApplyResult {
                success: false,
                new_content_hash: None,
                error: Some(e.to_string()),
                warnings: vec![],
            });
        }
    };
    let entry = file_write_inner(state.inner(), &patch.target_path, &applied)?;
    let hash = writing_workflow::compute_content_hash(&applied);
    Ok(PatchApplyResult {
        success: true,
        new_content_hash: Some(hash),
        error: None,
        warnings: vec![format!(
            "已写入「{}」，共 {} 字",
            entry.title, entry.word_count
        )],
    })
}

fn file_read_inner(state: &Arc<AppState>, path: &str) -> AppResult<String> {
    let vault = state.vault_path()?;
    let abs = crate::storage::paths::resolve_vault_path(&vault, path)?;
    Ok(std::fs::read_to_string(abs)?)
}

fn file_write_inner(
    state: &Arc<AppState>,
    path: &str,
    content: &str,
) -> AppResult<crate::indexer::scan::FileEntry> {
    use crate::error::AppError;
    use crate::storage::paths::is_user_note_path;

    if !is_user_note_path(path) {
        return Err(AppError::msg("只能写入用户笔记，不允许修改内部元数据路径"));
    }
    let vault = state.vault_path()?;
    let abs = crate::storage::paths::resolve_vault_path(&vault, path)?;
    let tmp = abs.with_extension("md.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &abs)?;

    let hash = crate::indexer::scan::file_hash(&abs)?;
    state.write_guard.mark(path, &hash);

    state.db.with_conn(|conn| {
        crate::indexer::scan::index_file_with_embed(conn, &vault, &abs, Some(state))
    })
}

/// Execute a writing task (shared by IPC command and assistant facade).
pub(crate) async fn execute_writing_task(
    state: &AppState,
    app_handle: &AppHandle,
    input: WritingTaskInputIpc,
) -> AppResult<WritingTaskOutput> {
    let request_id = uuid::Uuid::new_v4().to_string();

    TraceRecorder::start(&state.db, &request_id, AiScene::DraftingAssist)?;

    let intent =
        writing_workflow::detect_writing_intent(&input.writing_goal, input.selection.as_deref());

    let mut evidence = retrieve_writing_evidence(state, &input).await?;

    if input.web_authorized {
        let query = format!(
            "{} {}",
            input.writing_goal,
            input.selection.as_deref().unwrap_or("")
        );
        if let Ok(fetch) = search_web::fetch_search_context_for_db(&state.db, query.trim()).await {
            let web = evidence_mixer::web_packets_from_fetch(&fetch, &input.writing_goal, None);
            evidence = evidence_mixer::mix_and_rank(evidence, web, 20);
        }
    }

    let resolved = crate::llm::config::resolve_for_scene(&state.db, AiScene::DraftingAssist)?;
    let provider_config = resolved.to_provider_config(AiScene::DraftingAssist);

    let suggestion = match &intent {
        WritingIntent::Continue => {
            writing_workflow::build_writing_suggestion(intent.clone(), "基于选区与证据续写", 0.85)
        }
        WritingIntent::Rewrite => {
            writing_workflow::build_writing_suggestion(intent.clone(), "改写选中文本", 0.85)
        }
        WritingIntent::AddEvidence => {
            writing_workflow::build_writing_suggestion(intent.clone(), "补充引用依据", 0.8)
        }
        WritingIntent::Outline => {
            writing_workflow::build_writing_suggestion(intent.clone(), "生成提纲", 0.75)
        }
        WritingIntent::UnifyTone => {
            writing_workflow::build_writing_suggestion(intent.clone(), "统一语气", 0.8)
        }
        _ => writing_workflow::build_writing_suggestion(intent.clone(), "写作建议", 0.75),
    };

    let evidence_ids: Vec<String> = evidence.iter().map(|p| p.id.clone()).collect();
    let mut total_tokens = TokenUsage::default();

    let patches = if let Some(ref selection) = input.selection {
        if !selection.is_empty() {
            let range = find_selection_range(&input.cursor_context, selection);
            let (replacement, usage) = writing_workflow::generate_replacement_with_llm(
                &state.db,
                app_handle,
                &provider_config,
                &intent,
                selection,
                &input.cursor_context,
                &input.writing_goal,
                &evidence,
            )
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("Writing LLM failed: {e}");
                (
                    fallback_replacement(&intent, selection, &input.writing_goal),
                    TokenUsage::default(),
                )
            });
            total_tokens = usage;
            vec![writing_workflow::build_patch_proposal(
                &input.target_path,
                &input.base_content_hash,
                selection,
                &replacement,
                range,
                evidence_ids.clone(),
            )]
        } else {
            vec![]
        }
    } else {
        vec![]
    };

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

    let _ = app_handle.emit("ai:writing_complete", &request_id);

    Ok(WritingTaskOutput {
        request_id,
        suggestions: vec![suggestion],
        patches,
        evidence_used: evidence,
        total_tokens,
    })
}

/// Execute a writing task.
#[tauri::command]
pub async fn writing_execute(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: WritingTaskInputIpc,
) -> AppResult<WritingTaskOutput> {
    execute_writing_task(state.inner().as_ref(), &app_handle, input).await
}

fn fallback_replacement(intent: &WritingIntent, selection: &str, goal: &str) -> String {
    match intent {
        WritingIntent::Continue => format!("{selection}\n\n"),
        WritingIntent::Rewrite => selection.to_string(),
        _ => format!("{selection}\n\n<!-- {goal} -->"),
    }
}

async fn retrieve_writing_evidence(
    state: &AppState,
    input: &WritingTaskInputIpc,
) -> AppResult<Vec<crate::ai_runtime::ContextPacket>> {
    let query = format!(
        "{} {}",
        input.writing_goal,
        input.selection.as_deref().unwrap_or("")
    );

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

fn find_selection_range(content: &str, selection: &str) -> crate::ai_runtime::SourceSpan {
    if let Some(pos) = content.find(selection) {
        crate::ai_runtime::SourceSpan {
            start: pos,
            end: pos + selection.len(),
        }
    } else {
        crate::ai_runtime::SourceSpan {
            start: content.len(),
            end: content.len(),
        }
    }
}
