use rusqlite::Connection;
#[cfg(feature = "sqlite-vec")]
use rusqlite::{params_from_iter, types::Value};

use crate::ai_runtime::retrieval_scope::RetrievalScope;
use crate::ai_runtime::ContextPacket;
#[cfg(feature = "sqlite-vec")]
use crate::ai_runtime::{SourceSpan, SourceType, TrustLevel};
#[cfg(feature = "sqlite-vec")]
use crate::embedding::engine;
use crate::error::{AppError, AppResult};

#[cfg(feature = "sqlite-vec")]
use super::truncate;

#[cfg(all(test, feature = "sqlite-vec"))]
thread_local! {
    static OBSERVED_KNN_LIMITS: std::cell::RefCell<Vec<(&'static str, usize)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(all(test, feature = "sqlite-vec"))]
pub(super) fn clear_observed_knn_limits() {
    OBSERVED_KNN_LIMITS.with(|limits| limits.borrow_mut().clear());
}

#[cfg(all(test, feature = "sqlite-vec"))]
pub(super) fn observed_knn_limits() -> Vec<(&'static str, usize)> {
    OBSERVED_KNN_LIMITS.with(|limits| limits.borrow().clone())
}

#[cfg(all(test, feature = "sqlite-vec"))]
fn observe_knn_limit(layer: &'static str, limit: usize) {
    OBSERVED_KNN_LIMITS.with(|limits| limits.borrow_mut().push((layer, limit)));
}

#[cfg(feature = "sqlite-vec")]
pub(super) fn ensure_sqlite_vec_v3_available(conn: &Connection) -> AppResult<()> {
    conn.query_row("SELECT vec_version()", [], |_| Ok(()))
        .map_err(|_| {
            AppError::msg("sqlite-vec Agent retrieval unavailable: extension is not loaded")
        })?;
    let table_count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM sqlite_master
         WHERE type = 'table'
           AND name IN ('vec_chunks_v3', 'vec_anchors_v3', 'vec_regulations_v3')",
        [],
        |row| row.get(0),
    )?;
    if table_count != 3 {
        return Err(AppError::msg(
            "sqlite-vec Agent retrieval unavailable: v3 index migration is not applied",
        ));
    }
    Ok(())
}

#[cfg(not(feature = "sqlite-vec"))]
pub(super) fn ensure_sqlite_vec_v3_available(_conn: &Connection) -> AppResult<()> {
    Err(AppError::msg(
        "sqlite-vec Agent retrieval unavailable: this build has no sqlite-vec backend",
    ))
}

#[cfg(feature = "sqlite-vec")]
fn scoped_file_subquery(scope: &RetrievalScope, first_parameter: usize) -> (String, Vec<Value>) {
    let mut predicates = vec![
        "f.path <> '.classified'".to_string(),
        "f.path NOT LIKE '.classified/%'".to_string(),
    ];
    let mut parameters = Vec::new();
    let mut next_parameter = first_parameter;

    if !scope.is_path_unrestricted() {
        let mut path_predicates = Vec::new();
        for path in &scope.paths {
            path_predicates.push(format!("f.path = ?{next_parameter}"));
            parameters.push(Value::Text(path.clone()));
            next_parameter += 1;
        }
        for prefix in &scope.path_prefixes {
            path_predicates.push(format!(
                "substr(f.path, 1, length(?{next_parameter})) = ?{next_parameter}"
            ));
            parameters.push(Value::Text(prefix.clone()));
            next_parameter += 1;
        }
        predicates.push(format!("({})", path_predicates.join(" OR ")));
    }

    for tag in &scope.required_tags {
        predicates.push(format!(
            "EXISTS (
                 SELECT 1
                 FROM file_tags AS scoped_file_tags
                 INNER JOIN tags AS scoped_tags ON scoped_tags.id = scoped_file_tags.tag_id
                 WHERE scoped_file_tags.file_id = f.id
                   AND lower(scoped_tags.name) = ?{next_parameter}
             )"
        ));
        parameters.push(Value::Text(tag.trim().to_lowercase()));
        next_parameter += 1;
    }

    (
        format!(
            "SELECT f.id
             FROM files AS f
             WHERE {}",
            predicates.join(" AND ")
        ),
        parameters,
    )
}

#[cfg(feature = "sqlite-vec")]
fn knn_query_parts(
    query_embedding: &[f32],
    candidate_limit: usize,
    result_limit: usize,
    scope: &RetrievalScope,
) -> AppResult<Option<(Vec<Value>, String, usize)>> {
    if query_embedding.len() != engine::EMBEDDING_DIMENSION {
        return Err(AppError::Embed(format!(
            "sqlite-vec query has {} dimensions, expected {}",
            query_embedding.len(),
            engine::EMBEDDING_DIMENSION
        )));
    }
    if candidate_limit == 0 {
        return Ok(None);
    }
    // The candidate budget may be expanded geometrically to recover rows
    // consumed by stale vectors before the fingerprint filter; the final
    // result limit stays fixed so fusion (MMR is O(n^2)) never sees an
    // inflated candidate pool.
    let candidate_limit = i64::try_from(candidate_limit)
        .map_err(|_| AppError::Embed("sqlite-vec result limit exceeds SQLite range".into()))?;
    let result_limit = i64::try_from(result_limit)
        .map_err(|_| AppError::Embed("sqlite-vec result limit exceeds SQLite range".into()))?;
    let (file_subquery, scope_parameters) = scoped_file_subquery(scope, 3);
    let model_parameter = 3 + scope_parameters.len();
    let mut parameters = vec![
        Value::Blob(engine::f32_to_bytes(query_embedding)),
        Value::Integer(candidate_limit),
    ];
    parameters.extend(scope_parameters);
    parameters.push(Value::Text(engine::EMBEDDING_MODEL_ID.to_string()));
    parameters.push(Value::Integer(engine::EMBEDDING_DIMENSION as i64));
    parameters.push(Value::Integer(
        (engine::EMBEDDING_DIMENSION * std::mem::size_of::<f32>()) as i64,
    ));
    parameters.push(Value::Integer(result_limit));
    Ok(Some((parameters, file_subquery, model_parameter)))
}

/// Upper bound for the per-layer KNN candidate budget expansion. The normal
/// floor is `max_results * 4` (min 32); stale rows can consume it, so the
/// budget doubles until the filtered result meets the target or this ceiling.
#[cfg(feature = "sqlite-vec")]
const MAX_KNN_CANDIDATE_LIMIT: usize = 256;

#[cfg(feature = "sqlite-vec")]
pub(super) fn search_vector_chunks(
    conn: &Connection,
    query_embedding: &[f32],
    limit: usize,
    scope: &RetrievalScope,
) -> AppResult<Vec<ContextPacket>> {
    #[cfg(all(test, feature = "sqlite-vec"))]
    observe_knn_limit("vector_chunks", limit);
    // The KNN budget runs before the fingerprint/model filter, so a burst of
    // stale rows can consume it and drain the filtered result. Expand the
    // budget geometrically until the filtered result meets the target or the
    // ceiling is reached; stale rows can then never silently empty the layer.
    let mut candidate_limit = limit;
    loop {
        let packets = search_vector_chunks_with_candidate(
            conn,
            query_embedding,
            candidate_limit,
            limit,
            scope,
        )?;
        if packets.len() >= limit || candidate_limit >= MAX_KNN_CANDIDATE_LIMIT {
            return Ok(packets);
        }
        candidate_limit = candidate_limit.saturating_mul(2);
    }
}

#[cfg(feature = "sqlite-vec")]
fn search_vector_chunks_with_candidate(
    conn: &Connection,
    query_embedding: &[f32],
    candidate_limit: usize,
    result_limit: usize,
    scope: &RetrievalScope,
) -> AppResult<Vec<ContextPacket>> {
    let Some((parameters, file_subquery, model_parameter)) =
        knn_query_parts(query_embedding, candidate_limit, result_limit, scope)?
    else {
        return Ok(Vec::new());
    };
    let dimension_parameter = model_parameter + 1;
    let bytes_parameter = model_parameter + 2;
    let limit_parameter = model_parameter + 3;
    let sql = format!(
        "WITH nearest AS (
             SELECT chunk_id, distance
             FROM vec_chunks_v3
             WHERE embedding MATCH ?1
               AND k = ?2
               AND file_id IN ({file_subquery})
         )
         SELECT c.id, c.content, f.path, f.title, c.heading_path,
                c.source_start, c.source_end, c.content_hash, nearest.distance
         FROM nearest
         INNER JOIN chunks AS c ON c.id = nearest.chunk_id
         INNER JOIN files AS f ON f.id = c.file_id
         INNER JOIN chunk_embeddings_v2 AS cache ON cache.chunk_id = c.id
         WHERE cache.model_id = ?{model_parameter}
           AND cache.dimension = ?{dimension_parameter}
           AND cache.source_fingerprint = COALESCE(c.content_hash, '')
           AND length(cache.embedding) = ?{bytes_parameter}
         ORDER BY nearest.distance ASC
         LIMIT ?{limit_parameter}"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(parameters.iter()), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<i64>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, f64>(8)?,
        ))
    })?;

    let mut packets = Vec::new();
    for row in rows {
        let (chunk_id, content, path, title, heading_path, start, end, hash, distance) = row?;
        let source_span = match (start, end) {
            (Some(start), Some(end)) if start >= 0 && end >= start => Some(SourceSpan {
                start: start as usize,
                end: end as usize,
            }),
            _ => None,
        };
        packets.push(ContextPacket {
            id: format!("chunk-{chunk_id}"),
            source_type: SourceType::Note,
            source_path: Some(path),
            title,
            heading_path,
            source_span,
            content_hash: hash.unwrap_or_default(),
            excerpt: truncate(&content, 300),
            retrieval_reason: "vector_chunk".to_string(),
            score: (1.0_f64 - distance).clamp(0.0, 1.0),
            trust_level: TrustLevel::UserNote,
            citation_label: format!("[C{chunk_id}]"),
            stale: false,
            web: None,
            corpus: None,
        });
    }
    Ok(packets)
}

#[cfg(feature = "sqlite-vec")]
pub(super) fn search_vector_anchors(
    conn: &Connection,
    query_embedding: &[f32],
    limit: usize,
    scope: &RetrievalScope,
) -> AppResult<Vec<ContextPacket>> {
    #[cfg(all(test, feature = "sqlite-vec"))]
    observe_knn_limit("vector_anchors", limit);
    search_structured_vectors(
        conn,
        query_embedding,
        limit,
        scope,
        StructuredVectorKind::Anchor,
    )
}

#[cfg(feature = "sqlite-vec")]
pub(super) fn search_vector_regulations(
    conn: &Connection,
    query_embedding: &[f32],
    limit: usize,
    scope: &RetrievalScope,
) -> AppResult<Vec<ContextPacket>> {
    #[cfg(all(test, feature = "sqlite-vec"))]
    observe_knn_limit("vector_regulations", limit);
    search_structured_vectors(
        conn,
        query_embedding,
        limit,
        scope,
        StructuredVectorKind::Regulation,
    )
}

#[cfg(feature = "sqlite-vec")]
#[derive(Clone, Copy)]
enum StructuredVectorKind {
    Anchor,
    Regulation,
}

#[cfg(feature = "sqlite-vec")]
fn search_structured_vectors(
    conn: &Connection,
    query_embedding: &[f32],
    limit: usize,
    scope: &RetrievalScope,
    kind: StructuredVectorKind,
) -> AppResult<Vec<ContextPacket>> {
    // Same stale-row expansion as `search_vector_chunks`.
    let mut candidate_limit = limit;
    loop {
        let packets = search_structured_vectors_with_candidate(
            conn,
            query_embedding,
            candidate_limit,
            limit,
            scope,
            kind,
        )?;
        if packets.len() >= limit || candidate_limit >= MAX_KNN_CANDIDATE_LIMIT {
            return Ok(packets);
        }
        candidate_limit = candidate_limit.saturating_mul(2);
    }
}

#[cfg(feature = "sqlite-vec")]
fn search_structured_vectors_with_candidate(
    conn: &Connection,
    query_embedding: &[f32],
    candidate_limit: usize,
    result_limit: usize,
    scope: &RetrievalScope,
    kind: StructuredVectorKind,
) -> AppResult<Vec<ContextPacket>> {
    let Some((parameters, file_subquery, model_parameter)) =
        knn_query_parts(query_embedding, candidate_limit, result_limit, scope)?
    else {
        return Ok(Vec::new());
    };
    let dimension_parameter = model_parameter + 1;
    let bytes_parameter = model_parameter + 2;
    let limit_parameter = model_parameter + 3;
    let (vec_table, vec_id, source_table, source_id, cache_table, cache_id, kind_name) = match kind
    {
        StructuredVectorKind::Anchor => (
            "vec_anchors_v3",
            "anchor_id",
            "semantic_anchors",
            "id",
            "semantic_anchor_embeddings_v2",
            "anchor_id",
            "anchor",
        ),
        StructuredVectorKind::Regulation => (
            "vec_regulations_v3",
            "regulation_id",
            "regulation_index",
            "id",
            "regulation_embeddings_v2",
            "regulation_id",
            "regulation",
        ),
    };
    let (heading_expression, confidence_expression) = match kind {
        StructuredVectorKind::Anchor => ("source.heading_path", "source.confidence"),
        StructuredVectorKind::Regulation => ("source.article", "1.0"),
    };
    let sql = format!(
        "WITH nearest AS (
             SELECT {vec_id} AS source_id, distance
             FROM {vec_table}
             WHERE embedding MATCH ?1
               AND k = ?2
               AND file_id IN ({file_subquery})
         )
         SELECT source.id, source.content, f.path, f.title, {heading_expression},
                source.source_start, source.source_end, source.content_hash,
                nearest.distance, {confidence_expression}
         FROM nearest
         INNER JOIN {source_table} AS source ON source.{source_id} = nearest.source_id
         INNER JOIN files AS f ON f.id = source.file_id
         INNER JOIN {cache_table} AS cache ON cache.{cache_id} = source.id
         WHERE cache.model_id = ?{model_parameter}
           AND cache.dimension = ?{dimension_parameter}
           AND cache.source_fingerprint = COALESCE(source.content_hash, '')
           AND length(cache.embedding) = ?{bytes_parameter}
         ORDER BY nearest.distance ASC
         LIMIT ?{limit_parameter}"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(parameters.iter()), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, f64>(8)?,
            row.get::<_, f64>(9)?,
        ))
    })?;
    let mut packets = Vec::new();
    for row in rows {
        let (id, content, path, title, heading, start, end, hash, distance, confidence) = row?;
        if start < 0 || end < start || hash.is_empty() {
            continue;
        }
        packets.push(ContextPacket {
            id: format!("{kind_name}-{id}"),
            source_type: match kind {
                StructuredVectorKind::Anchor => SourceType::Anchor,
                StructuredVectorKind::Regulation => SourceType::Regulation,
            },
            source_path: Some(path),
            title,
            heading_path: heading,
            source_span: Some(SourceSpan {
                start: start as usize,
                end: end as usize,
            }),
            content_hash: hash,
            excerpt: truncate(&content, 400),
            retrieval_reason: format!("vector_{kind_name}"),
            score: ((1.0_f64 - distance).clamp(0.0, 1.0) * confidence.clamp(0.0, 1.0)),
            trust_level: TrustLevel::UserNote,
            citation_label: format!("[V{id}]"),
            stale: false,
            web: None,
            corpus: None,
        });
    }
    Ok(packets)
}

#[cfg(not(feature = "sqlite-vec"))]
pub(super) fn search_vector_chunks(
    conn: &Connection,
    _query_embedding: &[f32],
    _limit: usize,
    _scope: &RetrievalScope,
) -> AppResult<Vec<ContextPacket>> {
    ensure_sqlite_vec_v3_available(conn)?;
    Ok(Vec::new())
}

#[cfg(not(feature = "sqlite-vec"))]
pub(super) fn search_vector_anchors(
    conn: &Connection,
    _query_embedding: &[f32],
    _limit: usize,
    _scope: &RetrievalScope,
) -> AppResult<Vec<ContextPacket>> {
    ensure_sqlite_vec_v3_available(conn)?;
    Ok(Vec::new())
}

#[cfg(not(feature = "sqlite-vec"))]
pub(super) fn search_vector_regulations(
    conn: &Connection,
    _query_embedding: &[f32],
    _limit: usize,
    _scope: &RetrievalScope,
) -> AppResult<Vec<ContextPacket>> {
    ensure_sqlite_vec_v3_available(conn)?;
    Ok(Vec::new())
}
