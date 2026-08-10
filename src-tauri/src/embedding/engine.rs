use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use fastembed::{
    InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};
use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::ai_types::EmbedBackend;
use crate::error::{AppError, AppResult};

/// Pinned v2 embedding model and its fixed output dimension.
pub const EMBEDDING_MODEL_ID: &str = "Xenova/bge-small-zh-v1.5";
/// Immutable upstream revision pinned by the bundled model manifest.
pub const EMBEDDING_MODEL_REVISION: &str = "fcecc3c5fef6becfa2b2bdda15c1c938857be534";
/// SHA-256 of the immutable ONNX artifact pinned by the bundled model manifest.
pub const EMBEDDING_MODEL_ONNX_SHA256: &str =
    "69a0b846f4f116b5e6aabf9546ea6754d02264f3211a13a1bd69b31b8040749a";
/// Revision- and artifact-bound identity used by derived embedding caches.
pub const EMBEDDING_MODEL_FINGERPRINT: &str =
    "Xenova/bge-small-zh-v1.5@fcecc3c5fef6becfa2b2bdda15c1c938857be534#sha256:69a0b846f4f116b5e6aabf9546ea6754d02264f3211a13a1bd69b31b8040749a";
pub const EMBEDDING_DIMENSION: usize = 512;
const QUERY_INSTRUCTION: &str = "\u{4e3a}\u{8fd9}\u{4e2a}\u{53e5}\u{5b50}\u{751f}\u{6210}\u{8868}\u{793a}\u{4ee5}\u{7528}\u{4e8e}\u{68c0}\u{7d22}\u{76f8}\u{5173}\u{6587}\u{7ae0}\u{ff1a}";
const QUERY_BATCH_SIZE: usize = 16;
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
static EMBEDDER: OnceLock<Mutex<TextEmbedding>> = OnceLock::new();
static EMBEDDER_INITIALIZATION: Mutex<()> = Mutex::new(());
static EMBEDDING_RUNTIME_ENABLED: AtomicBool = AtomicBool::new(true);

/// Configure whether this process may load and use the embedding runtime.
pub fn set_embedding_runtime_enabled(enabled: bool) {
    EMBEDDING_RUNTIME_ENABLED.store(enabled, Ordering::Release);
}

/// Return whether the current process may use embedding inference.
pub fn embedding_runtime_enabled() -> bool {
    EMBEDDING_RUNTIME_ENABLED.load(Ordering::Acquire)
}

/// Release builds always enable embeddings; debug builds require explicit opt-in.
pub fn embedding_runtime_enabled_from_environment() -> bool {
    !cfg!(debug_assertions)
        || std::env::var("IRIS_ENABLE_EMBEDDINGS")
            .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

/// Return exclusive access to the bundled BGE v2 model.
fn get_embedder() -> AppResult<MutexGuard<'static, TextEmbedding>> {
    if !embedding_runtime_enabled() {
        return Err(AppError::Embed("Embedding runtime disabled".into()));
    }
    let model = initialize_once(&EMBEDDER, &EMBEDDER_INITIALIZATION, || {
        create_bundled_embedder().map(Mutex::new)
    })?;
    model
        .lock()
        .map_err(|_| AppError::Embed("Embedding model lock poisoned".into()))
}

/// Initialize a shared value once, retaining only a successful initialization.
fn initialize_once<'a, T>(
    cell: &'a OnceLock<T>,
    initialization: &Mutex<()>,
    initialize: impl FnOnce() -> AppResult<T>,
) -> AppResult<&'a T> {
    if let Some(value) = cell.get() {
        return Ok(value);
    }
    let _guard = initialization
        .lock()
        .map_err(|_| AppError::Embed("Embedding model initialization lock poisoned".into()))?;
    if let Some(value) = cell.get() {
        return Ok(value);
    }
    let value = initialize()?;
    cell.set(value)
        .map_err(|_| AppError::Embed("Embedding model was initialized concurrently".into()))?;
    Ok(cell
        .get()
        .expect("embedding model must be available after successful initialization"))
}

/// Verify that the bundled embedding model can be loaded without embedding text.
pub fn ensure_embedding_model_available() -> AppResult<()> {
    drop(get_embedder()?);
    Ok(())
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
    validate_ready_marker_identity(&marker)?;
    for relative_path in REQUIRED_MODEL_FILES {
        let path = directory.join(relative_path);
        if !path.is_file() {
            return Err(AppError::Embed(format!(
                "model directory is missing required artifact {relative_path}: {}",
                directory.display()
            )));
        }
    }
    let onnx_path = directory.join("onnx/model.onnx");
    let actual_digest = sha256_file(&onnx_path)?;
    if actual_digest != EMBEDDING_MODEL_ONNX_SHA256 {
        return Err(AppError::Embed(format!(
            "model artifact onnx/model.onnx failed pinned SHA-256 verification: {}",
            directory.display()
        )));
    }
    Ok(())
}

fn validate_ready_marker_identity(marker_path: &Path) -> AppResult<()> {
    let marker_bytes = fs::read(marker_path).map_err(|error| {
        AppError::Embed(format!(
            "Failed to read bundled model readiness marker: {error}"
        ))
    })?;
    let marker: serde_json::Value = serde_json::from_slice(&marker_bytes).map_err(|error| {
        AppError::Embed(format!(
            "Bundled model readiness marker is invalid JSON: {error}"
        ))
    })?;
    if marker
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
        || marker.get("repository").and_then(serde_json::Value::as_str) != Some(EMBEDDING_MODEL_ID)
        || marker.get("revision").and_then(serde_json::Value::as_str)
            != Some(EMBEDDING_MODEL_REVISION)
    {
        return Err(AppError::Embed(
            "Bundled model readiness marker revision does not match the pinned runtime identity"
                .into(),
        ));
    }
    let marker_onnx_sha = marker
        .get("files")
        .and_then(serde_json::Value::as_array)
        .and_then(|files| {
            files.iter().find_map(|file| {
                (file.get("path").and_then(serde_json::Value::as_str) == Some("onnx/model.onnx"))
                    .then(|| file.get("sha256").and_then(serde_json::Value::as_str))
                    .flatten()
            })
        });
    if marker_onnx_sha != Some(EMBEDDING_MODEL_ONNX_SHA256) {
        return Err(AppError::Embed(
            "Bundled model readiness marker does not pin the expected ONNX SHA-256".into(),
        ));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> AppResult<String> {
    let mut file = fs::File::open(path).map_err(|error| {
        AppError::Embed(format!(
            "Failed to open bundled model artifact for SHA-256 verification: {error}"
        ))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            AppError::Embed(format!(
                "Failed to hash bundled model artifact for SHA-256 verification: {error}"
            ))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
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

/// Batch-embed retrieval queries while preserving BGE's query instruction.
///
/// The model is locked once and queries are submitted in bounded batches so
/// release evaluation avoids one ORT invocation per labelled query without
/// ever using the document-embedding path for query vectors.
pub fn embed_queries_batch(queries: &[&str]) -> AppResult<Vec<Vec<f32>>> {
    if queries.is_empty() {
        return Ok(Vec::new());
    }
    let mut model = get_embedder()?;
    let mut output = Vec::with_capacity(queries.len());
    for batch in queries.chunks(QUERY_BATCH_SIZE) {
        let instructed = instructed_queries(batch);
        let texts = instructed.iter().map(String::as_str).collect::<Vec<_>>();
        let embeddings = model
            .embed(&texts, None)
            .map_err(|error| AppError::Embed(error.to_string()))?;
        output.extend(
            embeddings
                .into_iter()
                .map(validate_embedding_dimension)
                .collect::<AppResult<Vec<_>>>()?,
        );
    }
    Ok(output)
}

fn instructed_queries(queries: &[&str]) -> Vec<String> {
    queries
        .iter()
        .map(|query| format!("{QUERY_INSTRUCTION}{query}"))
        .collect()
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
    if !embedding_runtime_enabled() {
        return Ok(false);
    }
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
    // Model/dimension mismatches always gate: during migration the legacy
    // 384-dimensional cache is deliberately never mixed with BGE queries.
    if active_model_id != EMBEDDING_MODEL_ID
        || target_model_id != EMBEDDING_MODEL_ID
        || target_dimension != EMBEDDING_DIMENSION as i64
    {
        return Ok(false);
    }
    match phase.as_str() {
        "ready" => {
            if indexed != total {
                return Ok(false);
            }
            match super::scheduler::generation_coverage_complete(conn) {
                Ok(complete) => Ok(complete),
                Err(AppError::Db(_)) => Ok(false),
                Err(error) => Err(error),
            }
        }
        // `paused` is produced by `notify_index_committed` after an edit leaves
        // new chunks unembedded, and by interrupted first-time generation.
        // An active BGE model proves the generation reached `ready` before the
        // pause, so every remaining vector row is valid: edited files lost
        // their stale vectors through the FK cascade and vec0 triggers, and
        // missing chunks are filtered naturally by the query layer. Keeping the
        // global gate open avoids taking the whole vector index offline for
        // every single edit.
        "paused" => Ok(true),
        // First-time generation (legacy_ready/rebuilding/running) and failed
        // generations stay gated so partial indexes are never queried.
        _ => Ok(false),
    }
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

    #[cfg(feature = "sqlite-vec")]
    {
        ensure_sqlite_vec_v3_available(conn)?;
        let indexed: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM vec_chunks_v3 LIMIT 1)",
            [],
            |row| row.get(0),
        )?;
        if !indexed || limit == 0 {
            return Ok(Vec::new());
        }
        let query_embedding = embed_query(query)?;
        semantic_search_sqlite_vec_v3_with_embedding(conn, &query_embedding, limit)
    }

    #[cfg(not(feature = "sqlite-vec"))]
    {
        let _ = (conn, query, limit);
        Err(AppError::Embed(
            "sqlite-vec semantic search unavailable: this build has no sqlite-vec backend".into(),
        ))
    }
}

#[cfg(feature = "sqlite-vec")]
fn ensure_sqlite_vec_v3_available(conn: &Connection) -> AppResult<()> {
    conn.query_row("SELECT vec_version()", [], |_| Ok(()))
        .map_err(|error| {
            AppError::Embed(format!(
                "sqlite-vec semantic search unavailable: extension is not loaded ({error})"
            ))
        })?;
    let migrated: bool = conn.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'vec_chunks_v3'
         )",
        [],
        |row| row.get(0),
    )?;
    if !migrated {
        return Err(AppError::Embed(
            "sqlite-vec semantic search unavailable: v3 index migration is not applied".into(),
        ));
    }
    Ok(())
}

#[cfg(feature = "sqlite-vec")]
fn semantic_search_sqlite_vec_v3_with_embedding(
    conn: &Connection,
    query_embedding: &[f32],
    limit: usize,
) -> AppResult<Vec<SemanticHit>> {
    if query_embedding.len() != EMBEDDING_DIMENSION {
        return Err(AppError::Embed(format!(
            "sqlite-vec query has {} dimensions, expected {EMBEDDING_DIMENSION}",
            query_embedding.len()
        )));
    }
    if limit == 0 {
        return Ok(Vec::new());
    }
    let candidate_count = limit.saturating_mul(4).max(32);
    let candidate_count = i64::try_from(candidate_count)
        .map_err(|_| AppError::Embed("sqlite-vec result limit exceeds SQLite range".into()))?;
    let result_limit = i64::try_from(limit)
        .map_err(|_| AppError::Embed("sqlite-vec result limit exceeds SQLite range".into()))?;
    let embedding_bytes = f32_to_bytes(query_embedding);
    let mut stmt = conn.prepare(
        "WITH nearest AS (
             SELECT chunk_id, distance
             FROM vec_chunks_v3
             WHERE embedding MATCH ?1
               AND k = ?2
               AND file_id IN (
                   SELECT id FROM files
                   WHERE path <> '.classified'
                     AND path NOT LIKE '.classified/%'
               )
         )
         SELECT chunks.id, chunks.content, files.path, files.title, nearest.distance
         FROM nearest
         INNER JOIN chunks ON chunks.id = nearest.chunk_id
         INNER JOIN files ON files.id = chunks.file_id
         INNER JOIN chunk_embeddings_v2 AS cache ON cache.chunk_id = chunks.id
         WHERE cache.model_id = ?3
           AND cache.dimension = ?4
           AND cache.source_fingerprint = COALESCE(chunks.content_hash, '')
           AND length(cache.embedding) = ?5
         ORDER BY nearest.distance ASC
         LIMIT ?6",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![
            embedding_bytes,
            candidate_count,
            EMBEDDING_MODEL_ID,
            EMBEDDING_DIMENSION as i64,
            (EMBEDDING_DIMENSION * std::mem::size_of::<f32>()) as i64,
            result_limit,
        ],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
            ))
        },
    )?;

    let mut hits = Vec::new();
    for row in rows {
        let (chunk_id, snippet, path, title, distance) = row?;
        hits.push(SemanticHit {
            chunk_id,
            path,
            title,
            snippet: truncate_snippet(&snippet, 200),
            score: (1.0_f64 - distance).clamp(0.0, 1.0) as f32,
        });
    }
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

/// Read an embedding blob for storage-format compatibility tests.
/// Magic [0x51,0x55] => quantized; otherwise => raw f32 LE.
#[cfg(test)]
fn bytes_to_f32(blob: &[u8]) -> Vec<f32> {
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
/// still accepts the legacy scalar-quantized format for cache migration and
/// consumers that have not yet switched to the v3 sqlite-vec index.
pub fn f32_to_bytes(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(vec));
    for value in vec {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[cfg(feature = "sqlite-vec")]
fn truncate_snippet(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bytes_to_f32, f32_to_bytes, initialize_once, instructed_queries,
        validate_bundled_model_directory,
    };
    use std::fs;
    use std::sync::{Mutex, OnceLock};
    #[cfg(feature = "sqlite-vec")]
    use std::time::Instant;
    use tempfile::tempdir;

    fn write_complete_model_fixture(directory: &std::path::Path, revision: &str) {
        fs::create_dir_all(directory.join("onnx")).expect("create ONNX fixture directory");
        fs::write(directory.join("onnx/model.onnx"), b"not-the-pinned-model")
            .expect("write replacement ONNX fixture");
        for relative_path in [
            "tokenizer.json",
            "config.json",
            "special_tokens_map.json",
            "tokenizer_config.json",
        ] {
            fs::write(directory.join(relative_path), b"{}").expect("write model fixture artifact");
        }
        fs::write(
            directory.join(".iris-model-ready.json"),
            serde_json::json!({
                "schemaVersion": 1,
                "id": "bge-small-zh-v1.5",
                "repository": super::EMBEDDING_MODEL_ID,
                "revision": revision,
                "license": "MIT",
                "files": [{
                    "path": "onnx/model.onnx",
                    "sha256": "69a0b846f4f116b5e6aabf9546ea6754d02264f3211a13a1bd69b31b8040749a",
                    "bytes": 20
                }]
            })
            .to_string(),
        )
        .expect("write ready marker fixture");
    }

    #[test]
    fn failed_model_initialization_is_not_cached_and_can_retry() {
        let cell = OnceLock::new();
        let initialization = Mutex::new(());

        assert!(initialize_once(&cell, &initialization, || {
            Err(crate::error::AppError::Embed("temporary failure".into()))
        })
        .is_err());
        assert!(cell.get().is_none());

        let model = initialize_once(&cell, &initialization, || Ok(7_u8)).unwrap();
        assert_eq!(*model, 7);
        assert_eq!(cell.get(), Some(&7));
    }

    #[test]
    fn query_batches_preserve_the_bge_retrieval_instruction() {
        let queries = instructed_queries(&["合同解除条件", "项目里程碑"]);

        assert_eq!(queries.len(), 2);
        assert_eq!(
            queries[0],
            format!("{}合同解除条件", super::QUERY_INSTRUCTION)
        );
        assert_eq!(
            queries[1],
            format!("{}项目里程碑", super::QUERY_INSTRUCTION)
        );
    }

    #[test]
    fn unmigrated_database_is_not_ready_for_v2_semantic_search() {
        let conn = rusqlite::Connection::open_in_memory().expect("open unmigrated database");

        let ready = super::embedding_generation_ready(&conn)
            .expect("unmigrated database should degrade to not-ready");

        assert!(!ready);
    }

    /// Seed a minimal `embedding_generation_state` row plus the source tables
    /// used by `generation_coverage_complete`.
    fn seed_generation(
        conn: &rusqlite::Connection,
        phase: &str,
        active_model: &str,
        indexed: i64,
        total: i64,
    ) {
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS embedding_generation_state (
                singleton        INTEGER PRIMARY KEY CHECK (singleton = 1),
                active_model_id  TEXT NOT NULL,
                target_model_id  TEXT NOT NULL,
                target_dimension INTEGER NOT NULL,
                phase            TEXT NOT NULL,
                indexed_items    INTEGER NOT NULL DEFAULT 0,
                total_items      INTEGER NOT NULL DEFAULT 0,
                last_error       TEXT,
                updated_at       TEXT NOT NULL
             );
             DELETE FROM embedding_generation_state;
             INSERT INTO embedding_generation_state
                 (singleton, active_model_id, target_model_id, target_dimension,
                  phase, indexed_items, total_items, updated_at)
             VALUES (1, '{active}', '{target}', {dim}, '{phase}', {indexed}, {total}, datetime('now'));
             CREATE TABLE IF NOT EXISTS chunks (id INTEGER PRIMARY KEY, file_id INTEGER NOT NULL,
                 chunk_index INTEGER NOT NULL, content TEXT NOT NULL, char_count INTEGER,
                 source_start INTEGER, source_end INTEGER, content_hash TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS chunk_embeddings_v2 (
                 chunk_id INTEGER PRIMARY KEY, embedding BLOB NOT NULL,
                 source_fingerprint TEXT NOT NULL, model_id TEXT NOT NULL, dimension INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS semantic_anchors (id INTEGER PRIMARY KEY,
                 file_id INTEGER NOT NULL, content_hash TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS semantic_anchor_embeddings_v2 (
                 anchor_id INTEGER PRIMARY KEY, embedding BLOB NOT NULL,
                 source_fingerprint TEXT NOT NULL, model_id TEXT NOT NULL, dimension INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS regulation_index (id INTEGER PRIMARY KEY,
                 file_id INTEGER NOT NULL, content_hash TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS regulation_embeddings_v2 (
                 regulation_id INTEGER PRIMARY KEY, embedding BLOB NOT NULL,
                 source_fingerprint TEXT NOT NULL, model_id TEXT NOT NULL, dimension INTEGER NOT NULL);",
            active = active_model,
            target = super::EMBEDDING_MODEL_ID,
            dim = super::EMBEDDING_DIMENSION,
        ))
        .expect("seed generation state");
    }

    #[test]
    fn edit_paused_generation_keeps_semantic_search_usable() {
        // `phase = 'paused'` is produced by `notify_index_committed` when an
        // edit leaves new chunks unembedded. Stale vectors were already removed
        // by the FK cascade and vec0 triggers, so the query layer filters the
        // missing chunks naturally and the global gate must not take the whole
        // vector index offline for every edit.
        let conn = rusqlite::Connection::open_in_memory().expect("open database");
        seed_generation(&conn, "paused", super::EMBEDDING_MODEL_ID, 90, 100);

        let ready = super::embedding_generation_ready(&conn).expect("read generation state");
        assert!(
            ready,
            "edit-paused generation must keep semantic search usable"
        );
    }

    #[test]
    fn paused_generation_that_never_reached_ready_stays_gated() {
        // A paused generation whose active model is still the legacy one has
        // never completed a full BGE pass (interrupted first generation); it
        // must stay gated like the running/rebuilding states.
        let conn = rusqlite::Connection::open_in_memory().expect("open database");
        seed_generation(&conn, "paused", "fastembed/AllMiniLML6V2", 30, 100);

        let ready = super::embedding_generation_ready(&conn).expect("read generation state");
        assert!(!ready);
    }

    #[test]
    fn first_time_generation_states_remain_gated() {
        for phase in ["legacy_ready", "rebuilding", "running", "failed"] {
            let conn = rusqlite::Connection::open_in_memory().expect("open database");
            seed_generation(&conn, phase, "fastembed/AllMiniLML6V2", 0, 100);

            let ready = super::embedding_generation_ready(&conn).expect("read generation state");
            assert!(!ready, "phase {phase} must stay gated");
        }
    }

    #[test]
    fn ready_generation_requires_full_coverage_before_enabling() {
        let conn = rusqlite::Connection::open_in_memory().expect("open database");
        // Empty source tables: expected 0 == actual 0, so coverage is complete.
        seed_generation(&conn, "ready", super::EMBEDDING_MODEL_ID, 0, 0);
        let ready = super::embedding_generation_ready(&conn).expect("read generation state");
        assert!(ready);

        // A ready row that still reports missing items must stay gated.
        let conn = rusqlite::Connection::open_in_memory().expect("open database");
        seed_generation(&conn, "ready", super::EMBEDDING_MODEL_ID, 90, 100);
        let ready = super::embedding_generation_ready(&conn).expect("read generation state");
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

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn semantic_search_uses_sqlite_vec_knn_and_never_skips_a_large_index() {
        let db = crate::storage::db::Database::open_in_memory().expect("open sqlite-vec database");
        db.with_conn(|conn| {
            conn.execute_batch(
                "BEGIN;
                 WITH RECURSIVE ids(id) AS (
                     VALUES(1)
                     UNION ALL
                     SELECT id + 1 FROM ids WHERE id < 8001
                 )
                 INSERT INTO files
                     (id, path, title, content_hash, word_count, created_at, updated_at)
                 SELECT id,
                        CASE
                            WHEN id <= 40 THEN printf('.classified/secret-%d.md', id)
                            WHEN id = 8001 THEN 'needle.md'
                            ELSE printf('bulk/%d.md', id)
                        END,
                        CASE WHEN id = 8001 THEN 'Needle' ELSE printf('Bulk %d', id) END,
                        printf('file-hash-%d', id), 1,
                        '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
                 FROM ids;
                 WITH RECURSIVE ids(id) AS (
                     VALUES(1)
                     UNION ALL
                     SELECT id + 1 FROM ids WHERE id < 8001
                 )
                 INSERT INTO chunks
                     (id, file_id, chunk_index, content, content_hash, char_count)
                 SELECT id, id, 0,
                        CASE WHEN id = 8001 THEN 'needle' ELSE 'bulk' END,
                        printf('chunk-hash-%d', id), 6
                 FROM ids;",
            )?;

            let mut far_vector = vec![0.0_f32; super::EMBEDDING_DIMENSION];
            far_vector[1] = 1.0;
            conn.execute(
                "INSERT INTO chunk_embeddings_v2
                     (chunk_id, embedding, source_fingerprint, model_id, dimension)
                 SELECT id, ?1, content_hash, ?2, ?3
                 FROM chunks WHERE id BETWEEN 41 AND 8000",
                rusqlite::params![
                    f32_to_bytes(&far_vector),
                    super::EMBEDDING_MODEL_ID,
                    super::EMBEDDING_DIMENSION as i64,
                ],
            )?;
            let mut nearest_vector = vec![0.0_f32; super::EMBEDDING_DIMENSION];
            nearest_vector[0] = 1.0;
            conn.execute(
                "INSERT INTO chunk_embeddings_v2
                     (chunk_id, embedding, source_fingerprint, model_id, dimension)
                 SELECT id, ?1, content_hash, ?2, ?3
                 FROM chunks WHERE id <= 40",
                rusqlite::params![
                    f32_to_bytes(&nearest_vector),
                    super::EMBEDDING_MODEL_ID,
                    super::EMBEDDING_DIMENSION as i64,
                ],
            )?;
            let mut allowed_vector = nearest_vector.clone();
            allowed_vector[1] = 0.05;
            conn.execute(
                "INSERT INTO chunk_embeddings_v2
                     (chunk_id, embedding, source_fingerprint, model_id, dimension)
                 SELECT id, ?1, content_hash, ?2, ?3
                 FROM chunks WHERE id = 8001",
                rusqlite::params![
                    f32_to_bytes(&allowed_vector),
                    super::EMBEDDING_MODEL_ID,
                    super::EMBEDDING_DIMENSION as i64,
                ],
            )?;
            conn.execute_batch("COMMIT")?;

            let response =
                super::semantic_search_sqlite_vec_v3_with_embedding(conn, &nearest_vector, 5)?;

            assert!(response.iter().any(|hit| hit.path == "needle.md"));
            Ok(())
        })
        .unwrap();
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    #[ignore = "50k scale ladder runs once per main SHA in release-readiness"]
    fn sqlite_vec_knn_scale_ladder() {
        for scale in [1_000_i64, 10_000, 25_000, 50_000] {
            let db = crate::storage::db::Database::open_in_memory()
                .expect("open sqlite-vec scale-ladder database");
            let mut query_vector = vec![0.0_f32; super::EMBEDDING_DIMENSION];
            query_vector[0] = 1.0;
            let mut distractor_vector = vec![0.0_f32; super::EMBEDDING_DIMENSION];
            distractor_vector[1] = 1.0;
            let started = Instant::now();

            let (index_elapsed, knn_elapsed) = db
                .with_conn(|conn| {
                    conn.execute_batch("BEGIN")?;
                    conn.execute(
                        "WITH RECURSIVE ids(id) AS (
                             VALUES(1)
                             UNION ALL
                             SELECT id + 1 FROM ids WHERE id < ?1
                         )
                         INSERT INTO files
                             (id, path, title, content_hash, word_count, created_at, updated_at)
                         SELECT id, printf('scale/%d.md', id), printf('Scale %d', id),
                                printf('file-hash-%d', id), 1,
                                '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
                         FROM ids",
                        [scale + 1],
                    )?;
                    conn.execute(
                        "WITH RECURSIVE ids(id) AS (
                             VALUES(1)
                             UNION ALL
                             SELECT id + 1 FROM ids WHERE id < ?1
                         )
                         INSERT INTO chunks
                             (id, file_id, chunk_index, content, content_hash, char_count)
                         SELECT id, id, 0,
                                CASE WHEN id = ?1 THEN 'needle' ELSE 'bulk' END,
                                printf('chunk-hash-%d', id), 6
                         FROM ids",
                        [scale + 1],
                    )?;
                    conn.execute(
                        "INSERT INTO chunk_embeddings_v2
                             (chunk_id, embedding, source_fingerprint, model_id, dimension)
                         SELECT id, ?1, content_hash, ?2, ?3
                         FROM chunks WHERE id < ?4",
                        rusqlite::params![
                            f32_to_bytes(&distractor_vector),
                            super::EMBEDDING_MODEL_ID,
                            super::EMBEDDING_DIMENSION as i64,
                            scale + 1,
                        ],
                    )?;
                    conn.execute(
                        "INSERT INTO chunk_embeddings_v2
                             (chunk_id, embedding, source_fingerprint, model_id, dimension)
                         SELECT id, ?1, content_hash, ?2, ?3
                         FROM chunks WHERE id = ?4",
                        rusqlite::params![
                            f32_to_bytes(&query_vector),
                            super::EMBEDDING_MODEL_ID,
                            super::EMBEDDING_DIMENSION as i64,
                            scale + 1,
                        ],
                    )?;
                    conn.execute_batch("COMMIT")?;
                    let index_elapsed = started.elapsed();
                    let knn_started = Instant::now();
                    let response = super::semantic_search_sqlite_vec_v3_with_embedding(
                        conn,
                        &query_vector,
                        5,
                    )?;
                    let knn_elapsed = knn_started.elapsed();
                    assert!(
                        response
                            .iter()
                            .any(|hit| hit.path == format!("scale/{}.md", scale + 1)),
                        "KNN missed the needle at scale {scale}"
                    );
                    Ok((index_elapsed, knn_elapsed))
                })
                .unwrap();
            println!(
                "sqlite-vec scale={scale}: index_ms={} knn_ms={}",
                index_elapsed.as_millis(),
                knn_elapsed.as_millis()
            );
        }
    }

    #[cfg(not(feature = "sqlite-vec"))]
    #[test]
    fn semantic_search_without_sqlite_vec_reports_unavailable_explicitly() {
        let conn = rusqlite::Connection::open_in_memory().expect("open non-vec database");
        crate::storage::migrate::migrate_up(&conn).expect("migrate non-vec database");
        conn.execute(
            "UPDATE embedding_generation_state
             SET active_model_id = ?1,
                 target_model_id = ?1,
                 target_dimension = ?2,
                 phase = 'ready',
                 indexed_items = 0,
                 total_items = 0
             WHERE singleton = 1",
            rusqlite::params![super::EMBEDDING_MODEL_ID, super::EMBEDDING_DIMENSION as i64],
        )
        .expect("mark empty generation ready");

        let error = super::semantic_search(&conn, "needle", 5)
            .expect_err("a non-vec build must report semantic search unavailable");

        assert!(matches!(
            error,
            crate::error::AppError::Embed(message)
                if message.contains("sqlite-vec semantic search unavailable")
        ));
    }

    #[test]
    fn bundled_model_directory_requires_verified_ready_marker() {
        let temp = tempdir().expect("create model fixture directory");

        let error = validate_bundled_model_directory(temp.path())
            .expect_err("unverified model directory must be rejected");

        assert!(matches!(error, crate::error::AppError::Embed(_)));
    }

    #[test]
    fn bundled_model_directory_rejects_marker_from_another_revision() {
        let temp = tempdir().expect("create model fixture directory");
        write_complete_model_fixture(temp.path(), "0000000000000000000000000000000000000000");

        let error = validate_bundled_model_directory(temp.path())
            .expect_err("a stale marker revision must not identify the pinned model");

        assert!(
            matches!(error, crate::error::AppError::Embed(message) if message.contains("revision"))
        );
    }

    #[test]
    fn bundled_model_directory_rejects_replaced_onnx_even_with_pinned_marker() {
        let temp = tempdir().expect("create model fixture directory");
        write_complete_model_fixture(temp.path(), super::EMBEDDING_MODEL_REVISION);

        let error = validate_bundled_model_directory(temp.path())
            .expect_err("the ONNX digest must be verified against the pinned model identity");

        assert!(
            matches!(error, crate::error::AppError::Embed(message) if message.contains("SHA-256"))
        );
    }
}
