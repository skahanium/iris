use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::ai_runtime::retrieval_scope::filter_packets_by_scope;
use crate::ai_runtime::ContextPacket;
use crate::error::{AppError, AppResult};

use super::{
    fuse_and_rank, search_exact_regulation, search_fts, search_graph_neighbors, search_template,
    search_vector_anchors, search_vector_chunks, search_vector_regulations, RetrievalRequest,
};

/// Per-layer retrieval status reported by the diagnostic API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalLayerStatus {
    Ok,
    Empty,
    IndexNotReady,
    Unavailable,
    SchemaMismatch,
    QueryError,
}

/// Non-sensitive diagnostic for one retrieval layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalLayerDiagnostic {
    pub layer: String,
    pub status: RetrievalLayerStatus,
    pub message: Option<String>,
}

/// Retrieval result plus per-layer diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalOutcome {
    pub packets: Vec<ContextPacket>,
    pub diagnostics: Vec<RetrievalLayerDiagnostic>,
}

/// 执行混合检索，并返回每个检索层的非敏感诊断信息。
pub fn hybrid_retrieve_with_diagnostics(
    conn: &Connection,
    request: &RetrievalRequest,
) -> AppResult<RetrievalOutcome> {
    let mut packets: Vec<ContextPacket> = Vec::new();
    let mut diagnostics: Vec<RetrievalLayerDiagnostic> = Vec::new();

    if request.layers.fts {
        append_layer_result(
            "fts",
            search_fts(conn, &request.query, request.max_results),
            &mut packets,
            &mut diagnostics,
        );
    }

    if request.layers.vector {
        if crate::storage::db::vector_index_ready() {
            append_layer_result(
                "vector_chunks",
                search_vector_chunks(conn, &request.query, request.max_results),
                &mut packets,
                &mut diagnostics,
            );
            append_layer_result(
                "vector_anchors",
                search_vector_anchors(conn, &request.query, request.max_results),
                &mut packets,
                &mut diagnostics,
            );
            append_layer_result(
                "vector_regulations",
                search_vector_regulations(conn, &request.query, request.max_results),
                &mut packets,
                &mut diagnostics,
            );
        } else {
            diagnostics.push(RetrievalLayerDiagnostic {
                layer: "vector".to_string(),
                status: RetrievalLayerStatus::IndexNotReady,
                message: Some("sqlite-vec index is not ready".to_string()),
            });
        }
    }

    if request.layers.graph {
        if let Some(file_id) = request.file_id_context {
            append_layer_result(
                "graph",
                search_graph_neighbors(conn, file_id, request.max_results / 2),
                &mut packets,
                &mut diagnostics,
            );
        }
    }

    if request.layers.exact {
        append_layer_result(
            "exact",
            search_exact_regulation(conn, &request.query),
            &mut packets,
            &mut diagnostics,
        );
    }

    if request.layers.template {
        append_layer_result(
            "template",
            search_template(conn, &request.query, request.max_results),
            &mut packets,
            &mut diagnostics,
        );
    }

    fuse_and_rank(&mut packets, request.max_results);
    filter_packets_by_scope(&mut packets, &request.scope, |p| p.source_path.as_deref());

    Ok(RetrievalOutcome {
        packets,
        diagnostics,
    })
}

fn append_layer_result(
    layer: &str,
    result: AppResult<Vec<ContextPacket>>,
    packets: &mut Vec<ContextPacket>,
    diagnostics: &mut Vec<RetrievalLayerDiagnostic>,
) {
    match result {
        Ok(mut layer_packets) => {
            let status = if layer_packets.is_empty() {
                RetrievalLayerStatus::Empty
            } else {
                RetrievalLayerStatus::Ok
            };
            diagnostics.push(RetrievalLayerDiagnostic {
                layer: layer.to_string(),
                status,
                message: None,
            });
            packets.append(&mut layer_packets);
        }
        Err(err) => {
            diagnostics.push(RetrievalLayerDiagnostic {
                layer: layer.to_string(),
                status: classify_retrieval_error(&err),
                message: Some(sanitize_retrieval_error(&err.to_string())),
            });
        }
    }
}

fn classify_retrieval_error(err: &AppError) -> RetrievalLayerStatus {
    let message = match err {
        AppError::Db(db_err) => db_err.to_string().to_lowercase(),
        _ => err.to_string().to_lowercase(),
    };
    if message.contains("no such column") {
        RetrievalLayerStatus::SchemaMismatch
    } else if message.contains("no such table") || message.contains("no such module") {
        RetrievalLayerStatus::Unavailable
    } else if message.contains("index")
        || message.contains("embedding")
        || message.contains("model")
        || message.contains("vec")
    {
        RetrievalLayerStatus::IndexNotReady
    } else {
        RetrievalLayerStatus::QueryError
    }
}

fn sanitize_retrieval_error(message: &str) -> String {
    message.chars().take(240).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_schema_mismatch_separately_from_missing_tables() {
        let schema = AppError::msg("no such column: c.text");
        let unavailable = AppError::msg("no such table: vec_chunks");
        let query = AppError::msg("malformed MATCH expression");

        assert_eq!(
            classify_retrieval_error(&schema),
            RetrievalLayerStatus::SchemaMismatch
        );
        assert_eq!(
            classify_retrieval_error(&unavailable),
            RetrievalLayerStatus::Unavailable
        );
        assert_eq!(
            classify_retrieval_error(&query),
            RetrievalLayerStatus::QueryError
        );
    }

    #[test]
    fn empty_layer_result_is_not_reported_as_ok() {
        let mut packets = Vec::new();
        let mut diagnostics = Vec::new();

        append_layer_result("fts", Ok(Vec::new()), &mut packets, &mut diagnostics);

        assert!(packets.is_empty());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].status, RetrievalLayerStatus::Empty);
    }
}
