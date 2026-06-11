//! Hybrid retrieval broker — unified search across five layers.
//!
//! Layers: FTS → Vector → Graph → Exact Parser → Template
//! Results are fused by weighted score and returned as ContextPackets.

use rusqlite::Connection;

use crate::ai_runtime::packet_cache::PacketCache;
use crate::ai_runtime::retrieval_scope::{filter_packets_by_scope, RetrievalScope};
use crate::ai_runtime::ContextPacket;
use crate::error::AppResult;

#[path = "retrieval_broker/query_hash.rs"]
mod query_hash_impl;
pub use query_hash_impl::query_hash;

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
/// 各层内部错误会被静默忽略（表不存在等情况），不会中断整体检索。
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
    let mut packets: Vec<ContextPacket> = Vec::new();

    // Layer 1: FTS (keyword + regulation name)
    if request.layers.fts {
        if let Ok(fts_results) = search_fts(conn, &request.query, request.max_results) {
            packets.extend(fts_results);
        }
    }

    // Layer 2: Vector (chunks + anchors + regulations)
    if request.layers.vector && crate::storage::db::vector_index_ready() {
        if let Ok(chunk_results) = search_vector_chunks(conn, &request.query, request.max_results) {
            packets.extend(chunk_results);
        }
        if let Ok(vec_results) = search_vector_anchors(conn, &request.query, request.max_results) {
            packets.extend(vec_results);
        }
        if let Ok(reg_results) =
            search_vector_regulations(conn, &request.query, request.max_results)
        {
            packets.extend(reg_results);
        }
    }

    // Layer 3: Graph (confirmed links)
    if request.layers.graph {
        if let Some(file_id) = request.file_id_context {
            if let Ok(graph_results) =
                search_graph_neighbors(conn, file_id, request.max_results / 2)
            {
                packets.extend(graph_results);
            }
        }
    }

    // Layer 4: Exact parser (regulation article lookup)
    if request.layers.exact {
        if let Ok(exact_results) = search_exact_regulation(conn, &request.query) {
            packets.extend(exact_results);
        }
    }

    // Layer 5: Template (genre template match)
    if request.layers.template {
        if let Ok(template_results) = search_template(conn, &request.query, request.max_results) {
            packets.extend(template_results);
        }
    }

    // Score fusion: normalize and weight by layer, then deduplicate
    fuse_and_rank(&mut packets, request.max_results);

    filter_packets_by_scope(&mut packets, &request.scope, |p| p.source_path.as_deref());

    Ok(packets)
}

/// 计算检索请求的稳定哈希值，用于缓存键。
///
/// 基于 `query`、`layers` 开关和 `max_results` 生成，
/// Run hybrid retrieval with an in-memory packet cache keyed by [`query_hash`].
pub fn hybrid_retrieve_cached(
    conn: &Connection,
    request: &RetrievalRequest,
    cache: &mut PacketCache,
) -> AppResult<Vec<ContextPacket>> {
    let hash = query_hash(request);
    if let Some(cached) = cache.get(&hash) {
        return Ok(cached);
    }
    let packets = hybrid_retrieve(conn, request)?;
    cache.insert(hash, packets.clone());
    Ok(packets)
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
            query: "测试".into(),
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
