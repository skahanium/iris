use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use fastembed::{
    InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};
use rusqlite::{Connection, OptionalExtension};

use crate::ai_types::EmbedBackend;
use crate::error::{AppError, AppResult};

/// Maximum chunks for Rust cosine fallback (avoids loading entire vault into memory).
const MAX_COSINE_FALLBACK_CHUNKS: i64 = 8_000;

/// Pinned v2 embedding model and its fixed output dimension.
pub const EMBEDDING_MODEL_ID: &str = "Xenova/bge-small-zh-v1.5";
pub const EMBEDDING_DIMENSION: usize = 512;
const QUERY_INSTRUCTION: &str = "\u{4e3a}\u{8fd9}\u{4e2a}\u{53e5}\u{5b50}\u{751f}\u{6210}\u{8868}\u{793a}\u{4ee5}\u{7528}\u{4e8e}\u{68c0}\u{7d22}\u{76f8}\u{5173}\u{6587}\u{7ae0}\u{ff1a}";
const BUNDLED_MODEL_SUBDIRECTORY: &str = "models/bge-small-zh-v1.5";
const READY_MARKER: &str = ".iris-model-ready.json";
const REQUIRED_MODEL_FILES: [&str; 5] = [
    "onnx/model.onnx",
    "tokenizer.json",
    "config.json",
    "special_tokens_map.json",
    "tokenizer_config.json",
];

/// Global embedding model, lazy-initialized via OnceLock.
///
/// fastembed v5 mutates internal state during `embed()`, so calls share one
/// lazily loaded model behind a Mutex instead of loading one model per request.
static EMBEDDER: OnceLock<Result<Mutex<TextEmbedding>, String>> = OnceLock::new();

/// Return exclusive access to the bundled BGE v2 model.
fn get_embedder() -> AppResult<MutexGuard<'static, TextEmbedding>> {
    let model = EMBEDDER
        .get_or_init(|| {
            create_bundled_embedder()
                .map(Mutex::new)
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| {
            AppError::Embed(format!(
                "Failed to load bundled {EMBEDDING_MODEL_ID}: {error}"
            ))
        })?;
    model
        .lock()
        .map_err(|_| AppError::Embed("Embedding model lock poisoned".into()))
}

fn create_bundled_embedder() -> AppResult<TextEmbedding> {
    let directory = bundled_model_directory()?;
    let tokenizer_files = TokenizerFiles {
        tokenizer_file: read_bundled_model_file(&directory, "tokenizer.json")?,
        config_file: read_bundled_model_file(&directory, "config.json")?,
        special_tokens_map_file: read_bundled_model_file(&directory, "special_tokens_map.json")?,
        tokenizer_config_file: read_bundled_model_file(&directory, "tokenizer_config.json")?,
    };
    let model = UserDefinedEmbeddingModel::new(
        read_bundled_model_file(&directory, "onnx/model.onnx")?,
        tokenizer_files,
    )
    .with_pooling(Pooling::Cls);
    TextEmbedding::try_new_from_user_defined(model, InitOptionsUserDefined::new())
        .map_err(|error| AppError::Embed(error.to_string()))
}

fn bundled_model_directory() -> AppResult<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("IRIS_EMBEDDING_MODEL_DIR") {
        candidates.push(PathBuf::from(path));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".iris-dev")
            .join("models")
            .join("bge-small-zh-v1.5"),
    );
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.join(BUNDLED_MODEL_SUBDIRECTORY));
            candidates.push(
                parent
                    .join("..")
                    .join("Resources")
                    .join(BUNDLED_MODEL_SUBDIRECTORY),
            );
        }
    }

    let mut failures = Vec::new();
    for candidate in candidates {
        match validate_bundled_model_directory(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) => failures.push(error.to_string()),
        }
    }
    Err(AppError::Embed(format!(
        "Bundled {EMBEDDING_MODEL_ID} is unavailable; run npm run model:prepare before development or install a package containing the verified model ({})",
        failures.join("; ")
    )))
}

fn validate_bundled_model_directory(directory: &Path) -> AppResult<()> {
    if !directory.is_dir() {
        return Err(AppError::Embed(format!(
            "model directory does not exist: {}",
            directory.display()
        )));
    }
    let marker = directory.join(READY_MARKER);
    if !marker.is_file() {
        return Err(AppError::Embed(format!(
            "model directory is missing {READY_MARKER}: {}",
            directory.display()
        )));
    }
    for relative_path in REQUIRED_MODEL_FILES {
        let path = directory.join(relative_path);
        if !path.is_file() {
            return Err(AppError::Embed(format!(
                "model directory is missing required artifact {relative_path}: {}",
                directory.display()
            )));
        }
    }
    Ok(())
}

fn read_bundled_model_file(directory: &Path, relative_path: &str) -> AppResult<Vec<u8>> {
    fs::read(directory.join(relative_path)).map_err(|error| {
        AppError::Embed(format!(
            "Failed to read bundled model artifact {relative_path}: {error}"
        ))
    })
}

fn validate_embedding_dimension(embedding: Vec<f32>) -> AppResult<Vec<f32>> {
    if embedding.len() != EMBEDDING_DIMENSION {
        return Err(AppError::Embed(format!(
            "Bundled {EMBEDDING_MODEL_ID} returned {} dimensions, expected {EMBEDDING_DIMENSION}",
            embedding.len()
        )));
    }
    Ok(embedding)
}

/// Generate an embedding for indexed document text.
pub fn embed_text(text: &str) -> AppResult<Vec<f32>> {
    let mut model = get_embedder()?;
    let embedding = model
        .embed(vec![text], None)
        .map_err(|error| AppError::Embed(error.to_string()))?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::msg("Empty embedding result"))?;
    validate_embedding_dimension(embedding)
}

/// Generate a retrieval-query embedding with BGE's Chinese retrieval instruction.
pub fn embed_query(query: &str) -> AppResult<Vec<f32>> {
    embed_text(&format!("{QUERY_INSTRUCTION}{query}"))
}

/// Batch-embed multiple texts in a single model call for better throughput.
pub fn embed_texts_batch(texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let mut model = get_embedder()?;
    let embeddings = model
        .embed(texts, None)
        .map_err(|error| AppError::Embed(error.to_string()))?;
    embeddings
        .into_iter()
        .map(validate_embedding_dimension)
        .collect()
}
pub struct FastEmbedBackend;

impl EmbedBackend for FastEmbedBackend {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        embed_text(text).map_err(|e| e.to_string())
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        embed_texts_batch(texts).map_err(|e| e.to_string())
    }
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

/// Return whether the BGE v2 generation is complete, validated, and active.
///
/// The state row is only a progress checkpoint. The table may legitimately be
/// unavailable in an unmigrated in-memory database or while a vault is first
/// initialized, so that condition is a safe `false` rather than an error that
/// would take down keyword and graph retrieval.
pub fn embedding_generation_ready(conn: &Connection) -> AppResult<bool> {
    let state = match conn
        .query_row(
            "SELECT phase, active_model_id, target_model_id, target_dimension,
                    indexed_items, total_items
             FROM embedding_generation_state WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
    {
        Ok(state) => state,
        Err(error) if is_unavailable_embedding_schema(&error) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let Some((phase, active_model_id, target_model_id, target_dimension, indexed, total)) = state
    else {
        return Ok(false);
    };
    if phase != "ready"
        || active_model_id != EMBEDDING_MODEL_ID
        || target_model_id != EMBEDDING_MODEL_ID
        || target_dimension != EMBEDDING_DIMENSION as i64
        || indexed != total
    {
        return Ok(false);
    }

    let chunk_count = match conn.query_row("SELECT COUNT(*) FROM chunks", [], |row| {
        row.get::<_, i64>(0)
    }) {
        Ok(count) => count,
        Err(error) if is_unavailable_embedding_schema(&error) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let embedded_count =
        match conn.query_row("SELECT COUNT(*) FROM chunk_embeddings_v2", [], |row| {
            row.get::<_, i64>(0)
        }) {
            Ok(count) => count,
            Err(error) if is_unavailable_embedding_schema(&error) => return Ok(false),
            Err(error) => return Err(error.into()),
        };
    let invalid_dimension_count = match conn.query_row(
        "SELECT COUNT(*) FROM chunk_embeddings_v2 WHERE length(embedding) <> ?1",
        [((EMBEDDING_DIMENSION * std::mem::size_of::<f32>()) as i64)],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(count) => count,
        Err(error) if is_unavailable_embedding_schema(&error) => return Ok(false),
        Err(error) => return Err(error.into()),
    };

    let (anchor_count, anchor_embedded_count, invalid_anchor_dimensions) =
        auxiliary_embedding_counts(
            conn,
            "semantic_anchors",
            "semantic_anchor_embeddings_v2",
            "anchor_id",
        )?;
    let (regulation_count, regulation_embedded_count, invalid_regulation_dimensions) =
        auxiliary_embedding_counts(
            conn,
            "regulation_index",
            "regulation_embeddings_v2",
            "regulation_id",
        )?;
    let generation_count = chunk_count + anchor_count + regulation_count;
    Ok(total == generation_count
        && embedded_count == chunk_count
        && anchor_embedded_count == anchor_count
        && regulation_embedded_count == regulation_count
        && invalid_dimension_count == 0
        && invalid_anchor_dimensions == 0
        && invalid_regulation_dimensions == 0)
}

fn auxiliary_embedding_counts(
    conn: &Connection,
    source_table: &str,
    embedding_table: &str,
    id_column: &str,
) -> AppResult<(i64, i64, i64)> {
    let source_count: i64 =
        conn.query_row(&format!("SELECT COUNT(*) FROM {source_table}"), [], |row| {
            row.get(0)
        })?;
    let embedding_count: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM {embedding_table}"),
        [],
        |row| row.get(0),
    )?;
    let invalid_dimensions: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM {embedding_table} WHERE length(embedding) <> ?1"),
        [((EMBEDDING_DIMENSION * std::mem::size_of::<f32>()) as i64)],
        |row| row.get(0),
    )?;
    let orphan_count: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM {embedding_table} AS e LEFT JOIN {source_table} AS s ON s.id = e.{id_column} WHERE s.id IS NULL"),
        [],
        |row| row.get(0),
    )?;
    Ok((
        source_count,
        embedding_count + orphan_count,
        invalid_dimensions,
    ))
}

fn is_unavailable_embedding_schema(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(_, Some(detail)) if detail.contains("no such table")
    )
}
/// Semantic search over the active BGE v2 chunk embeddings.
///
/// During migration the legacy 384-dimensional cache is deliberately not mixed
/// with 512-dimensional BGE queries. Callers retain FTS and other non-vector
/// retrieval layers until the explicit rebuild marks the v2 generation ready.
pub fn semantic_search(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<SemanticHit>> {
    if !embedding_generation_ready(conn)? {
        return Ok(Vec::new());
    }
    semantic_search_cosine_v2(conn, query, limit)
}

/// Bounded Rust cosine scan for the v2 generation.
fn semantic_search_cosine_v2(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AppResult<Vec<SemanticHit>> {
    let chunk_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM chunk_embeddings_v2", [], |row| {
            row.get(0)
        })?;
    if chunk_count > MAX_COSINE_FALLBACK_CHUNKS {
        tracing::warn!(
            chunks = chunk_count,
            max = MAX_COSINE_FALLBACK_CHUNKS,
            "cosine fallback skipped: too many chunks for the non-sqlite-vec build"
        );
        return Ok(vec![]);
    }
    if chunk_count == 0 {
        return Ok(vec![]);
    }

    let query_vec = embed_query(query)?;
    let mut stmt = conn.prepare(
        "SELECT c.id, c.content, f.path, f.title, ce.embedding
         FROM chunks c
         JOIN files f ON f.id = c.file_id
         JOIN chunk_embeddings_v2 ce ON ce.chunk_id = c.id
         WHERE f.path <> '.classified'
           AND f.path NOT LIKE '.classified/%'",
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

    let mut hits = Vec::new();
    for row in rows.flatten() {
        let (chunk_id, snippet, path, title, blob) = row;
        let embedding = bytes_to_f32(&blob);
        if embedding.len() != EMBEDDING_DIMENSION {
            tracing::warn!(
                chunk_id,
                dimensions = embedding.len(),
                "skipping invalid v2 embedding dimension"
            );
            continue;
        }
        hits.push(SemanticHit {
            chunk_id,
            path,
            title,
            snippet: truncate_snippet(&snippet, 200),
            score: cosine_similarity(&query_vec, &embedding),
        });
    }
    hits.sort_by(|left, right| right.score.total_cmp(&left.score));
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

/// Read embedding blob (auto-detects format).
/// Magic [0x51,0x55] => quantized; otherwise => raw f32 LE.
pub(crate) fn bytes_to_f32(blob: &[u8]) -> Vec<f32> {
    if blob.is_empty() {
        return vec![];
    }
    // Quantized format: magic [0x51, 0x55] + scale (4 bytes) + i8 data
    if blob.len() >= 6 && blob[0] == 0x51 && blob[1] == 0x55 {
        let scale = f32::from_le_bytes([blob[2], blob[3], blob[4], blob[5]]);
        blob[6..]
            .iter()
            .map(|&b| (b as i8) as f32 / scale)
            .collect()
    } else {
        blob.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }
}

/// Serialize an embedding as contiguous little-endian `f32` values.
///
/// sqlite-vec `float[N]` columns require this exact representation. The reader
/// still accepts the legacy scalar-quantized format so existing databases stay
/// searchable by the cosine fallback until their v2 generation is rebuilt.
pub fn f32_to_bytes(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(vec));
    for value in vec {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn truncate_snippet(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::{bytes_to_f32, f32_to_bytes, validate_bundled_model_directory};
    use tempfile::tempdir;

    #[test]
    fn unmigrated_database_is_not_ready_for_v2_semantic_search() {
        let conn = rusqlite::Connection::open_in_memory().expect("open unmigrated database");

        let ready = super::embedding_generation_ready(&conn)
            .expect("unmigrated database should degrade to not-ready");

        assert!(!ready);
    }
    #[test]
    fn storage_format_is_raw_little_endian_f32_for_sqlite_vec() {
        let source = vec![0.25_f32, -1.5_f32, 3.0_f32];

        let blob = f32_to_bytes(&source);

        assert_eq!(blob.len(), source.len() * std::mem::size_of::<f32>());
        assert_eq!(bytes_to_f32(&blob), source);
    }

    #[test]
    fn legacy_quantized_blobs_remain_readable_during_generation_migration() {
        let scale = 127.0_f32;
        let mut legacy = vec![0x51, 0x55];
        legacy.extend_from_slice(&scale.to_le_bytes());
        legacy.extend_from_slice(&[127_u8, 129_u8, 0_u8]);

        let decoded = bytes_to_f32(&legacy);

        assert_eq!(decoded, vec![1.0_f32, -1.0_f32, 0.0_f32]);
    }
    #[test]
    fn bundled_model_directory_requires_verified_ready_marker() {
        let temp = tempdir().expect("create model fixture directory");

        let error = validate_bundled_model_directory(temp.path())
            .expect_err("unverified model directory must be rejected");

        assert!(matches!(error, crate::error::AppError::Embed(_)));
    }
}
