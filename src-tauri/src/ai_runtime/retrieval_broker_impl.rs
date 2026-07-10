//! Hybrid retrieval broker: request contract and layer wiring.

use rusqlite::Connection;

use crate::ai_runtime::retrieval_scope::RetrievalScope;
use crate::ai_runtime::{ContextPacket, RuntimeDocumentSnapshot};
use crate::error::AppResult;
use crate::knowledge::corpora::CorpusConfig;

#[path = "retrieval_broker/query_hash.rs"]
mod query_hash_impl;
pub use query_hash_impl::query_hash;

#[path = "retrieval_broker/diagnostics.rs"]
mod diagnostics_impl;
pub use diagnostics_impl::{
    hybrid_retrieve_with_diagnostics, RetrievalLayerDiagnostic, RetrievalLayerStatus,
    RetrievalOutcome,
};

#[path = "retrieval_broker/exact.rs"]
mod exact_impl;
#[path = "retrieval_broker/fts.rs"]
mod fts_impl;
#[path = "retrieval_broker/graph.rs"]
mod graph_impl;
#[path = "retrieval_broker/metadata.rs"]
mod metadata_impl;
#[path = "retrieval_broker/rank.rs"]
mod rank_impl;
#[path = "retrieval_broker/template.rs"]
mod template_impl;
#[path = "retrieval_broker/vector.rs"]
mod vector_impl;

use exact_impl::search_exact_regulation;
pub use fts_impl::escape_fts5_query;
use fts_impl::search_fts;
use graph_impl::search_graph_neighbors;
use metadata_impl::search_metadata;
use rank_impl::fuse_and_rank;
use template_impl::search_template;
use vector_impl::{search_vector_anchors, search_vector_chunks, search_vector_regulations};

/// Complete request contract for one hybrid retrieval call.
#[derive(Debug, Clone)]
pub struct RetrievalRequest {
    pub query: String,
    pub max_results: usize,
    pub layers: RetrievalLayers,
    pub note_context: Option<String>,
    pub file_id_context: Option<i64>,
    pub scope: RetrievalScope,
    pub runtime_documents: Vec<RuntimeDocumentSnapshot>,
    pub corpus_config: Option<CorpusConfig>,
}

/// Independently enabled retrieval layers.
#[derive(Debug, Clone)]
pub struct RetrievalLayers {
    pub fts: bool,
    pub vector: bool,
    pub graph: bool,
    pub exact: bool,
    pub template: bool,
}

impl Default for RetrievalLayers {
    fn default() -> Self {
        Self {
            fts: true,
            vector: true,
            graph: true,
            exact: true,
            template: false,
        }
    }
}

/// Run hybrid retrieval and return fused evidence packets.
pub fn hybrid_retrieve(
    conn: &Connection,
    request: &RetrievalRequest,
) -> AppResult<Vec<ContextPacket>> {
    Ok(hybrid_retrieve_with_diagnostics(conn, request)?.packets)
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max_chars).collect::<String>())
    }
}

/// Expand an excerpt around a source span with paragraph-boundary awareness.
///
/// Extends up to `margin` chars before and after the span, snapping to paragraph
/// boundaries (double newline). Respects UTF-8 character boundaries and the
/// content length.
#[allow(dead_code)]
pub(crate) fn expand_span_excerpt(
    content: &str,
    span: &crate::ai_runtime::SourceSpan,
    margin: usize,
) -> String {
    let len = content.len();
    if len == 0 || span.start >= len || span.end > len || span.start >= span.end {
        return truncate(content, margin * 2);
    }

    // Ensure byte positions are on character boundaries.
    let safe_start = floor_char_boundary(content, span.start);
    let safe_end = ceil_char_boundary(content, span.end.min(len));

    // Extend start backward, stopping at paragraph boundaries.
    let mut excerpt_start = safe_start;
    let chars_before = content[..excerpt_start].chars().count();
    let target_before = chars_before.saturating_sub(margin);
    let mut char_count = chars_before;
    for (byte_pos, _) in content[..excerpt_start].char_indices().rev() {
        if char_count <= target_before {
            break;
        }
        // Snap to paragraph boundary (double newline or start of content).
        if content.as_bytes().get(byte_pos) == Some(&b'\n')
            && byte_pos > 0
            && content.as_bytes().get(byte_pos - 1) == Some(&b'\n')
        {
            excerpt_start = byte_pos + 1; // after the second \n
            break;
        }
        excerpt_start = byte_pos;
        char_count = char_count.saturating_sub(1);
    }

    // Extend end forward, stopping at paragraph boundaries.
    let mut excerpt_end = safe_end;
    let chars_after = content[excerpt_end..].chars().count();
    let target_after = margin.min(chars_after);
    let mut last_was_newline =
        content.as_bytes().get(excerpt_end.saturating_sub(1)) == Some(&b'\n');
    for (char_count, (byte_pos, _)) in content[excerpt_end..].char_indices().enumerate() {
        if char_count >= target_after {
            break;
        }
        let abs_pos = excerpt_end + byte_pos;
        let ch = content.as_bytes().get(abs_pos);
        if last_was_newline && ch == Some(&b'\n') {
            excerpt_end = abs_pos.saturating_sub(1); // before the \n\n
            break;
        }
        last_was_newline = ch == Some(&b'\n');
        excerpt_end = abs_pos
            + content[abs_pos..]
                .chars()
                .next()
                .map_or(0, |c| c.len_utf8());
    }

    // Clamp to content boundaries.
    excerpt_start = excerpt_start.min(len);
    excerpt_end = excerpt_end.min(len);
    if excerpt_start >= excerpt_end {
        excerpt_start = safe_start;
        excerpt_end = safe_end;
    }

    content[excerpt_start..excerpt_end].to_string()
}

#[allow(dead_code)]
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if s.is_char_boundary(index) {
        index
    } else {
        (0..index)
            .rev()
            .find(|&i| s.is_char_boundary(i))
            .unwrap_or(0)
    }
}

#[allow(dead_code)]
fn ceil_char_boundary(s: &str, index: usize) -> usize {
    let index = index.min(s.len());
    if s.is_char_boundary(index) {
        index
    } else {
        ((index + 1)..=s.len())
            .find(|&i| s.is_char_boundary(i))
            .unwrap_or(s.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieval_request_default_layers_are_enabled_as_expected() {
        let request = RetrievalRequest {
            query: "test".into(),
            max_results: 10,
            layers: RetrievalLayers::default(),
            note_context: None,
            file_id_context: None,
            scope: RetrievalScope::default(),
            runtime_documents: Vec::new(),
            corpus_config: None,
        };
        assert!(request.layers.fts);
        assert!(request.layers.vector);
        assert!(request.layers.graph);
        assert!(request.layers.exact);
        assert!(!request.layers.template);
    }

    #[test]
    fn empty_database_returns_no_packets() {
        let conn = rusqlite::Connection::open_in_memory().expect("open database");
        let request = RetrievalRequest {
            query: "article 6".into(),
            max_results: 10,
            layers: RetrievalLayers::default(),
            note_context: None,
            file_id_context: None,
            scope: RetrievalScope::default(),
            runtime_documents: Vec::new(),
            corpus_config: None,
        };
        assert!(hybrid_retrieve(&conn, &request)
            .expect("retrieve")
            .is_empty());
    }

    #[test]
    fn truncate_preserves_short_values_and_marks_long_values() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate(&"a".repeat(100), 20).chars().count(), 23);
    }
}
