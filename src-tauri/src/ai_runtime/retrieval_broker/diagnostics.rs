use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::ai_runtime::retrieval_scope::filter_packets_by_scope;
use crate::ai_runtime::{
    ContextPacket, RuntimeDocumentSnapshot, SourceSpan, SourceType, TrustLevel,
};
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

    append_layer_result(
        "runtime_overlay",
        Ok(search_runtime_documents(
            &request.query,
            request.max_results.min(8),
            &request.runtime_documents,
        )),
        &mut packets,
        &mut diagnostics,
    );

    fuse_and_rank(&mut packets, request.max_results);
    filter_packets_by_scope(&mut packets, &request.scope, |p| p.source_path.as_deref());

    Ok(RetrievalOutcome {
        packets,
        diagnostics,
    })
}

const MAX_RUNTIME_DOCUMENTS: usize = 24;
const MAX_RUNTIME_DOCUMENT_CHARS: usize = 80_000;
const MAX_RUNTIME_EXCERPT_CHARS: usize = 900;

fn search_runtime_documents(
    query: &str,
    max_results: usize,
    documents: &[RuntimeDocumentSnapshot],
) -> Vec<ContextPacket> {
    if max_results == 0 {
        return Vec::new();
    }
    let terms = runtime_query_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let mut packets = Vec::new();
    for document in documents.iter().take(MAX_RUNTIME_DOCUMENTS) {
        let path = document.path.trim();
        let content = truncate_chars(&document.content, MAX_RUNTIME_DOCUMENT_CHARS);
        if path.is_empty() || content.trim().is_empty() {
            continue;
        }
        let haystack = format!("{}\n{}", document.title, content).to_lowercase();
        let score = terms
            .iter()
            .map(|term| haystack.matches(term).count())
            .sum::<usize>();
        if score == 0 {
            continue;
        }
        let excerpt = runtime_excerpt(&content, &terms);
        packets.push(ContextPacket {
            id: format!(
                "runtime-overlay:{}:{}",
                crate::cas::hash::content_hash_str(path),
                crate::cas::hash::content_hash_str(&content)
            ),
            source_type: SourceType::Note,
            source_path: Some(path.to_string()),
            title: if document.title.trim().is_empty() {
                path.to_string()
            } else {
                document.title.trim().to_string()
            },
            heading_path: None,
            source_span: None::<SourceSpan>,
            content_hash: crate::cas::hash::content_hash_str(&content),
            excerpt,
            retrieval_reason: "runtime_overlay".to_string(),
            score: 0.75 + (score as f64).min(8.0) / 20.0,
            trust_level: TrustLevel::UserNote,
            citation_label: String::new(),
            stale: false,
            web: None,
            corpus: None,
        });
    }
    packets.sort_by(|a, b| b.score.total_cmp(&a.score));
    packets.truncate(max_results);
    packets
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn runtime_query_terms(query: &str) -> Vec<String> {
    let normalized = query.trim().to_lowercase();
    let mut terms = Vec::new();
    for term in normalized
        .split(|c: char| !c.is_alphanumeric())
        .map(str::trim)
        .filter(|term| term.chars().count() >= 2)
    {
        if !terms.iter().any(|item| item == term) {
            terms.push(term.to_string());
        }
    }
    if normalized.chars().count() >= 4
        && normalized.chars().count() <= 80
        && !terms.iter().any(|item| item == &normalized)
    {
        terms.push(normalized);
    }
    terms
}

fn runtime_excerpt(content: &str, terms: &[String]) -> String {
    let lower = content.to_lowercase();
    let start_byte = terms
        .iter()
        .filter_map(|term| lower.find(term))
        .min()
        .unwrap_or(0);
    let bounded_start = start_byte.min(content.len());
    let safe_start = (0..=bounded_start)
        .rev()
        .find(|index| content.is_char_boundary(*index))
        .unwrap_or(0);
    let start_char = content[..safe_start].chars().count();
    let half_window = MAX_RUNTIME_EXCERPT_CHARS / 2;
    let from = start_char.saturating_sub(half_window);
    let excerpt: String = content
        .chars()
        .skip(from)
        .take(MAX_RUNTIME_EXCERPT_CHARS)
        .collect();
    let prefix = if from > 0 { "..." } else { "" };
    let suffix = if content.chars().count() > from + MAX_RUNTIME_EXCERPT_CHARS {
        "..."
    } else {
        ""
    };
    format!("{prefix}{}{suffix}", excerpt.trim())
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
    use crate::ai_runtime::retrieval_broker::RetrievalLayers;

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

    #[test]
    fn runtime_documents_are_transient_and_respect_scope() {
        let conn = Connection::open_in_memory().unwrap();
        let documents = vec![RuntimeDocumentSnapshot {
            path: "drafts/live.md".to_string(),
            title: "Live".to_string(),
            content: "needle-from-editor appears only in runtime memory".to_string(),
            is_locked: false,
        }];
        let mut request = RetrievalRequest {
            query: "needle-from-editor".into(),
            max_results: 5,
            layers: RetrievalLayers {
                fts: false,
                vector: false,
                graph: false,
                exact: false,
                template: false,
            },
            note_context: None,
            file_id_context: None,
            scope: crate::ai_runtime::retrieval_scope::RetrievalScope::default(),
            runtime_documents: documents,
        };

        let outcome = hybrid_retrieve_with_diagnostics(&conn, &request).unwrap();
        assert_eq!(outcome.packets.len(), 1);
        assert_eq!(
            outcome.packets[0].retrieval_reason.as_str(),
            "runtime_overlay"
        );

        request.scope.paths = vec!["other.md".to_string()];
        let scoped_out = hybrid_retrieve_with_diagnostics(&conn, &request).unwrap();
        assert!(scoped_out.packets.is_empty());
    }
}
