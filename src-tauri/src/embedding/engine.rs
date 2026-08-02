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

/// Maximum chunks for Rust cosine fallback (avoids loading entire vault into memory).
const MAX_COSINE_FALLBACK_CHUNKS: i64 = 8_000;

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
    if phase != "ready"
        || active_model_id != EMBEDDING_MODEL_ID
        || target_model_id != EMBEDDING_MODEL_ID
        || target_dimension != EMBEDDING_DIMENSION as i64
        || indexed != total
    {
        return Ok(false);
    }

    match super::scheduler::generation_coverage_complete(conn) {
        Ok(complete) => Ok(complete),
        Err(AppError::Db(_)) => Ok(false),
        Err(error) => Err(error),
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
    use super::{bytes_to_f32, f32_to_bytes, initialize_once, validate_bundled_model_directory};
    use std::fs;
    use std::sync::{Mutex, OnceLock};
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
