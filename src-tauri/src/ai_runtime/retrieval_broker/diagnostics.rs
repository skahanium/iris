use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::ai_runtime::retrieval_scope::{
    filter_packets_by_required_tags, filter_packets_by_scope,
};
use crate::ai_runtime::{
    ContextPacket, RuntimeDocumentSnapshot, SourceSpan, SourceType, TrustLevel,
};
use crate::error::{AppError, AppResult};

use super::{
    ensure_sqlite_vec_v3_available, fuse_and_rank, search_exact_regulation, search_fts,
    search_graph_neighbors, search_metadata, search_template, search_vector_anchors,
    search_vector_chunks, search_vector_regulations, RetrievalRequest,
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
    /// Backend used for this layer (e.g. "sqlite-vec").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// Active embedding model identifier when the layer ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Active embedding generation identifier when the layer ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_id: Option<String>,
}

/// Retrieval result plus per-layer diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalOutcome {
    pub packets: Vec<ContextPacket>,
    pub diagnostics: Vec<RetrievalLayerDiagnostic>,
}

/// Execute hybrid retrieval and return non-sensitive per-layer diagnostics.
pub fn hybrid_retrieve_with_diagnostics(
    conn: &Connection,
    request: &RetrievalRequest,
) -> AppResult<RetrievalOutcome> {
    hybrid_retrieve_with_diagnostics_with_embedder(conn, request, |query| {
        crate::embedding::engine::embed_query(query)
    })
}

pub fn hybrid_retrieve_with_diagnostics_with_embedder(
    conn: &Connection,
    request: &RetrievalRequest,
    embedder: impl FnOnce(&str) -> AppResult<Vec<f32>>,
) -> AppResult<RetrievalOutcome> {
    let mut packets: Vec<ContextPacket> = Vec::new();
    let mut diagnostics: Vec<RetrievalLayerDiagnostic> = Vec::new();
    // Each layer retrieves a bounded candidate pool before final rank fusion.
    // Vector hard scope is also pushed into each vec0 KNN query below.
    let candidate_limit = request.max_results.saturating_mul(4).max(8);
    let vector_candidate_limit = request.max_results.saturating_mul(4).max(32);

    if request.layers.fts {
        append_layer_result(
            "fts",
            search_fts(conn, &request.query, candidate_limit),
            &mut packets,
            &mut diagnostics,
        );
        append_layer_result(
            "metadata",
            search_metadata(conn, &request.query, candidate_limit),
            &mut packets,
            &mut diagnostics,
        );
    }

    if request.layers.vector {
        if let Err(error) = ensure_sqlite_vec_v3_available(conn) {
            append_layer_result_with_meta(
                "vector",
                Err(error),
                &mut packets,
                &mut diagnostics,
                Some("sqlite-vec".into()),
                Some(crate::embedding::engine::EMBEDDING_MODEL_ID.into()),
            );
        } else if crate::embedding::engine::embedding_generation_ready(conn)? {
            let model_id = crate::embedding::engine::EMBEDDING_MODEL_ID.to_string();
            match embedder(&request.query) {
                Ok(query_embedding) => {
                    append_layer_result_with_meta(
                        "vector_chunks",
                        search_vector_chunks(
                            conn,
                            &query_embedding,
                            vector_candidate_limit,
                            &request.scope,
                        ),
                        &mut packets,
                        &mut diagnostics,
                        Some("sqlite-vec".into()),
                        Some(model_id.clone()),
                    );
                    append_layer_result_with_meta(
                        "vector_anchors",
                        search_vector_anchors(
                            conn,
                            &query_embedding,
                            vector_candidate_limit,
                            &request.scope,
                        ),
                        &mut packets,
                        &mut diagnostics,
                        Some("sqlite-vec".into()),
                        Some(model_id.clone()),
                    );
                    append_layer_result_with_meta(
                        "vector_regulations",
                        search_vector_regulations(
                            conn,
                            &query_embedding,
                            vector_candidate_limit,
                            &request.scope,
                        ),
                        &mut packets,
                        &mut diagnostics,
                        Some("sqlite-vec".into()),
                        Some(model_id),
                    );
                }
                Err(error) => append_layer_result_with_meta(
                    "vector",
                    Err(error),
                    &mut packets,
                    &mut diagnostics,
                    Some("sqlite-vec".into()),
                    Some(model_id),
                ),
            }
        } else {
            diagnostics.push(RetrievalLayerDiagnostic {
                layer: "vector".to_string(),
                status: RetrievalLayerStatus::IndexNotReady,
                message: Some(embedding_not_ready_message(conn)?),
                backend: Some("sqlite-vec".into()),
                model_id: Some(crate::embedding::engine::EMBEDDING_MODEL_ID.into()),
                generation_id: None,
            });
        }
    }

    if request.layers.graph {
        if let Some(file_id) = request.file_id_context {
            // A max_results=1 request must still get at least one neighbor;
            // a zero LIMIT would silently drop the whole graph layer.
            let graph_limit = request.max_results.max(2) / 2;
            append_layer_result(
                "graph",
                search_graph_neighbors(conn, file_id, graph_limit),
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
            search_template(conn, &request.query, candidate_limit),
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

    annotate_packets_with_corpus(request.corpus_config.as_ref(), &mut packets);
    filter_packets_by_scope(&mut packets, &request.scope, |p| p.source_path.as_deref());
    filter_packets_by_required_tags(conn, &mut packets, &request.scope, |p| {
        p.source_path.as_deref()
    })?;
    fuse_and_rank(&mut packets, request.max_results);

    Ok(RetrievalOutcome {
        packets,
        diagnostics,
    })
}

fn embedding_not_ready_message(conn: &Connection) -> AppResult<String> {
    let phase = crate::embedding::scheduler::embedding_index_status(conn)?.phase;
    let message = match phase.as_str() {
        "legacy_ready" => "BGE v2 embedding generation awaits idle upgrade",
        "running" => "BGE v2 embedding generation is rebuilding",
        "paused" => "BGE v2 embedding generation is paused",
        "failed" => "BGE v2 embedding generation failed; keyword search remains available",
        _ => "BGE v2 embedding generation is not ready",
    };
    Ok(message.to_string())
}

fn annotate_packets_with_corpus(
    corpora: Option<&crate::knowledge::corpora::CorpusConfig>,
    packets: &mut [ContextPacket],
) {
    let Some(corpora) = corpora else {
        return;
    };
    for packet in packets {
        let Some(path) = packet.source_path.as_deref() else {
            continue;
        };
        if let Some(entry) = crate::knowledge::corpora::corpus_for_path(corpora, path) {
            packet.corpus = Some(crate::knowledge::corpora::packet_meta_for_entry(entry));
        }
    }
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
        let (excerpt, source_span) = runtime_excerpt(&content, &terms);
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
            source_span: Some(source_span),
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

fn runtime_excerpt(content: &str, terms: &[String]) -> (String, SourceSpan) {
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
    let from = start_char.saturating_sub(MAX_RUNTIME_EXCERPT_CHARS / 2);
    let excerpt: String = content
        .chars()
        .skip(from)
        .take(MAX_RUNTIME_EXCERPT_CHARS)
        .collect();
    let end = from + excerpt.chars().count();
    (excerpt, SourceSpan { start: from, end })
}
fn append_layer_result(
    layer: &str,
    result: AppResult<Vec<ContextPacket>>,
    packets: &mut Vec<ContextPacket>,
    diagnostics: &mut Vec<RetrievalLayerDiagnostic>,
) {
    append_layer_result_with_meta(layer, result, packets, diagnostics, None, None);
}

fn append_layer_result_with_meta(
    layer: &str,
    result: AppResult<Vec<ContextPacket>>,
    packets: &mut Vec<ContextPacket>,
    diagnostics: &mut Vec<RetrievalLayerDiagnostic>,
    backend: Option<String>,
    model_id: Option<String>,
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
                backend,
                model_id,
                generation_id: None,
            });
            packets.append(&mut layer_packets);
        }
        Err(err) => {
            diagnostics.push(RetrievalLayerDiagnostic {
                layer: layer.to_string(),
                status: classify_retrieval_error(&err),
                message: Some(sanitize_retrieval_error(&err.to_string())),
                backend,
                model_id,
                generation_id: None,
            });
        }
    }
}

fn classify_retrieval_error(err: &AppError) -> RetrievalLayerStatus {
    let message = match err {
        AppError::Db(db_err) => db_err.to_string().to_lowercase(),
        _ => err.to_string().to_lowercase(),
    };
    if message.contains("unavailable")
        || message.contains("no such table")
        || message.contains("no such module")
    {
        RetrievalLayerStatus::Unavailable
    } else if message.contains("no such column") {
        RetrievalLayerStatus::SchemaMismatch
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
    use super::super::RetrievalLayers;
    #[cfg(feature = "sqlite-vec")]
    use super::super::{clear_observed_knn_limits, observed_knn_limits};
    use super::*;

    #[cfg(feature = "sqlite-vec")]
    fn vector_fixture(second: f32) -> Vec<f32> {
        let mut vector = vec![0.0_f32; crate::embedding::engine::EMBEDDING_DIMENSION];
        vector[0] = 1.0;
        vector[1] = second;
        vector
    }

    #[cfg(feature = "sqlite-vec")]
    fn insert_file(conn: &Connection, id: i64, path: &str) {
        conn.execute(
            "INSERT INTO files
             (id, path, title, content_hash, word_count, created_at, updated_at)
             VALUES (?1, ?2, ?2, ?3, 1, datetime('now'), datetime('now'))",
            rusqlite::params![id, path, format!("file-{id}")],
        )
        .expect("insert vector fixture file");
    }

    #[cfg(feature = "sqlite-vec")]
    fn insert_chunk_vector(conn: &Connection, id: i64, file_id: i64, vector: &[f32]) {
        let fingerprint = format!("chunk-{id}");
        conn.execute(
            "INSERT INTO chunks
             (id, file_id, chunk_index, content, source_start, source_end, content_hash, char_count)
             VALUES (?1, ?2, 0, ?3, 0, 10, ?4, 10)",
            rusqlite::params![id, file_id, format!("chunk evidence {id}"), fingerprint],
        )
        .expect("insert chunk fixture");
        conn.execute(
            "INSERT INTO chunk_embeddings_v2
             (chunk_id, embedding, source_fingerprint, model_id, dimension)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                id,
                crate::embedding::engine::f32_to_bytes(vector),
                fingerprint,
                crate::embedding::engine::EMBEDDING_MODEL_ID,
                crate::embedding::engine::EMBEDDING_DIMENSION as i64,
            ],
        )
        .expect("insert chunk vector fixture");
    }

    #[cfg(feature = "sqlite-vec")]
    fn insert_anchor_vector(conn: &Connection, id: i64, file_id: i64, vector: &[f32]) {
        let fingerprint = format!("anchor-{id}");
        conn.execute(
            "INSERT INTO semantic_anchors
             (id, anchor_key, file_id, anchor_type, content, heading_path, source_start,
              source_end, content_hash, extractor_version, embedding_model, embedding_dim,
              confidence, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'semantic', ?4, 'Anchor', 20, 30, ?5, 'test', ?6, 512,
                     1.0, datetime('now'), datetime('now'))",
            rusqlite::params![
                id,
                format!("anchor-key-{id}"),
                file_id,
                format!("anchor evidence {id}"),
                fingerprint,
                crate::embedding::engine::EMBEDDING_MODEL_ID,
            ],
        )
        .expect("insert anchor fixture");
        conn.execute(
            "INSERT INTO semantic_anchor_embeddings_v2
             (anchor_id, embedding, source_fingerprint, model_id, dimension)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                id,
                crate::embedding::engine::f32_to_bytes(vector),
                fingerprint,
                crate::embedding::engine::EMBEDDING_MODEL_ID,
                crate::embedding::engine::EMBEDDING_DIMENSION as i64,
            ],
        )
        .expect("insert anchor vector fixture");
    }

    #[cfg(feature = "sqlite-vec")]
    fn insert_regulation_vector(conn: &Connection, id: i64, file_id: i64, vector: &[f32]) {
        let fingerprint = format!("regulation-{id}");
        conn.execute(
            "INSERT INTO regulation_index
             (id, file_id, regulation_name, article, content, source_start, source_end,
              content_hash, parser_version, embedding_model, embedding_dim, created_at)
             VALUES (?1, ?2, '条例', ?3, ?4, 40, 50, ?5, 'test', ?6, 512, datetime('now'))",
            rusqlite::params![
                id,
                file_id,
                format!("第{id}条"),
                format!("regulation evidence {id}"),
                fingerprint,
                crate::embedding::engine::EMBEDDING_MODEL_ID,
            ],
        )
        .expect("insert regulation fixture");
        conn.execute(
            "INSERT INTO regulation_embeddings_v2
             (regulation_id, embedding, source_fingerprint, model_id, dimension)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                id,
                crate::embedding::engine::f32_to_bytes(vector),
                fingerprint,
                crate::embedding::engine::EMBEDDING_MODEL_ID,
                crate::embedding::engine::EMBEDDING_DIMENSION as i64,
            ],
        )
        .expect("insert regulation vector fixture");
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn agent_vector_knn_applies_exact_prefix_and_required_tag_before_each_top_k() {
        let db = crate::storage::db::Database::open_in_memory().expect("open sqlite-vec database");
        db.with_conn(|conn| {
            let closer = vector_fixture(0.0);
            for id in 1..=33_i64 {
                insert_file(conn, id, &format!("outside/{id}.md"));
                insert_chunk_vector(conn, id, id, &closer);
                insert_anchor_vector(conn, id, id, &closer);
                insert_regulation_vector(conn, id, id, &closer);
            }
            insert_file(conn, 99, ".classified/secret.md");
            insert_chunk_vector(conn, 99, 99, &closer);
            insert_anchor_vector(conn, 99, 99, &closer);
            insert_regulation_vector(conn, 99, 99, &closer);

            let allowed = vector_fixture(0.05);
            insert_file(conn, 100, "exact/needle.md");
            insert_chunk_vector(conn, 100, 100, &allowed);
            insert_file(conn, 101, "prefix/needle.md");
            insert_anchor_vector(conn, 101, 101, &allowed);
            insert_regulation_vector(conn, 101, 101, &allowed);
            conn.execute("INSERT INTO tags (id, name) VALUES (1, 'required')", [])?;
            conn.execute(
                "INSERT INTO file_tags (file_id, tag_id) VALUES (99, 1), (100, 1), (101, 1)",
                [],
            )?;
            conn.execute(
                "UPDATE embedding_generation_state
                 SET active_model_id = ?1, target_model_id = ?1, target_dimension = ?2,
                     phase = 'ready', indexed_items = 105, total_items = 105
                 WHERE singleton = 1",
                rusqlite::params![
                    crate::embedding::engine::EMBEDDING_MODEL_ID,
                    crate::embedding::engine::EMBEDDING_DIMENSION as i64,
                ],
            )?;

            for max_results in [1, 3, 5] {
                let request = RetrievalRequest {
                    query: "needle".into(),
                    max_results,
                    layers: RetrievalLayers {
                        fts: false,
                        vector: true,
                        graph: false,
                        exact: false,
                        template: false,
                    },
                    note_context: None,
                    file_id_context: None,
                    scope: crate::ai_runtime::retrieval_scope::RetrievalScope {
                        paths: vec!["exact/needle.md".into(), ".classified/secret.md".into()],
                        path_prefixes: vec!["prefix/".into()],
                        required_tags: vec!["required".into()],
                    },
                    runtime_documents: Vec::new(),
                    corpus_config: None,
                };

                clear_observed_knn_limits();
                let outcome =
                    hybrid_retrieve_with_diagnostics_with_embedder(conn, &request, |_| {
                        Ok(closer.clone())
                    })?;

                assert_eq!(
                    observed_knn_limits(),
                    vec![
                        ("vector_chunks", 32),
                        ("vector_anchors", 32),
                        ("vector_regulations", 32),
                    ],
                    "all Agent vector layers must receive the independent KNN floor"
                );
                assert!(outcome.packets.iter().all(|packet| {
                    matches!(
                        packet.source_path.as_deref(),
                        Some("exact/needle.md" | "prefix/needle.md")
                    )
                }));

                if max_results == 3 {
                    assert_eq!(outcome.packets.len(), 3);
                    for reason in ["vector_chunk", "vector_anchor", "vector_regulation"] {
                        assert!(
                            outcome
                                .packets
                                .iter()
                                .any(|packet| packet.retrieval_reason == reason),
                            "missing broker packet from {reason}"
                        );
                    }
                    let vector_diagnostics: Vec<_> = outcome
                        .diagnostics
                        .iter()
                        .filter(|diagnostic| diagnostic.layer.starts_with("vector_"))
                        .collect();
                    assert_eq!(vector_diagnostics.len(), 3);
                    assert!(vector_diagnostics.iter().all(|diagnostic| {
                        diagnostic.backend.as_deref() == Some("sqlite-vec")
                            && diagnostic.status == RetrievalLayerStatus::Ok
                    }));
                }
            }
            Ok(())
        })
        .expect("run scoped Agent retrieval broker");
    }

    #[cfg(not(feature = "sqlite-vec"))]
    #[test]
    fn agent_retrieval_reports_vector_unavailable_and_preserves_fts_without_feature() {
        let database =
            crate::storage::db::Database::open_in_memory().expect("open non-vec database");
        database
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO files
                     (id, path, title, content_hash, word_count, created_at, updated_at)
                     VALUES (1, 'notes/needle.md', 'Needle', 'file-hash', 1,
                             datetime('now'), datetime('now'))",
                    [],
                )?;
                conn.execute(
                    "INSERT INTO chunks
                     (id, file_id, chunk_index, content, source_start, source_end,
                      content_hash, char_count)
                     VALUES (1, 1, 0, 'needle keyword evidence', 0, 23, 'chunk-hash', 23)",
                    [],
                )?;
                conn.execute(
                    "INSERT INTO files_fts (path, title, content)
                     VALUES ('notes/needle.md', 'Needle', 'needle keyword evidence')",
                    [],
                )?;

                let outcome = hybrid_retrieve_with_diagnostics(
                    conn,
                    &RetrievalRequest {
                        query: "needle".into(),
                        max_results: 3,
                        layers: RetrievalLayers {
                            fts: true,
                            vector: true,
                            graph: false,
                            exact: false,
                            template: false,
                        },
                        note_context: None,
                        file_id_context: None,
                        scope: crate::ai_runtime::retrieval_scope::RetrievalScope::default(),
                        runtime_documents: Vec::new(),
                        corpus_config: None,
                    },
                )?;

                assert!(outcome
                    .packets
                    .iter()
                    .any(|packet| packet.retrieval_reason == "fts_keyword_match"));
                assert!(
                    outcome.diagnostics.iter().any(|diagnostic| {
                        diagnostic.layer == "vector"
                            && diagnostic.status == RetrievalLayerStatus::Unavailable
                            && diagnostic.backend.as_deref() == Some("sqlite-vec")
                    }),
                    "unexpected diagnostics: {:#?}",
                    outcome.diagnostics
                );
                Ok(())
            })
            .expect("run degraded Agent retrieval broker");
    }

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
    fn corpus_role_is_attached_before_rank_v2() {
        let conn = Connection::open_in_memory().expect("open database");
        let corpora = crate::knowledge::corpora::CorpusConfig {
            corpus: vec![
                crate::knowledge::corpora::CorpusEntry {
                    id: "authority".into(),
                    name: "Authority".into(),
                    path_prefix: "authority/".into(),
                    kind: "authority".into(),
                    intents: Vec::new(),
                },
                crate::knowledge::corpora::CorpusEntry {
                    id: "lookup".into(),
                    name: "Lookup".into(),
                    path_prefix: "lookup/".into(),
                    kind: "lookup".into(),
                    intents: Vec::new(),
                },
            ],
        };
        let request = RetrievalRequest {
            query: "evidence".into(),
            max_results: 2,
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
            runtime_documents: vec![
                RuntimeDocumentSnapshot {
                    path: "lookup/a.md".into(),
                    title: "A".into(),
                    content: "evidence".into(),
                    is_locked: false,
                },
                RuntimeDocumentSnapshot {
                    path: "authority/z.md".into(),
                    title: "Z".into(),
                    content: "evidence".into(),
                    is_locked: false,
                },
            ],
            corpus_config: Some(corpora),
        };

        let outcome = hybrid_retrieve_with_diagnostics(&conn, &request).expect("retrieve");

        assert_eq!(
            outcome.packets[0].source_path.as_deref(),
            Some("authority/z.md")
        );
        assert_eq!(
            outcome.packets[0]
                .corpus
                .as_ref()
                .map(|meta| meta.kind.as_str()),
            Some("authority")
        );
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
            corpus_config: None,
        };

        let outcome = hybrid_retrieve_with_diagnostics(&conn, &request).unwrap();
        assert_eq!(outcome.packets.len(), 1);
        assert_eq!(
            outcome.packets[0].retrieval_reason.as_str(),
            "runtime_overlay"
        );
        assert!(outcome.packets[0].source_span.is_some());
        assert!(!outcome.packets[0].content_hash.is_empty());

        request.scope.paths = vec!["other.md".to_string()];
        let scoped_out = hybrid_retrieve_with_diagnostics(&conn, &request).unwrap();
        assert!(scoped_out.packets.is_empty());
    }

    #[test]
    fn graph_layer_keeps_at_least_one_neighbor_for_minimal_max_results() {
        let db = crate::storage::db::Database::open_in_memory().expect("open database");
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO files (id, path, title, content_hash, word_count, created_at, updated_at)
                 VALUES (1, 'source.md', 'Source', 'hash-1', 1, datetime('now'), datetime('now')),
                        (2, 'target.md', 'Target', 'hash-2', 1, datetime('now'), datetime('now'))",
                [],
            )
            .expect("insert graph files");
            conn.execute(
                "INSERT INTO chunks (id, file_id, chunk_index, content, source_start, source_end,
                                     content_hash, char_count)
                 VALUES (1, 2, 0, 'neighbor evidence', 0, 10, 'chunk-hash', 10)",
                [],
            )
            .expect("insert neighbor chunk");
            conn.execute(
                "INSERT INTO links (source_id, target_id, context) VALUES (1, 2, '[[Target]]')",
                [],
            )
            .expect("insert wikilink");
            Ok(())
        })
        .expect("seed graph");

        db.with_read_conn(|conn| {
            for max_results in [1, 2, 4] {
                let request = RetrievalRequest {
                    query: "query".into(),
                    max_results,
                    layers: RetrievalLayers {
                        fts: false,
                        vector: false,
                        graph: true,
                        exact: false,
                        template: false,
                    },
                    note_context: None,
                    file_id_context: Some(1),
                    scope: crate::ai_runtime::retrieval_scope::RetrievalScope::default(),
                    runtime_documents: Vec::new(),
                    corpus_config: None,
                };
                let outcome = hybrid_retrieve_with_diagnostics(conn, &request).unwrap();
                let graph_packets: Vec<_> = outcome
                    .packets
                    .iter()
                    .filter(|packet| packet.retrieval_reason.starts_with("graph_"))
                    .collect();
                assert!(
                    !graph_packets.is_empty(),
                    "graph layer must not be silently dropped for max_results={max_results}"
                );
            }
            Ok(())
        })
        .expect("run graph retrieval");
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn vector_layer_survives_stale_vectors_consuming_knn_budget() {
        let closer = vector_fixture(0.0);
        let farther = vector_fixture(0.6);
        let db = crate::storage::db::Database::open_in_memory().expect("open sqlite-vec database");
        db.with_conn(|conn| {
            // Stale rows: vectors live in vec0, but the canonical cache
            // fingerprint no longer matches the current chunk content (e.g. an
            // interrupted re-embed after an edit). They consume KNN budget
            // before the fingerprint filter runs, so they must not drain the
            // whole vector result.
            for id in 1..=40_i64 {
                insert_file(conn, id, &format!("stale/{id}.md"));
                insert_chunk_vector(conn, id, id, &closer);
                conn.execute(
                    "UPDATE chunk_embeddings_v2 SET source_fingerprint = 'stale-fingerprint'
                     WHERE chunk_id = ?1",
                    [id],
                )
                .expect("mark vector stale");
            }
            // Fresh rows with matching fingerprints.
            for id in 41..=43_i64 {
                insert_file(conn, id, &format!("fresh/{id}.md"));
                insert_chunk_vector(conn, id, id, &farther);
            }
            conn.execute(
                "UPDATE embedding_generation_state
                 SET active_model_id = ?1, target_model_id = ?1, target_dimension = ?2,
                     phase = 'paused', indexed_items = 40, total_items = 43
                 WHERE singleton = 1",
                rusqlite::params![
                    crate::embedding::engine::EMBEDDING_MODEL_ID,
                    crate::embedding::engine::EMBEDDING_DIMENSION as i64,
                ],
            )
            .expect("mark generation edit-paused");
            Ok(())
        })
        .expect("seed stale and fresh vectors");

        db.with_read_conn(|conn| {
            let request = RetrievalRequest {
                query: "needle".into(),
                max_results: 3,
                layers: RetrievalLayers {
                    fts: false,
                    vector: true,
                    graph: false,
                    exact: false,
                    template: false,
                },
                note_context: None,
                file_id_context: None,
                scope: crate::ai_runtime::retrieval_scope::RetrievalScope::default(),
                runtime_documents: Vec::new(),
                corpus_config: None,
            };
            let outcome = hybrid_retrieve_with_diagnostics_with_embedder(conn, &request, |_| {
                Ok(closer.clone())
            })
            .expect("retrieve with stale vectors");
            assert!(
                outcome.packets.iter().any(|packet| packet
                    .source_path
                    .as_deref()
                    .is_some_and(|path| path.starts_with("fresh/"))),
                "stale rows must not drain the whole vector result"
            );
            assert!(
                outcome.packets.iter().all(|packet| {
                    packet
                        .source_path
                        .as_deref()
                        .is_some_and(|path| !path.starts_with("stale/"))
                }),
                "stale rows must never surface as evidence"
            );
            Ok(())
        })
        .expect("run vector retrieval");
    }
}
