//! Hybrid retrieval broker — unified search across five layers.
//!
//! Layers: FTS → Vector → Graph → Exact Parser → Template
//! Results are fused by weighted score and returned as ContextPackets.

use rusqlite::Connection;

use crate::ai_runtime::retrieval_scope::RetrievalScope;
use crate::ai_runtime::ContextPacket;
use crate::error::AppResult;

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
#[path = "retrieval_broker/rank.rs"]
mod rank_impl;
#[path = "retrieval_broker/template.rs"]
mod template_impl;
#[path = "retrieval_broker/vector.rs"]
mod vector_impl;

use exact_impl::search_exact_regulation;
use fts_impl::search_fts;
use graph_impl::search_graph_neighbors;
use rank_impl::fuse_and_rank;
use template_impl::search_template;
use vector_impl::{search_vector_anchors, search_vector_chunks, search_vector_regulations};

// ─── Retrieval Request ───────────────────────────────────

/// 检索请求，定义查询内容和检索参数。
#[derive(Debug, Clone)]
pub struct RetrievalRequest {
    /// 用户查询文本
    pub query: String,
    /// 最大返回结果数
    pub max_results: usize,
    /// 启用的检索层配置
    pub layers: RetrievalLayers,
    /// 当前笔记路径，用于图谱反向链接增强
    pub note_context: Option<String>,
    /// 当前笔记的文件 ID，用于图谱邻居检索
    pub file_id_context: Option<i64>,
    /// 检索范围约束
    pub scope: RetrievalScope,
}

/// 检索层开关，控制启用哪些检索通道。
#[derive(Debug, Clone)]
pub struct RetrievalLayers {
    /// 全文检索（FTS5 关键词匹配）
    pub fts: bool,
    /// 向量检索（sqlite-vec 语义相似度）
    pub vector: bool,
    /// 图谱检索（已确认的链接邻居）
    pub graph: bool,
    /// 精确匹配（法规条文号解析）
    pub exact: bool,
    /// 模板匹配（文种模板）
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

// ─── Unified Retrieval ───────────────────────────────────

/// 执行混合检索，返回融合评分后的证据包列表。
///
/// 按以下顺序依次检索各层，结果合并后由 [`fuse_and_rank`] 统一评分：
///
/// 1. **FTS** — FTS5 全文关键词匹配
/// 2. **Vector** — sqlite-vec 向量相似度（chunks / anchors / regulations）
/// 3. **Graph** — 已确认链接的邻居文件
/// 4. **Exact** — 法规条文号精确解析（如 `《纪律处分条例》第六条`）
/// 5. **Template** — 文种模板关键词匹配
///
/// 各层内部错误会被降级为诊断信息，不会中断整体检索。
///
/// # Arguments
///
/// - `conn` — SQLite 数据库连接
/// - `request` — 检索请求参数
///
/// # Returns
///
/// 按加权评分降序排列的证据包列表，已去重且不超过 `max_results`。
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
        format!("{}…", s.chars().take(max_chars).collect::<String>())
    }
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieval_request_default_layers() {
        let req = RetrievalRequest {
            query: "test".into(),
            max_results: 10,
            layers: RetrievalLayers::default(),
            note_context: None,
            file_id_context: None,
            scope: RetrievalScope::default(),
        };
        assert!(req.layers.fts);
        assert!(req.layers.vector);
        assert!(req.layers.graph);
        assert!(req.layers.exact);
        assert!(!req.layers.template);
    }

    #[test]
    fn exact_regulation_regex_matches() {
        let query = "《纪律处分条例》第六条怎么规定";
        let re = regex::Regex::new(r"《([^》]+)》\s*第([一二三四五六七八九十百千0-9]+)条").unwrap();
        assert!(re.is_match(query));
    }

    #[test]
    fn hybrid_retrieve_empty_db_returns_empty() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let req = RetrievalRequest {
            query: "《纪律处分条例》第六条".into(),
            max_results: 10,
            layers: RetrievalLayers::default(),
            note_context: None,
            file_id_context: None,
            scope: RetrievalScope::default(),
        };
        let packets = hybrid_retrieve(&conn, &req).unwrap();
        // No tables exist in a fresh in-memory DB, so all layers should fail gracefully
        assert!(packets.is_empty());
    }

    #[test]
    fn hybrid_retrieve_with_diagnostics_reports_unavailable_layers() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let req = RetrievalRequest {
            query: "《纪律处分条例》第六条".into(),
            max_results: 10,
            layers: RetrievalLayers::default(),
            note_context: None,
            file_id_context: None,
            scope: RetrievalScope::default(),
        };

        let outcome = hybrid_retrieve_with_diagnostics(&conn, &req).unwrap();
        assert!(outcome.packets.is_empty());
        assert!(outcome.diagnostics.iter().any(|diag| {
            diag.layer == "fts" && diag.status == RetrievalLayerStatus::Unavailable
        }));
        assert!(outcome.diagnostics.iter().any(|diag| {
            diag.layer == "vector" && diag.status == RetrievalLayerStatus::IndexNotReady
        }));
    }

    #[test]
    fn truncate_within_limit() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exceeds_limit() {
        let long = "a".repeat(100);
        let result = truncate(&long, 20);
        assert!(result.ends_with('…'));
        assert_eq!(result.chars().count(), 21); // 20 chars + '…'
    }

    #[test]
    fn truncate_empty() {
        assert_eq!(truncate("", 10), "");
    }
}
