//! 语义搜索 Recall@5 评测（fixture vault）。
//!
//! 首次运行会下载 fastembed 模型，较慢：
//! `cargo test semantic_recall_at_5_on_fixture_vault -- --ignored --nocapture`

use std::path::PathBuf;
use std::sync::Arc;

use iris_lib::app::AppState;
use iris_lib::embedding::engine::{semantic_search, SemanticHit};
use iris_lib::indexer::scan::scan_vault;
use iris_lib::storage::migrate::migrate_up;
use rusqlite::Connection;

/// (查询, 期望命中的笔记 path)
const EVAL_QUERIES: &[(&str, &str)] = &[
    ("性能优化 帧率 reindex profiling", "perf-meeting.md"),
    ("SQLite 元数据与 FTS 索引", "sqlite-arch.md"),
    ("Tauri 2 桌面应用", "tauri-stack.md"),
    ("TipTap ai-stream 流式", "tiptap-editor.md"),
    ("iris.minimax 凭据", "credentials-security.md"),
    ("all-MiniLM-L6-v2 嵌入", "embedding-model.md"),
    ("search_semantic 关联笔记", "semantic-search-impl.md"),
    ("MiniMax 失败 DuckDuckGo", "web-search-fallback.md"),
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
    ("AES-256-GCM 加密", "vault-encryption.md"),
    ("Recall@5 0.6 目标", "eval-recall.md"),
];

const RECALL_TARGET: f32 = 0.6;

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
    scan_vault(&conn, &vault).expect("index fixture vault");

    let chunk_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
        .unwrap();
    assert!(chunk_count > 0, "expected indexed chunks");

    let mut hits_count = 0usize;
    for (i, (query, expected)) in EVAL_QUERIES.iter().enumerate() {
        let hits = semantic_search(&conn, query, 5).expect("semantic_search");
        let ok = recall_at_k(&hits, expected, 5);
        if ok {
            hits_count += 1;
        }
        eprintln!(
            "[{i:02}] q={query:?} expect={expected} hit={} top={:?}",
            ok,
            hits.iter()
                .take(3)
                .map(|h| (&h.path, h.score))
                .collect::<Vec<_>>()
        );
    }

    let recall = hits_count as f32 / EVAL_QUERIES.len() as f32;
    eprintln!(
        "Recall@5 = {hits_count}/{} = {recall:.3}",
        EVAL_QUERIES.len()
    );
    assert!(
        recall >= RECALL_TARGET,
        "Recall@5 {recall:.3} below target {RECALL_TARGET}"
    );
}

#[test]
#[ignore = "loads fastembed model"]
fn semantic_recall_via_app_state_db() {
    let vault = fixture_vault_path();
    let dir = tempfile::tempdir().unwrap();
    let state = Arc::new(AppState::new(dir.path().to_path_buf()).unwrap());
    state.set_vault(vault.clone()).unwrap();
    state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();

    let (query, expected) = EVAL_QUERIES[0];
    let hits = state
        .db
        .with_conn(|conn| semantic_search(conn, query, 5))
        .unwrap();
    assert!(recall_at_k(&hits, expected, 5));
}
