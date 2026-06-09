import os
path = r"D:\Iris\src-tauri\src\embedding\engine.rs"
with open(path, "r", encoding="utf-8") as f:
    content = f.read()

# Replace use std::sync::RwLock with OnceLock
content = content.replace("use std::sync::RwLock;", "use std::sync::OnceLock;")

# Replace static EMBEDDER
old_static = """static EMBEDDER: RwLock<Option<TextEmbedding>> = RwLock::new(None);"""
new_static = """static EMBEDDER: OnceLock<TextEmbedding> = OnceLock::new();"""
content = content.replace(old_static, new_static)

# Replace embed_text function
old_fn1 = """pub fn embed_text(text: &str) -> AppResult<Vec<f32>> {
    // Fast path: model already loaded \u2192 concurrent read lock
    if let Ok(guard) = EMBEDDER.read() {
        if let Some(ref model) = *guard {
            return model
                .embed(vec![text], None)
                .map_err(|e| AppError::Embed(e.to_string()))?
                .into_iter()
                .next()
                .ok_or_else(|| AppError::msg("Empty embedding result"));
        }
    }
    // Cold path: lazy init under exclusive write lock
    let mut guard = EMBEDDER
        .write()
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
}"""

new_fn1 = """/// Return a shared reference to the global embedding model.
/// On first call, loads the model (blocks calling thread briefly).
/// Subsequent calls return immediately with zero contention.
fn get_embedder() -> &'static TextEmbedding {
    EMBEDDER.get_or_init(|| {
        TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))
            .expect("Failed to load embedding model")
    })
}

/// Generate embedding vector for text.
pub fn embed_text(text: &str) -> AppResult<Vec<f32>> {
    let model = get_embedder();
    model
        .embed(vec![text], None)
        .map_err(|e| AppError::Embed(e.to_string()))?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::msg("Empty embedding result"))
}"""

assert old_fn1 in content, "old_fn1 not found"
content = content.replace(old_fn1, new_fn1)

# Replace embed_texts_batch function
old_fn2 = """pub fn embed_texts_batch(texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    // Fast path: model already loaded \u2192 concurrent read lock
    if let Ok(guard) = EMBEDDER.read() {
        if let Some(ref model) = *guard {
            return model
                .embed(texts.to_vec(), None)
                .map_err(|e| AppError::Embed(e.to_string()));
        }
    }
    // Cold path: lazy init under exclusive write lock
    let mut guard = EMBEDDER
        .write()
        .map_err(|_| AppError::msg("embedder lock"))?;
    if guard.is_none() {
        *guard = Some(
            TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))
                .map_err(|e| AppError::Embed(e.to_string()))?,
        );
    }
    let model = guard.as_ref().expect("embedder initialized");
    model
        .embed(texts.to_vec(), None)
        .map_err(|e| AppError::Embed(e.to_string()))
}"""

new_fn2 = """/// Batch-embed multiple texts in a single model call for better throughput.
pub fn embed_texts_batch(texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let model = get_embedder();
    model
        .embed(texts.to_vec(), None)
        .map_err(|e| AppError::Embed(e.to_string()))
}"""

assert old_fn2 in content, "old_fn2 not found"
content = content.replace(old_fn2, new_fn2)

# Also update the doc comment for EMBEDDER
old_doc = """/// Global embedding model, protected by RwLock for concurrent read access.
///
/// `TextEmbedding::embed()` takes `&Self` and fastembed documents the type as
/// `Send + Sync`, so multiple threads can safely call `embed()` concurrently
/// once the model is initialized.  The read-first-then-write pattern ensures:
///
/// - **Hot path** (model loaded): `read()` lock — concurrent calls from the
///   background embed worker and semantic search proceed in parallel.
/// - **Cold path** (lazy init): `write()` lock — only the first caller loads
///   the model; subsequent callers never contend for the write lock.
static EMBEDDER"""

new_doc = """/// Global embedding model, lazy-initialized via OnceLock.
///
/// `TextEmbedding::embed()` takes `&Self` and fastembed documents the type as
/// `Send + Sync`, so multiple threads can safely call `embed()` concurrently.
///
/// OnceLock ensures only the first caller pays the model-load cost;
/// all subsequent calls get a shared reference with zero contention.
static EMBEDDER"""

assert old_doc in content, "old_doc not found"
content = content.replace(old_doc, new_doc)

with open(path, "w", encoding="utf-8") as f:
    f.write(content)
print("OK")
