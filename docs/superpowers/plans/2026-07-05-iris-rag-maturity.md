# Iris RAG Maturity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` for independent tasks or `superpowers:executing-plans` for inline execution. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Iris 现有 RAG 从“有骨架但易断链”升级为可信、可诊断、可引用、可评测的本地优先证据系统，并保留助手自然协作感。

**Architecture:** 沿现有 `indexer -> embedding/FTS/sqlite-vec -> retrieval_broker -> packet_builder -> EvidenceLedger -> harness` 演进。P0 修复语义检索和 broker 断链；P1 增量加入 chunk metadata、ranking v2 和评测；P2 强化 research coverage，而不重写 Agent 或改变默认 UI 流程。

**Tech Stack:** Rust 2021, Tauri 2.x, rusqlite/SQLite FTS5/sqlite-vec optional, fastembed, React 19, TypeScript, Vitest, existing Iris AI runtime.

---

## File Structure

- Modify: `src-tauri/src/embedding/engine.rs`  
  修复 semantic search SQL，增加可测试的 classified path 过滤和 fallback 行为。
- Modify: `src-tauri/src/ai_runtime/retrieval_broker/vector.rs`  
  使用当前 `chunks.content` schema，后续读取 chunk v2 metadata。
- Modify: `src-tauri/src/ai_runtime/retrieval_broker/diagnostics.rs`  
  扩展 diagnostics 分类，避免空结果掩盖错误。
- Modify: `src-tauri/src/ai_runtime/retrieval_broker/rank.rs`  
  承载 ranking v2：归一、去重、多样性和语料权重。
- Modify: `src-tauri/src/indexer/chunker.rs`  
  从 `Vec<String>` 逐步扩展到 chunk metadata，同时保留旧函数兼容。
- Modify: `src-tauri/src/indexer/scan.rs`  
  写入 chunk v2 metadata，并继续兼容旧索引路径。
- Create: `src-tauri/migrations/043_chunk_evidence_metadata.sql`
- Create: `src-tauri/migrations/043_chunk_evidence_metadata.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`
- Modify: `src-tauri/tests/semantic_recall_eval.rs`
- Create: `src-tauri/tests/rag_retrieval_contract.rs`
- Modify: `tests/rust-runtime-refactor-contract.test.ts`
- Modify: `src-tauri/src/ai_workflows/research_workflow.rs`
- Modify: `src/components/ai/ContextPacketDrawer.tsx`
- Modify: `src/components/ai/EvidenceChainView.tsx`
- Test: Rust cargo tests and focused frontend Vitest contracts.

---

## Task 1: P0 回归测试锁定当前语义检索断链

**Files:**

- Create: `src-tauri/tests/rag_retrieval_contract.rs`
- Modify: `src-tauri/src/embedding/engine.rs`

- [ ] **Step 1: 写 failing test，证明 classified path 过滤 SQL 不应报错**

Create `src-tauri/tests/rag_retrieval_contract.rs` with:

```rust
use iris_lib::embedding::engine::semantic_search;
use iris_lib::storage::migrate::migrate_up;
use rusqlite::Connection;

fn setup_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("open memory db");
    migrate_up(&conn).expect("migrate");
    conn
}

#[test]
fn semantic_search_empty_index_returns_empty_not_sql_error() {
    let conn = setup_conn();
    let hits = semantic_search(&conn, "anything", 5).expect("semantic search should not fail");
    assert!(hits.is_empty());
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test rag_retrieval_contract semantic_search_empty_index_returns_empty_not_sql_error -- --nocapture`

Expected before fix: fail with a rusqlite prepare error from the malformed classified path filter. Expected after fix: pass with an empty hit list.

- [ ] **Step 2: 修复 `embedding::engine` SQL classified filter**

In `src-tauri/src/embedding/engine.rs`, replace the malformed filters:

```rust
AND f.path NOT LIKE ''.classified/%''
```

with:

```rust
AND f.path <> '.classified'
AND f.path NOT LIKE '.classified/%'
```

Do this in both `semantic_search_vec` and `semantic_search_cosine`.

- [ ] **Step 3: 验证测试通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test rag_retrieval_contract semantic_search_empty_index_returns_empty_not_sql_error -- --nocapture`

Expected: pass.

- [ ] **Step 4: 跑现有语义评测 smoke**

Run: `cargo test --manifest-path src-tauri/Cargo.toml semantic_recall_via_app_state_db`

Expected: ignored test remains ignored in the normal suite; normal test execution does not download fastembed.

---

## Task 2: P0 修复 broker vector chunk schema 漂移

**Files:**

- Modify: `src-tauri/src/ai_runtime/retrieval_broker/vector.rs`
- Modify: `tests/rust-runtime-refactor-contract.test.ts`

- [ ] **Step 1: 写源码契约测试，证明 vector chunk layer 使用当前 schema**

Append to `tests/rust-runtime-refactor-contract.test.ts`:

```ts
it("keeps vector chunk retrieval aligned with chunks.content schema", () => {
  const vector = read("src-tauri/src/ai_runtime/retrieval_broker/vector.rs");
  expect(vector).toContain("c.content");
  expect(vector).not.toContain("c.text");
  expect(vector).toContain("JOIN chunks c ON c.id = vc.rowid");
});
```

Run: `npm run test -- rust-runtime-refactor-contract`

Expected before fix: fail because `vector.rs` still contains `c.text`. Expected after fix: pass.

- [ ] **Step 2: 修改 vector chunk SELECT 使用当前 schema**

In `src-tauri/src/ai_runtime/retrieval_broker/vector.rs`, change:

```rust
SELECT vc.rowid, c.text, f.path, f.title, c.heading_path,
        c.char_count, vc.distance
```

to:

```rust
SELECT vc.rowid, c.content, f.path, f.title, NULL AS heading_path,
        c.char_count, vc.distance
```

Keep `heading_path` as `None` until Task 5 adds chunk v2 metadata.

- [ ] **Step 3: 验证 broker source contract**

Run: `npm run test -- rust-runtime-refactor-contract`

Expected: pass.

---

## Task 3: P0 扩展 retrieval diagnostics，不用空数组掩盖错误

**Files:**

- Modify: `src-tauri/src/ai_runtime/retrieval_broker/diagnostics.rs`
- Test: `src-tauri/tests/rag_retrieval_contract.rs`

- [ ] **Step 1: 写 diagnostics 分类测试**

Append:

```rust
#[test]
fn missing_schema_is_reported_as_schema_mismatch_or_unavailable() {
    let conn = Connection::open_in_memory().expect("open raw memory db");
    let request = RetrievalRequest {
        query: "anything".to_string(),
        max_results: 5,
        layers: RetrievalLayers {
            fts: true,
            vector: false,
            graph: false,
            exact: false,
            template: false,
        },
        note_context: None,
        file_id_context: None,
        scope: RetrievalScope::default(),
    };

    let outcome = hybrid_retrieve_with_diagnostics(&conn, &request).expect("outcome");
    assert!(outcome.packets.is_empty());
    assert!(outcome.diagnostics.iter().any(|diag| {
        diag.layer == "fts"
            && matches!(
                diag.status,
                iris_lib::ai_runtime::retrieval_broker::RetrievalLayerStatus::Unavailable
            )
    }));
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test rag_retrieval_contract missing_schema_is_reported_as_schema_mismatch_or_unavailable -- --nocapture`

Expected: pass with current `Unavailable`; keep this behavior while adding more precise statuses.

- [ ] **Step 2: Add status variants**

In `RetrievalLayerStatus`, add:

```rust
Empty,
SchemaMismatch,
QueryError,
```

Keep existing variants serialized with `snake_case`.

- [ ] **Step 3: Classify errors more precisely**

Update `classify_retrieval_error`:

```rust
if message.contains("no such column") {
    RetrievalLayerStatus::SchemaMismatch
} else if message.contains("no such table") || message.contains("no such module") {
    RetrievalLayerStatus::Unavailable
} else if message.contains("index is not ready") || message.contains("embedding model") {
    RetrievalLayerStatus::IndexNotReady
} else {
    RetrievalLayerStatus::QueryError
}
```

- [ ] **Step 4: Mark successful empty layers**

In `append_layer_result`, when `Ok(layer_packets)` is empty, push status `Empty`; otherwise `Ok`.

Expected user-facing behavior remains unchanged in P0. UI surfacing is handled in Task 11.

- [ ] **Step 5: Run retrieval diagnostics tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test rag_retrieval_contract -- --nocapture`

Expected: pass.

---

## Task 4: P0 端到端 ContextPacket smoke

**Files:**

- Modify: `src-tauri/tests/rag_retrieval_contract.rs`
- Uses: existing `index_vault_incremental`, `IndexEmbeddingMode::Skip`, FTS path

- [ ] **Step 1: 写 deterministic FTS-to-ContextPacket test**

Append:

```rust
use iris_lib::indexer::scan::{index_file_with_embed, IndexEmbeddingMode};
use std::fs;
use tempfile::tempdir;

#[test]
fn hybrid_retrieve_keyword_hit_becomes_context_packet() {
    let dir = tempdir().expect("tempdir");
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).expect("vault dir");
    let note = vault.join("rag-note.md");
    fs::write(
        &note,
        "# RAG Note\n\nIris retrieval should return this pineapple evidence.",
    )
    .expect("write note");

    let conn = setup_conn();
    index_file_with_embed(&conn, &vault, &note, IndexEmbeddingMode::Skip).expect("index");

    let request = RetrievalRequest {
        query: "pineapple evidence".to_string(),
        max_results: 5,
        layers: RetrievalLayers {
            fts: true,
            vector: false,
            graph: false,
            exact: false,
            template: false,
        },
        note_context: None,
        file_id_context: None,
        scope: RetrievalScope::default(),
    };

    let packets = iris_lib::ai_runtime::retrieval_broker::hybrid_retrieve(&conn, &request)
        .expect("hybrid retrieve");
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].source_path.as_deref(), Some("rag-note.md"));
    assert!(packets[0].excerpt.contains("pineapple"));
    assert_eq!(packets[0].trust_level, iris_lib::ai_runtime::TrustLevel::UserNote);
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test rag_retrieval_contract hybrid_retrieve_keyword_hit_becomes_context_packet -- --nocapture`

Expected: pass.

- [ ] **Step 2: Verify no existing behavior broke**

Run: `cargo test --manifest-path src-tauri/Cargo.toml retrieval_broker -- --nocapture`

Expected: pass.

---

## Task 5: P1 增量 migration 添加 chunk evidence metadata

**Files:**

- Create: `src-tauri/migrations/043_chunk_evidence_metadata.sql`
- Create: `src-tauri/migrations/043_chunk_evidence_metadata.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`
- Test: storage migration tests in `migrate.rs`

- [ ] **Step 1: Add migration SQL**

Create `043_chunk_evidence_metadata.sql`:

```sql
-- 043: Add chunk evidence metadata for mature local RAG.
-- These columns are derived from Markdown and can be rebuilt from .md files.

ALTER TABLE chunks ADD COLUMN heading_path TEXT;
ALTER TABLE chunks ADD COLUMN source_start INTEGER;
ALTER TABLE chunks ADD COLUMN source_end INTEGER;
ALTER TABLE chunks ADD COLUMN content_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_chunks_file_heading
    ON chunks(file_id, heading_path);

CREATE INDEX IF NOT EXISTS idx_chunks_content_hash
    ON chunks(content_hash);
```

Create `043_chunk_evidence_metadata.down.sql`:

```sql
DROP INDEX IF EXISTS idx_chunks_content_hash;
DROP INDEX IF EXISTS idx_chunks_file_heading;
ALTER TABLE chunks DROP COLUMN content_hash;
ALTER TABLE chunks DROP COLUMN source_end;
ALTER TABLE chunks DROP COLUMN source_start;
ALTER TABLE chunks DROP COLUMN heading_path;
```

- [ ] **Step 2: Wire migration**

In `src-tauri/src/storage/migrate.rs`, add constants:

```rust
const MIGRATION_043_UP: &str = include_str!("../../migrations/043_chunk_evidence_metadata.sql");
const MIGRATION_043_DOWN: &str =
    include_str!("../../migrations/043_chunk_evidence_metadata.down.sql");
```

Add `apply_migration(conn, "043_chunk_evidence_metadata", MIGRATION_043_UP, true)?;` after migration 042.

Add rollback entry near existing down migrations:

```rust
rollback_migration(conn, "043_chunk_evidence_metadata", MIGRATION_043_DOWN);
```

- [ ] **Step 3: Add migration test**

Add to `migrate.rs` tests:

```rust
#[test]
fn migration_043_adds_chunk_evidence_metadata() {
    let conn = Connection::open_in_memory().unwrap();
    migrate_up(&conn).unwrap();
    let cols = table_columns(&conn, "chunks");
    for name in ["heading_path", "source_start", "source_end", "content_hash"] {
        assert!(cols.contains(&name.to_string()), "missing {name}");
    }
}
```

Add this local helper in the same test module and use it from the migration 043 test:

```rust
fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("pragma table_info");
    stmt.query_map([], |row| row.get::<_, String>(1))
        .expect("query columns")
        .flatten()
        .collect()
}
```

- [ ] **Step 4: Verify migration**

Run: `cargo test --manifest-path src-tauri/Cargo.toml migration_043 -- --nocapture`

Expected: pass.

---

## Task 6: P1 chunker v2 metadata without breaking old callers

**Files:**

- Modify: `src-tauri/src/indexer/chunker.rs`
- Modify: `src-tauri/src/indexer/scan.rs`
- Test: unit tests in `chunker.rs`

- [ ] **Step 1: Add metadata struct and tests**

In `chunker.rs`, add above `chunk_markdown`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownChunk {
    pub content: String,
    pub heading_path: Option<String>,
    pub source_start: usize,
    pub source_end: usize,
    pub char_count: usize,
}
```

Add tests:

```rust
#[test]
fn chunk_markdown_with_metadata_tracks_heading_path() {
    let md = "# Alpha\n\nFirst paragraph.\n\n## Beta\n\nSecond paragraph.";
    let chunks = chunk_markdown_with_metadata(md, 512);
    assert!(chunks.iter().any(|chunk| {
        chunk.heading_path.as_deref() == Some("Alpha > Beta")
            && chunk.content.contains("Second paragraph")
    }));
}

#[test]
fn chunk_markdown_with_metadata_tracks_source_span() {
    let md = "# Title\n\nBody text.";
    let chunks = chunk_markdown_with_metadata(md, 512);
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    assert_eq!(&md[chunk.source_start..chunk.source_end], chunk.content);
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml chunk_markdown_with_metadata -- --nocapture`

Expected: fail because function is not implemented.

- [ ] **Step 2: Implement metadata chunker conservatively**

Add:

```rust
pub fn chunk_markdown_with_metadata(content: &str, max_chars: usize) -> Vec<MarkdownChunk> {
    let mut chunks = Vec::new();
    let mut heading_stack: Vec<(usize, String)> = Vec::new();
    let mut current = String::new();
    let mut current_start: Option<usize> = None;
    let mut current_heading: Option<String> = None;
    let max_chars = max_chars.max(1);
    const MIN_CHARS: usize = 100;

    for (line_start, line) in line_ranges(content) {
        if let Some((level, title)) = parse_heading(line) {
            heading_stack.retain(|(existing_level, _)| *existing_level < level);
            heading_stack.push((level, title));
        }

        let is_boundary = line.starts_with('#') || line.trim().is_empty();
        if is_boundary && !current.is_empty() && current.chars().count() >= MIN_CHARS {
            push_metadata_chunk(&mut chunks, content, &current, current_start, line_start, current_heading.clone());
            current.clear();
            current_start = None;
            current_heading = None;
        }

        if !line.is_empty() || !current.is_empty() {
            if current_start.is_none() {
                current_start = Some(line_start);
                current_heading = heading_path(&heading_stack);
            }
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }

        while current.chars().count() > max_chars {
            let split_at = byte_index_after_chars(&current, max_chars);
            let head = current[..split_at].trim().to_string();
            if !head.is_empty() {
                let start = current_start.unwrap_or(line_start);
                let end = start + head.len();
                chunks.push(MarkdownChunk {
                    char_count: head.chars().count(),
                    content: head,
                    heading_path: current_heading.clone(),
                    source_start: start,
                    source_end: end.min(content.len()),
                });
            }
            current = current[split_at..].trim_start().to_string();
            current_start = Some(line_start.saturating_sub(current.len()));
        }
    }

    push_metadata_chunk(&mut chunks, content, &current, current_start, content.len(), current_heading);
    chunks
}
```

Add helper functions with deterministic behavior:

```rust
fn line_ranges(content: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for line in content.lines() {
        out.push((offset, line));
        offset += line.len() + 1;
    }
    out
}

fn parse_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = trimmed[hashes..].trim();
    if rest.is_empty() {
        None
    } else {
        Some((hashes, rest.to_string()))
    }
}

fn heading_path(stack: &[(usize, String)]) -> Option<String> {
    if stack.is_empty() {
        None
    } else {
        Some(stack.iter().map(|(_, title)| title.as_str()).collect::<Vec<_>>().join(" > "))
    }
}

fn push_metadata_chunk(
    chunks: &mut Vec<MarkdownChunk>,
    content: &str,
    text: &str,
    start: Option<usize>,
    fallback_end: usize,
    heading: Option<String>,
) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let source_start = start
        .and_then(|s| content[s..].find(trimmed).map(|delta| s + delta))
        .unwrap_or_else(|| content.find(trimmed).unwrap_or(0));
    let source_end = (source_start + trimmed.len()).min(fallback_end.max(source_start));
    chunks.push(MarkdownChunk {
        content: trimmed.to_string(),
        heading_path: heading,
        source_start,
        source_end,
        char_count: trimmed.chars().count(),
    });
}
```

Keep existing `chunk_markdown(content, max_chars) -> Vec<String>` by mapping `chunk_markdown_with_metadata` to content, so old callers keep compiling:

```rust
pub fn chunk_markdown(content: &str, max_chars: usize) -> Vec<String> {
    chunk_markdown_with_metadata(content, max_chars)
        .into_iter()
        .map(|chunk| chunk.content)
        .collect()
}
```

- [ ] **Step 3: Update indexer to write metadata**

In both chunk insert locations in `scan.rs`, replace `chunk_markdown` use with:

```rust
let chunks = chunk_markdown_with_metadata(&parsed.body, 2000);
for (idx, chunk) in chunks.iter().enumerate() {
    tx.execute(
        "INSERT INTO chunks
            (file_id, chunk_index, content, char_count, heading_path, source_start, source_end, content_hash, embedding_model)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            file_id,
            idx as i64,
            chunk.content,
            chunk.char_count as i64,
            chunk.heading_path,
            chunk.source_start as i64,
            chunk.source_end as i64,
            content_hash(&chunk.content),
            crate::knowledge::EMBEDDING_MODEL,
        ],
    )?;
}
```

Update import:

```rust
use super::chunker::chunk_markdown_with_metadata;
```

- [ ] **Step 4: Run chunker and indexer tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chunker -- --nocapture`

Run: `cargo test --manifest-path src-tauri/Cargo.toml index_file_creates_files_and_chunks -- --nocapture`

Expected: pass.

---

## Task 7: P1 ContextPacket uses span/hash metadata when available

**Files:**

- Modify: `src-tauri/src/ai_runtime/retrieval_broker/vector.rs`
- Modify: `src-tauri/src/ai_runtime/retrieval_broker/fts.rs`
- Test: `src-tauri/tests/rag_retrieval_contract.rs`

- [ ] **Step 1: Add contract test for metadata-bearing packet**

Append:

```rust
#[test]
fn vector_chunk_packet_can_carry_heading_span_and_hash_when_indexed() {
    let dir = tempdir().expect("tempdir");
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).expect("vault dir");
    let note = vault.join("meta.md");
    fs::write(&note, "# Parent\n\n## Child\n\nEvidence body for metadata.").expect("write");

    let conn = setup_conn();
    index_file_with_embed(&conn, &vault, &note, IndexEmbeddingMode::Skip).expect("index");

    let (heading, source_start, source_end, hash): (Option<String>, Option<i64>, Option<i64>, Option<String>) =
        conn.query_row(
            "SELECT heading_path, source_start, source_end, content_hash FROM chunks WHERE file_id = 1 LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("chunk metadata");

    assert!(heading.is_some());
    assert!(source_start.unwrap_or_default() < source_end.unwrap_or_default());
    assert!(hash.is_some());
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test rag_retrieval_contract vector_chunk_packet_can_carry_heading_span_and_hash_when_indexed -- --nocapture`

Expected: pass after Task 6.

- [ ] **Step 2: Update vector SELECT**

In `vector.rs`, select:

```rust
c.content, f.path, f.title, c.heading_path, c.char_count,
c.source_start, c.source_end, c.content_hash, vc.distance
```

Set packet fields:

```rust
source_span: match (source_start, source_end) {
    (Some(start), Some(end)) => Some(crate::ai_runtime::SourceSpan {
        start: start as usize,
        end: end as usize,
    }),
    _ => None,
},
content_hash: content_hash.unwrap_or_default(),
```

- [ ] **Step 3: Keep FTS packets stable**

FTS currently returns file-level snippets, not chunk rows. Keep `source_span: None` and `content_hash: String::new()` for FTS packets in this plan. This avoids overclaiming precision.

- [ ] **Step 4: Verify packet tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test rag_retrieval_contract -- --nocapture`

Expected: pass.

---

## Task 8: P1 ranking v2 without new dependency

**Files:**

- Modify: `src-tauri/src/ai_runtime/retrieval_broker/rank.rs`

- [ ] **Step 1: Add ranking tests**

Add to the existing `#[cfg(test)] mod tests` in `rank.rs`:

```rust
fn packet_with_path(id: &str, path: &str, reason: &str, score: f64) -> ContextPacket {
    ContextPacket {
        id: id.to_string(),
        source_type: SourceType::Note,
        source_path: Some(path.to_string()),
        title: path.to_string(),
        heading_path: None,
        source_span: None,
        content_hash: id.to_string(),
        excerpt: format!("evidence {id}"),
        retrieval_reason: reason.to_string(),
        score,
        trust_level: TrustLevel::UserNote,
        citation_label: String::new(),
        stale: false,
        web: None,
        corpus: None,
    }
}

#[test]
fn ranking_keeps_diverse_files_when_scores_are_close() {
    let mut packets = vec![
        packet_with_path("a1", "a.md", "vector_chunk", 0.91),
        packet_with_path("a2", "a.md", "vector_chunk", 0.90),
        packet_with_path("b1", "b.md", "fts_keyword_match", 0.88),
    ];
    fuse_and_rank(&mut packets, 2);
    let paths: Vec<_> = packets.iter().map(|p| p.source_path.as_deref().unwrap()).collect();
    assert_eq!(paths, vec!["a.md", "b.md"]);
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml ranking_keeps_diverse_files_when_scores_are_close -- --nocapture`

Expected: fail until test support and ranking v2 are implemented.

- [ ] **Step 2: Implement file diversity**

In `rank.rs`, after initial weighted sort, add a small diversity pass:

```rust
fn apply_file_diversity(packets: &mut Vec<ContextPacket>) {
    let mut seen_paths = std::collections::HashSet::new();
    for packet in packets.iter_mut() {
        if let Some(path) = packet.source_path.as_deref() {
            if !seen_paths.insert(path.to_string()) {
                packet.score *= 0.92;
            }
        }
    }
    packets.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}
```

Call it before dedup/truncate.

- [ ] **Step 3: Verify ranking**

Run: `cargo test --manifest-path src-tauri/Cargo.toml ranking_keeps_diverse_files_when_scores_are_close -- --nocapture`

Expected: pass.

---

## Task 9: P1 evaluation metrics expansion

**Files:**

- Modify: `src-tauri/tests/semantic_recall_eval.rs`
- Modify: `docs/eval/semantic-search.md`

- [ ] **Step 1: Add metric helpers**

In `semantic_recall_eval.rs`, add:

```rust
fn reciprocal_rank(hits: &[SemanticHit], expected: &str) -> f32 {
    hits.iter()
        .position(|hit| hit.path == expected)
        .map(|idx| 1.0 / (idx as f32 + 1.0))
        .unwrap_or(0.0)
}

fn recall_at(hits: &[SemanticHit], expected: &str, k: usize) -> f32 {
    if recall_at_k(hits, expected, k) {
        1.0
    } else {
        0.0
    }
}
```

- [ ] **Step 2: Print Recall@10 and MRR**

Inside the ignored eval loop, request 10 hits and accumulate:

```rust
let hits = semantic_search(&conn, query, 10).expect("semantic_search");
recall5_total += recall_at(&hits, expected, 5);
recall10_total += recall_at(&hits, expected, 10);
mrr_total += reciprocal_rank(&hits, expected);
```

Print:

```rust
eprintln!("Recall@5 = {:.3}", recall5_total / EVAL_QUERIES.len() as f32);
eprintln!("Recall@10 = {:.3}", recall10_total / EVAL_QUERIES.len() as f32);
eprintln!("MRR = {:.3}", mrr_total / EVAL_QUERIES.len() as f32);
```

- [ ] **Step 3: Add no-answer query list**

Add:

```rust
const NO_ANSWER_QUERIES: &[&str] = &[
    "火星基地农业产量统计",
    "Iris 是否支持多人 CRDT 云协作",
    "不存在的内部项目代号 ZetaPine",
];
```

For no-answer evaluation, print the top hit path and score for each query and report a non-gating `no_answer_low_confidence_rate`. Do not hard fail no-answer cases in this implementation because current score calibration is not stable enough.

- [ ] **Step 4: Update eval doc**

In `docs/eval/semantic-search.md`, add the new metrics definitions and clarify that no-answer accuracy is reported before it becomes a merge gate.

- [ ] **Step 5: Verify ignored eval still compiles**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test semantic_recall_eval -- --ignored --no-run`

Expected: compile pass without downloading model.

---

## Task 10: P2 research coverage state

**Files:**

- Modify: `src-tauri/src/ai_workflows/research_workflow.rs`
- Test: unit tests in `research_workflow.rs`

- [ ] **Step 1: Add coverage structs**

In `research_workflow.rs`, add serializable structs near existing research result types:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct EvidenceCoverageItem {
    pub question: String,
    pub supporting_packet_ids: Vec<String>,
    pub conflicting_packet_ids: Vec<String>,
    pub missing_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct EvidenceCoverageMatrix {
    pub items: Vec<EvidenceCoverageItem>,
}
```

- [ ] **Step 2: Add pure coverage builder test**

Add:

```rust
#[test]
fn coverage_marks_missing_question_without_packets() {
    let matrix = build_evidence_coverage_matrix(
        &["本地 RAG 是否支持 citation".to_string(), "不存在的问题".to_string()],
        &[ContextPacket {
            id: "p1".into(),
            source_type: SourceType::Note,
            source_path: Some("a.md".into()),
            title: "a".into(),
            heading_path: None,
            source_span: None,
            content_hash: String::new(),
            excerpt: "citation label and evidence ledger".into(),
            retrieval_reason: "fts_keyword_match".into(),
            score: 0.9,
            trust_level: TrustLevel::UserNote,
            citation_label: "[C1]".into(),
            stale: false,
            web: None,
            corpus: None,
        }],
    );
    assert_eq!(matrix.items.len(), 2);
    assert!(matrix.items[0].missing_reason.is_none());
    assert!(matrix.items[1].missing_reason.is_some());
}
```

- [ ] **Step 3: Implement simple coverage builder**

Add:

```rust
fn build_evidence_coverage_matrix(
    questions: &[String],
    packets: &[ContextPacket],
) -> EvidenceCoverageMatrix {
    let items = questions
        .iter()
        .map(|question| {
            let terms: Vec<String> = question
                .split_whitespace()
                .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
                .filter(|s| s.chars().count() >= 2)
                .collect();
            let supporting_packet_ids: Vec<String> = packets
                .iter()
                .filter(|packet| {
                    let excerpt = packet.excerpt.to_lowercase();
                    terms.iter().any(|term| excerpt.contains(term))
                })
                .map(|packet| packet.id.clone())
                .collect();
            EvidenceCoverageItem {
                question: question.clone(),
                missing_reason: if supporting_packet_ids.is_empty() {
                    Some("没有检索到可直接支持该子问题的本地证据".to_string())
                } else {
                    None
                },
                supporting_packet_ids,
                conflicting_packet_ids: Vec::new(),
            }
        })
        .collect();
    EvidenceCoverageMatrix { items }
}
```

This is intentionally conservative. A future LLM-assisted claim extraction task can improve it without changing the output contract.

- [ ] **Step 4: Wire matrix into research result**

Add an optional `coverage_matrix: Option<EvidenceCoverageMatrix>` to the relevant research result struct. Populate it after proposition/subquestion retrieval using accumulated evidence.

- [ ] **Step 5: Verify research tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml research_workflow -- --nocapture`

Expected: pass.

---

## Task 11: P2 UI keeps diagnostics useful but not stiff

**Files:**

- Modify: `src/components/ai/ContextPacketDrawer.tsx`
- Modify: `src/components/ai/EvidenceChainView.tsx`
- Test: existing frontend tests or new focused Vitest contract

- [ ] **Step 1: Add frontend contract test**

Create or extend `tests/context-packet-drawer.test.tsx`:

```ts
it("shows concise evidence diagnostics without forcing a workflow", () => {
  const text = read("src/components/ai/ContextPacketDrawer.tsx");
  expect(text).toContain("diagnostic");
  expect(text).not.toContain("必须先完成");
});
```

Use the existing source-contract style and add this helper at the top of the test file:

```ts
import { readFileSync } from "node:fs";

const read = (path: string): string => readFileSync(path, "utf8");
```

- [ ] **Step 2: Add compact diagnostic rendering**

In `ContextPacketDrawer.tsx`, render diagnostics as small secondary metadata near the title or evidence group:

```tsx
{
  diagnostics.length > 0 ? (
    <div className="text-xs text-muted-foreground">
      {diagnosticsSummary(diagnostics)}
    </div>
  ) : null;
}
```

Use short labels such as:

- `语义索引未就绪，已使用其他证据`
- `未找到本地证据`
- `部分检索层降级`

Do not add modal dialogs or blocking confirmation.

- [ ] **Step 3: Keep natural answer style**

In `EvidenceChainView.tsx`, display coverage/conflict details only when data exists and the user opens evidence details. Do not inject mandatory tables into normal chat messages.

- [ ] **Step 4: Run frontend tests**

Run: `npm run test -- context-packet-drawer`

Expected: pass.

---

## Task 12: Full verification and documentation

**Files:**

- Modify: `docs/eval/semantic-search.md`
- No source changes beyond prior tasks

- [ ] **Step 1: Update semantic search documentation**

Document:

- current embedding model and sqlite-vec optional status;
- chunk v2 evidence metadata;
- diagnostics statuses;
- metrics printed by ignored eval;
- no-answer accuracy as reported metric.

- [ ] **Step 2: Run Rust focused gates**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml --test rag_retrieval_contract -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml ranking_keeps_diverse_files_when_scores_are_close -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml migration_043 -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test semantic_recall_eval -- --ignored --no-run
```

Expected: all pass or compile pass for ignored eval.

- [ ] **Step 3: Run frontend focused gates**

Run:

```bash
npm run test -- context-packet-drawer
npm run typecheck
```

Expected: pass.

- [ ] **Step 4: Run full quality gates before claiming completion**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run lint
npm run format:check
npm run typecheck
npm run test
```

Expected: pass. Do not run the fastembed ignored eval as part of this gate; run it only when explicitly validating retrieval quality with model download allowed.

- [ ] **Step 5: Commit in reviewable slices**

Suggested commit sequence:

```bash
git add src-tauri/tests/rag_retrieval_contract.rs src-tauri/src/embedding/engine.rs src-tauri/src/ai_runtime/retrieval_broker
git commit -m "fix(search): 修复 RAG 检索断链与诊断"

git add src-tauri/migrations/043_chunk_evidence_metadata.sql src-tauri/migrations/043_chunk_evidence_metadata.down.sql src-tauri/src/storage/migrate.rs src-tauri/src/indexer
git commit -m "feat(search): 添加 chunk 证据元数据"

git add src-tauri/src/ai_runtime/retrieval_broker/rank.rs src-tauri/tests/semantic_recall_eval.rs tests/rust-runtime-refactor-contract.test.ts docs/eval/semantic-search.md
git commit -m "feat(search): 提升 RAG 排序与评测"

git add src-tauri/src/ai_workflows/research_workflow.rs src/components/ai/ContextPacketDrawer.tsx src/components/ai/EvidenceChainView.tsx tests/context-packet-drawer.test.tsx
git commit -m "feat(ai): 添加研究证据覆盖反馈"
```

Do not commit unrelated user changes.

---

## Implementation Notes

- P0 should be implemented first and can ship independently.
- P1 migration is additive. Existing vaults can keep working before reindex; full metadata appears after `search_reindex` or normal incremental indexing.
- P2 should not block normal chat. Coverage diagnostics are supporting context, not a rigid answer template.
- Any new reranker dependency is outside this first implementation plan. A future reranker plan must first document AGPL compatibility, platform support, model size, and fallback behavior.
