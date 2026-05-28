//! Citation check IPC commands.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::ai_runtime::citation_workflow;
use crate::ai_runtime::retrieval_broker::{RetrievalLayers, RetrievalRequest};
use crate::ai_runtime::retrieval_scope::RetrievalScope;
use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
use crate::ai_runtime::{CitationCheckInput, CitationCheckResult};
use crate::app::AppState;
use crate::error::AppResult;

/// Execute a citation check task.
///
/// This command:
/// 1. Extracts fact claims from the paragraph text
/// 2. Searches local evidence for support or conflict
/// 3. Optionally searches the web if authorized
/// 4. Outputs citation coverage assessment
/// 5. Gives suggestions for adding citations or rewriting
#[tauri::command]
pub async fn citation_check(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    input: CitationCheckInput,
) -> AppResult<CitationCheckResult> {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Start trace
    TraceRecorder::start(
        &state.db,
        &request_id,
        crate::ai_runtime::AiScene::KnowledgeLookup,
    )?;

    let mut evidence = retrieve_citation_evidence(&state, &input).await?;

    if input.web_authorized {
        if let Ok(fetch) =
            crate::llm::search_web::fetch_search_context_for_db(&state.db, &input.paragraph_text)
                .await
        {
            let web = crate::ai_runtime::evidence_mixer::web_packets_from_fetch(
                &fetch,
                &input.paragraph_text,
                None,
            );
            evidence = crate::ai_runtime::evidence_mixer::mix_and_rank(evidence, web, 20);
        }
    }

    let result = citation_workflow::execute_citation_check(&input, evidence)?;

    // Complete trace
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

    // Emit event to frontend
    let _ = app_handle.emit("ai:citation_check_complete", &request_id);

    Ok(result)
}

/// Retrieve local evidence for citation checking.
async fn retrieve_citation_evidence(
    state: &AppState,
    input: &CitationCheckInput,
) -> AppResult<Vec<crate::ai_runtime::ContextPacket>> {
    // Build a query from the paragraph text
    let query = input.paragraph_text.clone();

    // Use the existing hybrid_retrieve functionality
    let request = RetrievalRequest {
        query: query.trim().to_string(),
        max_results: 15, // Get more evidence for citation checking
        layers: RetrievalLayers {
            fts: true,
            vector: true,
            graph: true,
            exact: true,
            template: false,
        },
        note_context: Some(input.document_path.clone()),
        file_id_context: None,
        scope: input
            .scope
            .as_ref()
            .map(|s| RetrievalScope {
                paths: s.paths.clone(),
                path_prefixes: s.path_prefixes.clone(),
            })
            .unwrap_or_default(),
    };

    let packets = state
        .db
        .with_conn(|conn| crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request))?;

    Ok(packets)
}
