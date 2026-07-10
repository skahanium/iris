use crate::ai_runtime::retrieval_broker::{RetrievalLayers, RetrievalRequest};
use crate::ai_runtime::retrieval_scope::RetrievalScope;
use crate::app::AppState;
use crate::error::{AppError, AppResult};

use super::ToolDispatchContext;

pub(super) async fn hybrid_search(
    state: &AppState,
    tool_name: &str,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    let query = args["query"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing query"))?;
    let limit = (args["limit"].as_u64().unwrap_or(10) as usize).clamp(1, 8);
    let layers = match tool_name {
        "search_keyword" => RetrievalLayers {
            fts: true,
            vector: false,
            graph: false,
            exact: false,
            template: false,
        },
        "search_semantic" => RetrievalLayers {
            fts: false,
            vector: true,
            graph: false,
            exact: false,
            template: false,
        },
        _ => RetrievalLayers {
            fts: true,
            vector: true,
            graph: ctx.note_path.is_some(),
            exact: false,
            template: false,
        },
    };
    let packets = state.db.with_read_conn(|conn| {
        let request = RetrievalRequest {
            query: query.to_string(),
            max_results: limit,
            layers,
            note_context: ctx.note_path.map(|s| s.to_string()),
            file_id_context: ctx.file_id,
            scope: ctx.retrieval_scope.clone(),
            runtime_documents: ctx.runtime_documents.to_vec(),
            corpus_config: None,
        };
        crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)
    })?;
    Ok(serde_json::json!({ "results": packets, "count": packets.len() }))
}

pub(super) async fn regulation_lookup(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let regulation_name = args["regulation_name"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing regulation_name"))?;
    let article = args["article"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing article"))?;
    let query = format!("《{regulation_name}》{article}");
    let packets = state.db.with_read_conn(|conn| {
        let request = RetrievalRequest {
            query,
            max_results: 3,
            layers: RetrievalLayers {
                fts: false,
                vector: false,
                graph: false,
                exact: true,
                template: false,
            },
            note_context: None,
            file_id_context: None,
            scope: RetrievalScope::default(),
            runtime_documents: Vec::new(),
            corpus_config: None,
        };
        crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)
    })?;
    Ok(serde_json::json!({
        "regulation": packets.first(),
        "found": !packets.is_empty(),
    }))
}
