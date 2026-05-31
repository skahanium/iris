use std::sync::Mutex;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::storage::db;

/// Maximum chunks for Rust cosine fallback (avoids loading entire vault into memory).
const MAX_COSINE_FALLBACK_CHUNKS: i64 = 2_000;

static EMBEDDER: Mutex<Option<TextEmbedding>> = Mutex::new(None);

/// Generate embedding vector for text.
pub fn embed_text(text: &str) -> AppResult<Vec<f32>> {
    let mut guard = EMBEDDER
        .lock()
        .map_err(|_| AppError::msg("embedder lock"))?;
    if guard.is_none() {
        *guard = Some(
            TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))
                .map_err(|e| AppError::Embed(e.to_string()))?,
        );
    }
    let model = guard.as_ref().expect("embedder initialized");
    let embeddings = model
        .embed(vec![text], None)
        .map_err(|e| AppError::Embed(e.to_string()))?;
    embeddings
        .into_iter()
        .next()
        .ok_or_else(|| AppError::msg("Empty embedding result"))
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Semantic search over chunk embeddings.
/// v0.2: tries sqlite-vec virtual table first; falls back to Rust cosine scan (v0.1).
pub fn semantic_search(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<SemanticHit>> {
    if db::vector_index_ready() {
        if let Ok(hits) = semantic_search_vec(conn, query, limit) {
            if !hits.is_empty() {
                return Ok(hits);
            }
        }
    }

    semantic_search_cosine(conn, query, limit)
}

/// sqlite-vec path: uses vec0 virtual table for approximate KNN.
fn semantic_search_vec(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<SemanticHit>> {
    let query_vec = embed_text(query)?;
    let blob = f32_to_bytes(&query_vec);

    let mut stmt = conn.prepare(
        "SELECT vc.rowid, c.content, f.path, f.title, vc.distance
         FROM vec_chunks vc
         JOIN chunks c ON c.id = vc.rowid
         JOIN files f ON f.id = c.file_id
         WHERE vc.embedding MATCH ?1
         ORDER BY vc.distance
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(rusqlite::params![blob, limit as i64], |row| {
        Ok(SemanticHit {
            chunk_id: row.get(0)?,
            path: row.get(2)?,
            title: row.get(3)?,
            snippet: truncate_snippet(&row.get::<_, String>(1)?, 200),
            score: 1.0 - row.get::<_, f64>(4)? as f32,
        })
    })?;

    Ok(rows.flatten().collect())
}

/// v0.1 cosine scan: loads all embeddings and computes cosine in Rust.
fn semantic_search_cosine(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<SemanticHit>> {
    let chunk_count: i64 = conn.query_row("SELECT COUNT(*) FROM chunk_embeddings", [], |row| {
        row.get(0)
    })?;
    if chunk_count > MAX_COSINE_FALLBACK_CHUNKS {
        tracing::warn!(
            chunks = chunk_count,
            max = MAX_COSINE_FALLBACK_CHUNKS,
            "cosine fallback skipped: too many chunks (enable sqlite-vec or reindex)"
        );
        return Ok(vec![]);
    }

    let query_vec = embed_text(query)?;

    let mut stmt = conn.prepare(
        "SELECT c.id, c.content, f.path, f.title, ce.embedding
         FROM chunks c
         JOIN files f ON f.id = c.file_id
         JOIN chunk_embeddings ce ON ce.chunk_id = c.id",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Vec<u8>>(4)?,
        ))
    })?;

    let mut hits: Vec<SemanticHit> = Vec::new();
    for row in rows.flatten() {
        let (chunk_id, snippet, path, title, blob) = row;
        let vec = bytes_to_f32(&blob);
        let score = cosine_similarity(&query_vec, &vec);
        hits.push(SemanticHit {
            chunk_id,
            path,
            title,
            snippet: truncate_snippet(&snippet, 200),
            score,
        });
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);
    Ok(hits)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SemanticHit {
    pub chunk_id: i64,
    pub path: String,
    pub title: String,
    pub snippet: String,
    pub score: f32,
}

fn bytes_to_f32(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

pub fn f32_to_bytes(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn truncate_snippet(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}
