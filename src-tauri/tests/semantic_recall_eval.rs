//! 语义搜索 Recall@5 评测（fixture vault）。
//!
//! 首次运行会下载 fastembed 模型，较慢：
//! `cargo test semantic_recall_at_5_on_fixture_vault -- --ignored --nocapture`

use iris_lib::app::AppState;
use iris_lib::embedding::engine::{semantic_search, SemanticHit};
use iris_lib::indexer::scan::{index_vault_incremental, IndexEmbeddingMode};
use iris_lib::storage::migrate::migrate_up;
use rusqlite::Connection;
use std::path::PathBuf;

/// (查询, 期望命中的笔记 path)
const EVAL_QUERIES: &[(&str, &str)] = &[
    ("性能优化 帧率 reindex profiling", "perf-meeting.md"),
    ("SQLite 元数据与 FTS 索引", "sqlite-arch.md"),
    ("Tauri 2 桌面应用", "tauri-stack.md"),
    ("TipTap ai-stream 流式", "tiptap-editor.md"),
    ("MCP 凭据作用域", "credentials-security.md"),
    ("all-MiniLM-L6-v2 嵌入", "embedding-model.md"),
    ("search_semantic 关联笔记", "semantic-search-impl.md"),
    ("未配置 MCP 搜索提供方", "web-search-fallback.md"),
    ("frontmatter tags 表", "frontmatter-tags.md"),
    ("FileWatcher notify 监听", "file-watcher.md"),
    ("Anthropic content_block_delta", "anthropic-api.md"),
    ("htmlToMarkdown round-trip", "markdown-roundtrip.md"),
    ("内联 AI 接受回退", "inline-ai.md"),
    ("双向链接 力导向图", "knowledge-graph-v02.md"),
    ("AGPL-3.0 依赖许可", "agpl-license.md"),
    ("chunk_markdown 分块", "chunking-strategy.md"),
    ("files_fts unicode61", "fts-keyword.md"),
    ("Ollama 11434 本地", "ollama-local.md"),
    ("混合检索 broker 融合", "semantic-search-impl.md"),
    ("Recall@5 0.6 目标", "eval-recall.md"),
];

const RECALL_TARGET: f32 = 0.6;
const NO_ANSWER_QUERIES: &[&str] = &[
    "火星基地氧气农业排班",
    "量子咖啡机保修政策",
    "古典油画颜料配方库存",
];
const NO_ANSWER_SCORE_THRESHOLD: f32 = 0.55;

#[derive(Debug, Default, Clone)]
struct RetrievalMetrics {
    total: usize,
    hits_at_5: usize,
    hits_at_10: usize,
    reciprocal_rank_sum: f32,
    no_answer_queries: usize,
    no_answer_false_positives: usize,
}

impl RetrievalMetrics {
    fn recall_at_5(&self) -> f32 {
        ratio(self.hits_at_5, self.total)
    }

    fn recall_at_10(&self) -> f32 {
        ratio(self.hits_at_10, self.total)
    }

    fn mrr(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.reciprocal_rank_sum / self.total as f32
        }
    }

    fn no_answer_false_positive_rate(&self) -> f32 {
        ratio(self.no_answer_false_positives, self.no_answer_queries)
    }
}

fn ratio(numerator: usize, denominator: usize) -> f32 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f32 / denominator as f32
    }
}

fn fixture_vault_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("docs")
        .join("eval")
        .join("fixtures")
        .join("semantic-vault")
}

fn recall_at_k(hits: &[SemanticHit], expected: &str, k: usize) -> bool {
    hits.iter().take(k).any(|h| h.path == expected)
}

fn reciprocal_rank_at_k(hits: &[SemanticHit], expected: &str, k: usize) -> f32 {
    hits.iter()
        .take(k)
        .position(|h| h.path == expected)
        .map(|idx| 1.0 / (idx + 1) as f32)
        .unwrap_or(0.0)
}

fn record_labeled_query(metrics: &mut RetrievalMetrics, hits: &[SemanticHit], expected: &str) {
    metrics.total += 1;
    if recall_at_k(hits, expected, 5) {
        metrics.hits_at_5 += 1;
    }
    if recall_at_k(hits, expected, 10) {
        metrics.hits_at_10 += 1;
    }
    metrics.reciprocal_rank_sum += reciprocal_rank_at_k(hits, expected, 10);
}

fn record_no_answer_query(metrics: &mut RetrievalMetrics, hits: &[SemanticHit]) {
    metrics.no_answer_queries += 1;
    let confident_hit = hits
        .first()
        .is_some_and(|hit| hit.score >= NO_ANSWER_SCORE_THRESHOLD);
    if confident_hit {
        metrics.no_answer_false_positives += 1;
    }
}

#[test]
#[ignore = "loads fastembed model; run: cargo test semantic_recall_at_5_on_fixture_vault -- --ignored --nocapture"]
fn semantic_recall_at_5_on_fixture_vault() {
    let vault = fixture_vault_path();
    assert!(
        vault.is_dir(),
        "fixture vault missing at {}",
        vault.display()
    );

    let conn = Connection::open_in_memory().unwrap();
    migrate_up(&conn).unwrap();
    index_vault_incremental(&conn, &vault, IndexEmbeddingMode::Sync).expect("index fixture vault");

    let chunk_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
        .unwrap();
    assert!(chunk_count > 0, "expected indexed chunks");

    let mut metrics = RetrievalMetrics::default();
    for (i, (query, expected)) in EVAL_QUERIES.iter().enumerate() {
        let hits = semantic_search(&conn, query, 10).expect("semantic_search");
        record_labeled_query(&mut metrics, &hits, expected);
        eprintln!(
            "[{i:02}] q={query:?} expect={expected} hit@5={} hit@10={} rr@10={:.3} top={:?}",
            recall_at_k(&hits, expected, 5),
            recall_at_k(&hits, expected, 10),
            reciprocal_rank_at_k(&hits, expected, 10),
            hits.iter()
                .take(3)
                .map(|h| (&h.path, h.score))
                .collect::<Vec<_>>()
        );
    }
    for query in NO_ANSWER_QUERIES {
        let hits = semantic_search(&conn, query, 10).expect("semantic_search no-answer");
        record_no_answer_query(&mut metrics, &hits);
        eprintln!(
            "[no-answer] q={query:?} top={:?}",
            hits.first().map(|h| (&h.path, h.score))
        );
    }

    eprintln!(
        "Recall@5={:.3} Recall@10={:.3} MRR@10={:.3} no_answer_false_positive_rate={:.3}",
        metrics.recall_at_5(),
        metrics.recall_at_10(),
        metrics.mrr(),
        metrics.no_answer_false_positive_rate()
    );
    assert!(
        metrics.recall_at_5() >= RECALL_TARGET,
        "Recall@5 {:.3} below target {RECALL_TARGET}",
        metrics.recall_at_5()
    );
}

#[test]
#[ignore = "loads fastembed model"]
fn semantic_recall_via_app_state_db() {
    let vault = fixture_vault_path();
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::new(dir.path().to_path_buf()).unwrap();
    state.set_vault(vault.clone()).unwrap();
    state
        .db
        .with_conn(|conn| index_vault_incremental(conn, &vault, IndexEmbeddingMode::Sync))
        .unwrap();

    let (query, expected) = EVAL_QUERIES[0];
    let hits = state
        .db
        .with_conn(|conn| semantic_search(conn, query, 5))
        .unwrap();
    assert!(recall_at_k(&hits, expected, 5));
}

#[test]
fn retrieval_metrics_compute_recall_mrr_and_no_answer_rate() {
    let mut metrics = RetrievalMetrics::default();
    let hits = vec![
        SemanticHit {
            chunk_id: 1,
            path: "a.md".into(),
            title: "A".into(),
            snippet: "a".into(),
            score: 0.9,
        },
        SemanticHit {
            chunk_id: 2,
            path: "expected.md".into(),
            title: "Expected".into(),
            snippet: "expected".into(),
            score: 0.8,
        },
    ];

    record_labeled_query(&mut metrics, &hits, "expected.md");
    record_no_answer_query(&mut metrics, &hits);
    record_no_answer_query(&mut metrics, &[]);

    assert_eq!(metrics.recall_at_5(), 1.0);
    assert_eq!(metrics.recall_at_10(), 1.0);
    assert_eq!(metrics.mrr(), 0.5);
    assert_eq!(metrics.no_answer_false_positive_rate(), 0.5);
}
