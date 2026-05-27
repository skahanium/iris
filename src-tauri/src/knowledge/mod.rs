//! Knowledge index modules: anchors, regulations, templates, graph links.
//!
//! These modules build and maintain the derived knowledge cache that
//! powers hybrid retrieval and AI context assembly. All data is
//! reconstructible from `.md` files.

pub mod anchors;
pub mod graph;
pub mod regulations;
pub mod templates;

use sha2::{Digest, Sha256};

/// Current extractor version — bump when extraction logic changes.
pub const EXTRACTOR_VERSION: &str = "0.1.0";

/// Current embedding model identifier.
pub const EMBEDDING_MODEL: &str = "fastembed/AllMiniLML6V2";
pub const EMBEDDING_DIM: i32 = 384;

/// Generate a stable `anchor_key` from file path, source span, and content hash.
///
/// Format: `sha256(path).truncate(12) + sha256(content).truncate(12)`
/// This produces a 24-char hex key that is stable across database rebuilds.
pub fn make_anchor_key(
    path: &str,
    source_start: usize,
    source_end: usize,
    content: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    hasher.update(b":");
    hasher.update(source_start.to_string().as_bytes());
    hasher.update(b"-");
    hasher.update(source_end.to_string().as_bytes());
    hasher.update(b":");
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..12])
}

/// Generate a stable `template_key` from genre and source.
pub fn make_template_key(genre: &str, source_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(genre.as_bytes());
    hasher.update(b":");
    hasher.update(source_path.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// Generate `content_hash` for deduplication and change detection.
pub fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_key_is_stable() {
        let k1 = make_anchor_key("/notes/test.md", 10, 50, "hello world");
        let k2 = make_anchor_key("/notes/test.md", 10, 50, "hello world");
        assert_eq!(k1, k2);
    }

    #[test]
    fn anchor_key_differs_by_span() {
        let k1 = make_anchor_key("/notes/test.md", 10, 50, "same");
        let k2 = make_anchor_key("/notes/test.md", 20, 60, "same");
        assert_ne!(k1, k2);
    }

    #[test]
    fn anchor_key_differs_by_content() {
        let k1 = make_anchor_key("/notes/test.md", 0, 5, "hello");
        let k2 = make_anchor_key("/notes/test.md", 0, 5, "world");
        assert_ne!(k1, k2);
    }

    #[test]
    fn content_hash_deterministic() {
        assert_eq!(content_hash("abc"), content_hash("abc"));
        assert_ne!(content_hash("abc"), content_hash("abd"));
    }
}
