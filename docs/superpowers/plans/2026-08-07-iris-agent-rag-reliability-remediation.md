# Iris Agent / RAG 可靠性修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不改变 Iris 本地优先、Markdown 权威、显式授权与确认写入边界的前提下，修复 Agent/RAG 已确认的正确性和可用性问题，并把检索与 Agent 评测升级为可发布的质量门禁。

**Architecture:** `.md` 文件仍是唯一笔记真相，SQLite 只存派生索引与应用状态。`links` 是唯一的笔记链接事实源；`chunk_embeddings_v2`、`semantic_anchor_embeddings_v2` 和 `regulation_embeddings_v2` 分别是三类 embedding cache 的权威来源，sqlite-vec 仅由数据库触发器派生为可检索物化索引。检索先执行强范围过滤，再在 sqlite-vec KNN、FTS、metadata 与图层之间融合；向量不可用时明确报告状态并退化到 FTS，绝不返回假空集或在 Rust 中无界扫描全库。

**Tech Stack:** Tauri 2、Rust、SQLite、sqlite-vec 0.1.9、React 19、TypeScript、Vitest、Cargo test、现有 SQLite migration runner。

## Global Constraints

- 用户 `.md` 是笔记内容的唯一权威来源；任何索引数据均可由 Vault 重建，任何写入仍须用户确认。
- 只使用 SQLite + sqlite-vec；不新增远程向量数据库、重排序服务或长期记忆。
- 所有 LLM/MCP API 保持 HTTPS；不得为了 Ollama 或测试放宽为 HTTP。
- `links` 是图关系唯一真相；新代码不得读取或写入 `block_links`。
- sqlite-vec 是全平台默认功能；其 Rust 注册若含 `unsafe`，必须有安全替代不可行的代码注释、PR 的“含 unsafe 代码”说明和 maintainer 专门审查。
- SQLite schema 变更必须有递增 up/down migration；不要求用户删除数据库重建。
- IPC 改动必须同步 Rust command、`src/types/ai.ts`、`src/types/ipc.ts`、`src/lib/ipc.ts`、文档和测试。
- 任何新增行为先写并实际运行失败测试；每个任务独立提交，提交信息使用中文 Conventional Commits。
- 状态字段不得以“空数组”暗示成功：调用方必须获得 `ready | partial | indexing | unavailable | disabled` 的明确语义。

## 阶段 0 冻结结论

基线文件 [v1.2.18-agent-rag-stage0-baseline.json](../../eval/results/v1.2.18-agent-rag-stage0-baseline.json) 固定了 `478ba3cd` 的事实：FTS-only RAG 质量门禁通过，但 `npm run agent:eval:smoke` 的加密凭据子进程在 24 个闭环用例中仅完成 12 个，case 25/36/37/48 每次重复均失败。该失败是后续发布阻断，不得降低到 12/24 或把 `agent_run_incomplete_output`、`agent_run_web_provider_failed` 当作通过。

本计划也固定以下待修复事实：图检索读 `block_links` 而索引器写 `links`；`MAX_COSINE_FALLBACK_CHUNKS=8000` 以上返回空向量集；全局 generation ready 会把已有向量变为不可用；chunker 会将短小前序内容并入后一个 heading；当前打包/CI 未证明全平台 sqlite-vec 检索。阶段 0 已校正 normal/classified Run 的文档事实：normal Run 可持久化回放；涉密 Run 仅在进程内易失执行，`assistant_run_get` 只可按显式 ID 读取无正文状态/安全事件，正文仍一次性取走。RAG 的功能指标可重复验证，`warmP95Milliseconds` 只记录该次单机测量值，不作为跨机器复现目标；Task 6 才定义声明参考机器后的发布性能阈值。

---

### Task 1: Markdown 分块与链接事实源

**Files:**

- Modify: `src-tauri/src/indexer/chunker.rs`
- Modify: `src-tauri/src/ai_runtime/retrieval_broker/graph.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/read.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch_impl.rs`
- Modify: `src-tauri/src/ai_runtime/agent_permissions.rs`
- Test: `src-tauri/src/indexer/chunker.rs`
- Test: `src-tauri/src/ai_runtime/retrieval_broker/graph.rs`

**Interfaces:**

- Consumes: `links(source_file_id, target_file_id, link_type)` written by the existing Markdown indexer.
- Produces: `chunk_markdown_with_metadata(content, max_chars) -> Vec<MarkdownChunk>` with stable source spans and heading ancestry; graph packets sourced from `links`.

- [ ] **Step 1: Write failing chunker tests**

```rust
#[test]
fn heading_flushes_a_short_preamble_before_updating_heading_path() {
    let chunks = chunk_markdown_with_metadata("brief\n# Next\nbody", 512);
    assert_eq!(chunks[0].content, "brief");
    assert_eq!(chunks[0].heading_path, None);
    assert_eq!(chunks[1].heading_path.as_deref(), Some("Next"));
}

#[test]
fn setext_heading_starts_a_new_chunk_outside_fences() {
    let chunks = chunk_markdown_with_metadata("Intro\n=====\n\nBody", 512);
    assert_eq!(chunks[0].heading_path.as_deref(), Some("Intro"));
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml indexer::chunker::tests -- --nocapture`

Expected: the preamble test fails because the old code assigns `Next`; the Setext test fails because it has no heading parser.

- [ ] **Step 3: Implement structural boundaries**

```rust
// Before mutating heading_stack, flush any non-empty current chunk.
// Recognize ATX headings with up to three leading spaces and Setext underlines.
// Never parse either form while FenceState reports an active fence.
// Preserve exact byte spans; only the stored text is trimmed.
```

Use `target_chars = 320`, `hard_max_chars = 384`, and `overlap_chars = 48` in the new internal chunk policy. Do not duplicate heading text solely to manufacture overlap, and preserve a short preamble as its own chunk.

- [ ] **Step 4: Write the graph source-of-truth failure test**

```rust
#[test]
fn graph_neighbors_read_confirmed_links_written_by_the_markdown_indexer() {
    // Seed files, chunks and links only; intentionally do not create block_links.
    let packets = search_graph_neighbors(&conn, 1, 3).expect("graph retrieval");
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].source_path.as_deref(), Some("target.md"));
}
```

- [ ] **Step 5: Run the graph test and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml retrieval_broker::graph::tests -- --nocapture`

Expected: failure with `no such table: block_links` or no packet.

- [ ] **Step 6: Implement links-only graph retrieval and remove the dead tool surface**

Replace the `block_links` query with `links`, keep `chunks` as the only evidence text source, remove `get_block_links` from the catalog/dispatch/permission/display allowlists, and update exact catalog parity tests.

- [ ] **Step 7: Run focused verification**

Run: `cargo test --manifest-path src-tauri/Cargo.toml indexer::chunker::tests retrieval_broker::graph::tests tool_catalog -- --nocapture`

Expected: PASS; no production reference to `get_block_links` or `block_links` except migration compatibility tests.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/indexer/chunker.rs src-tauri/src/ai_runtime/retrieval_broker/graph.rs src-tauri/src/ai_runtime/tool_catalog/read.rs src-tauri/src/ai_runtime/tool_dispatch_impl.rs src-tauri/src/ai_runtime/agent_permissions.rs
git commit -m "fix(search): 修复分块边界并统一链接事实源"
```

### Task 2: sqlite-vec 默认检索与可回滚索引迁移

**Files:**

- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/storage/db.rs`
- Modify: `src-tauri/src/storage/migrate.rs`
- Create: `src-tauri/migrations/061_sqlite_vec_v3.sql`
- Create: `src-tauri/migrations/061_sqlite_vec_v3.down.sql`
- Modify: `src-tauri/src/embedding/engine.rs`
- Test: `src-tauri/src/storage/migrate.rs`
- Test: `src-tauri/src/embedding/engine.rs`

**Interfaces:**

- Consumes: `chunk_embeddings_v2(chunk_id, embedding, model_id, dimension, source_fingerprint)`、`semantic_anchor_embeddings_v2(anchor_id, embedding, model_id, dimension, source_fingerprint)` 和 `regulation_embeddings_v2(regulation_id, embedding, model_id, dimension, source_fingerprint)`.
- Produces: sqlite-vec v3 tables with `float[512] distance_metric=cosine`: `vec_chunks_v3` 包含 `file_id` 元数据，`vec_anchors_v3` 与 `vec_regulations_v3` 分别以 anchor/regulation ID 为标识；三张表均只由对应 canonical cache 的 SQL insert/update/delete triggers 派生。

- [ ] **Step 1: Write failing migration tests**

```rust
#[test]
fn sqlite_vec_v3_mirrors_all_canonical_caches_insert_update_and_delete() {
    migrate_up(&conn).unwrap();
    assert_cache_mirror(&conn, CacheKind::Chunk, 7, "hash-a", "hash-b");
    assert_cache_mirror(&conn, CacheKind::Anchor, 8, "hash-a", "hash-b");
    assert_cache_mirror(&conn, CacheKind::Regulation, 9, "hash-a", "hash-b");
}

#[test]
fn sqlite_vec_v3_down_restores_the_pre_migration_schema() {
    migrate_up(&conn).unwrap();
    migrate_down(&conn, "061_sqlite_vec_v3").unwrap();
    assert!(!table_exists(&conn, "vec_chunks_v3"));
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage::migrate::tests::sqlite_vec_v3 -- --nocapture`

Expected: migration identifier and v3 tables do not yet exist.

- [ ] **Step 3: Implement migration and dependency policy**

Pin `sqlite-vec` to `=0.1.9`, enable it by default on macOS, Windows and Linux, and define `vec_chunks_v3`, `vec_anchors_v3`, and `vec_regulations_v3` with `float[512] distance_metric=cosine`. For each of `chunk_embeddings_v2`、`semantic_anchor_embeddings_v2`、`regulation_embeddings_v2`, write a complete SQL trigger trio for insert/update/delete into its corresponding v3 table; do not add a Rust dual-write path. Preserve all three v2 caches as the canonical rebuild sources. If runtime extension loading fails, leave vector status unavailable and retain FTS; release packaging must fail before distribution if the default extension cannot load.

- [ ] **Step 4: Write a failing KNN test**

```rust
#[test]
fn semantic_search_uses_sqlite_vec_knn_and_never_skips_a_large_index() {
    // Seed 8_001 valid cache/vector rows and one nearest match.
    let response = semantic_search(&conn, "needle", 5).unwrap();
    assert!(response.iter().any(|hit| hit.path == "needle.md"));
}
```

- [ ] **Step 5: Verify RED**

Run: `cargo test --features sqlite-vec --manifest-path src-tauri/Cargo.toml embedding::engine::tests::semantic_search_uses_sqlite_vec_knn_and_never_skips_a_large_index -- --nocapture`

Expected: old `MAX_COSINE_FALLBACK_CHUNKS` returns an empty result.

- [ ] **Step 6: Implement bounded KNN search**

Query sqlite-vec with the embedded query vector, `k = max(max_results * 4, 32)`, hard scope as SQL metadata filtering, current cache model/dimension/fingerprint joins, and `score = clamp(1.0 - distance, 0.0, 1.0)`. Delete `MAX_COSINE_FALLBACK_CHUNKS` and the Rust full-table cosine fallback; do not apply an arbitrary similarity cutoff.

- [ ] **Step 7: Verify both build modes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml embedding::engine::tests -- --nocapture && cargo test --features sqlite-vec --manifest-path src-tauri/Cargo.toml embedding::engine::tests -- --nocapture`

Expected: non-vec build reports unavailable explicitly; vec build returns scoped KNN hits above 8,000 rows.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/storage src-tauri/src/embedding src-tauri/migrations/061_sqlite_vec_v3.sql src-tauri/migrations/061_sqlite_vec_v3.down.sql
git commit -m "feat(search): 默认启用 sqlite-vec 并建立 v3 索引"
```

### Task 3: 部分可用 embedding 状态与前后端契约

**Files:**

- Modify: `src-tauri/src/embedding/engine.rs`
- Modify: `src-tauri/src/embedding/scheduler.rs`
- Modify: `src-tauri/src/commands/search.rs`
- Modify: `src-tauri/src/commands/assistant_commands.rs`
- Modify: `src/types/ai.ts`
- Modify: `src/types/ipc.ts`
- Modify: `src/lib/ipc.ts`
- Test: `src-tauri/src/embedding/scheduler.rs`
- Test: `tests/assistant-run-ipc.test.ts`

**Interfaces:**

- Produces: `SemanticSearchResponse { hits, status, backend, indexedCount, totalCount }`, where status is exactly `ready | partial | indexing | unavailable | disabled` and backend is `sqlite_vec | null`.
- Produces: `EmbeddingIndexStatus.searchAvailable`, `backend`, and coverage counts; retrieval diagnostics preserve `partial`.

- [ ] **Step 1: Write failing status tests**

```rust
#[test]
fn changed_file_leaves_existing_generation_searchable_as_partial() {
    insert_current_v2_embedding(&conn, "current.md", "current-hash");
    insert_stale_v2_embedding(&conn, "stale.md", "old-hash");
    let response = semantic_search_with_status(&conn, "current", 5).unwrap();
    assert_eq!(response.status, SemanticSearchStatus::Partial);
    assert_eq!(response.indexed_count, 1);
    assert_eq!(response.total_count, 2);
    assert!(response.hits.iter().any(|hit| hit.path == "current.md"));
}
```

```ts
expect(searchSemanticResult).toMatchObject({
  status: "partial",
  backend: "sqlite_vec",
  indexedCount: 9,
  totalCount: 10,
});
```

- [ ] **Step 2: Verify RED**

Run: `cargo test --features sqlite-vec --manifest-path src-tauri/Cargo.toml embedding::scheduler::tests::changed_file_leaves_existing_generation_searchable_as_partial -- --nocapture && npm run test -- tests/assistant-run-ipc.test.ts`

Expected: current `embedding_generation_ready` blocks all vector hits and TypeScript has no response state.

- [ ] **Step 3: Implement partial coverage semantics**

Replace global all-or-nothing readiness in the search path with current-row coverage. Keep scheduler progress truthful; do not claim ready before coverage is complete. Return FTS-only diagnostics when vec is unavailable and never substitute a hidden Rust scan.

- [ ] **Step 4: Verify contract parity**

Run: `cargo test --features sqlite-vec --manifest-path src-tauri/Cargo.toml embedding::scheduler::tests -- --nocapture && npm run typecheck && npm run test -- tests/assistant-run-ipc.test.ts`

Expected: all statuses serialize identically across Rust and TypeScript.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/embedding src-tauri/src/commands/search.rs src-tauri/src/commands/assistant_commands.rs src/types/ai.ts src/types/ipc.ts src/lib/ipc.ts tests/assistant-run-ipc.test.ts
git commit -m "fix(search): 暴露部分可用的嵌入检索状态"
```

### Task 4: Agent 执行预算、历史和真实归因边界

**Files:**

- Modify: `src-tauri/src/ai_runtime/agent_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/run_contract.rs`
- Modify: `src-tauri/src/ai_runtime/run_engine/providers.rs`
- Modify: `src-tauri/src/ai_runtime/guardrails.rs`
- Modify: `src-tauri/src/ai_runtime/retrieval_broker/rank.rs`
- Test: `src-tauri/src/ai_runtime/agent_tool_loop_tests.rs`
- Test: `src-tauri/src/ai_runtime/run_engine_tests.rs`

**Interfaces:**

- Produces: frozen budget profiles Direct `64k/8k/8k`, Standard `128k/16k/4k`, Delegated parent `96k/12k/4k`, Durable `128k/16k/4k` (prompt/completion/turn output); ChildRun remains max `2` model turns and `6` calls with `2k/1024` per turn.
- Produces: `source_binding` versus `claim_support` wording. Source groups only communicate source binding for uncalibrated routes.

- [ ] **Step 1: Write failing budget tests**

```rust
#[test]
fn standard_turn_reserves_output_before_selecting_history() {
    let budget = RunBudgetPolicy::standard();
    assert_eq!(budget.max_prompt_tokens, 128_000);
    assert_eq!(budget.max_completion_tokens, 16_000);
    assert_eq!(budget.max_turn_output_tokens, 4_000);
}

#[test]
fn direct_provider_has_an_explicit_nonzero_turn_budget() {
    assert_ne!(AgentModelTurnBudget::default().max_prompt_tokens, None);
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml agent_tool_loop_tests::standard_turn_reserves_output_before_selecting_history agent_tool_loop_tests::direct_provider_has_an_explicit_nonzero_turn_budget -- --nocapture`

Expected: parent defaults are unbounded or budget selection does not reserve output.

- [ ] **Step 3: Implement frozen budgets and bounded history**

Pass the chosen `AgentModelTurnBudget` to every provider call. When usage is missing, estimate locally; select at most 24 history candidates, retain the newest coherent user/assistant pair, and fit at most 12 pairs within 8k tokens. Do not add LLM-generated long-term memory.

- [ ] **Step 4: Remove only proven-dead indirection**

After searching call sites, delete unused `sanitize_query`, `verify_citations`, `filter_by_trust` and `NoopCandidateReranker` only when production behavior is covered by direct PromptContract/tool-pipeline tests. Preserve `verify_tool_args`. Rename docs/tests so source binding is never advertised as semantic claim support.

- [ ] **Step 5: Verify Agent closure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml agent_tool_loop_tests run_engine_tests -- --nocapture && npm run agent:eval:smoke`

Expected: all 24 encrypted-credential local-transport cases complete and pass; no test weakens the count or accepts incomplete/web-provider failures.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/ai_runtime/agent_tool_loop.rs src-tauri/src/ai_runtime/run_contract.rs src-tauri/src/ai_runtime/run_engine/providers.rs src-tauri/src/ai_runtime/guardrails.rs src-tauri/src/ai_runtime/retrieval_broker/rank.rs src-tauri/src/ai_runtime/agent_tool_loop_tests.rs src-tauri/src/ai_runtime/run_engine_tests.rs
git commit -m "fix(ai): 冻结模型预算并收紧归因语义"
```

### Task 5: 清理旧存储、打包和文档事实

**Files:**

- Create: `src-tauri/migrations/062_remove_legacy_search_graph.sql`
- Create: `src-tauri/migrations/062_remove_legacy_search_graph.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`
- Modify: `src-tauri/src/llm/search_web.rs`
- Modify: `src-tauri/src/app.rs`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/package-desktop.yml`
- Modify: `docs/eval/semantic-search.md`
- Modify: `docs/eval/rag-v2-broker-evaluation.md`
- Modify: `SECURITY.md`
- Modify: `CONTRIBUTING.md`
- Test: `tests/runtime-contracts.test.ts`
- Test: `tests/github-actions-workflows.test.ts`

**Interfaces:**

- Produces: a reversible migration that removes `block_links` and `search_cache`; one release only may retain a read-only compatibility view, and new code must not use it.
- Produces: vector RAG CI and release checks on all desktop platforms.

- [ ] **Step 1: Write failing migration and workflow tests**

```rust
#[test]
fn legacy_graph_and_search_cache_tables_are_absent_after_migration_062() {
    migrate_up(&conn).unwrap();
    assert!(!table_exists(&conn, "block_links"));
    assert!(!table_exists(&conn, "search_cache"));
}
```

```ts
expect(ci).toContain("--features sqlite-vec");
expect(packageWorkflow).not.toContain("--no-sqlite-vec mac");
```

- [ ] **Step 2: Verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage::migrate::tests::legacy_graph_and_search_cache_tables_are_absent_after_migration_062 -- --nocapture && npm run test -- tests/runtime-contracts.test.ts tests/github-actions-workflows.test.ts`

Expected: old tables and non-vector packaging references still exist.

- [ ] **Step 3: Implement migration, cleanup and release evidence**

Delete the search-cache cleanup invocation and implementation after migration 062. The down migration recreates historic schemas only. Make sqlite-vec release-default on Windows/macOS/Linux, add vector migration/KNN tests to PR CI, and add release-platform smoke plus a nightly 1k/10k/25k/50k performance ladder. Update stale version/model claims in docs; `docs:check` must reject factual drift.

- [ ] **Step 4: Verify release gates**

Run: `npm run docs:check && npm run version:check && npm run test -- tests/runtime-contracts.test.ts tests/github-actions-workflows.test.ts && cargo test --features sqlite-vec --manifest-path src-tauri/Cargo.toml storage::migrate::tests -- --nocapture`

Expected: PASS; no stale v1.2.6 baseline claim or default no-vec package path remains.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/migrations/062_remove_legacy_search_graph.sql src-tauri/migrations/062_remove_legacy_search_graph.down.sql src-tauri/src/storage/migrate.rs src-tauri/src/llm/search_web.rs src-tauri/src/app.rs .github/workflows docs SECURITY.md CONTRIBUTING.md tests
git commit -m "chore(search): 清理旧索引并强化发布门禁"
```

### Task 6: 质量阈值、回归评测与开源维护闭环

**Files:**

- Modify: `src-tauri/tests/rag_broker_eval.rs`
- Modify: `docs/eval/rag-v2-broker-evaluation.md`
- Modify: `docs/eval/semantic-search.md`
- Modify: `scripts/agent-eval.mjs`
- Modify: `scripts/agent-eval.test.mjs`
- Create: `.github/ISSUE_TEMPLATE/bug_report.yml`
- Create: `.github/ISSUE_TEMPLATE/feature_request.yml`
- Create: `.github/pull_request_template.md`
- Create: `.github/dependabot.yml`

**Interfaces:**

- Produces: semantic-only Recall@5 ≥ 0.80; hybrid any-source Recall@5 ≥ 0.95 and Recall@30 ≥ 0.98; all-required Recall@5 ≥ 0.90 and Recall@30 ≥ 0.95; nDCG@10 ≥ 0.85; no-answer FPR ≤ 0.10; scope leaks 0; citation span/hash validity 100%.
- Produces: 50k warm KNN p95 ≤ 750 ms and end-to-end retrieval p95 ≤ 1 s on the declared reference machine; results always include revision, model, platform, fixture hash and raw metrics.

- [ ] **Step 1: Write failing quality-gate tests**

```rust
#[test]
fn vector_quality_gate_fails_when_all_required_recall_at_30_is_below_095() {
    let metrics = BrokerMetrics { all_required_hits_at_30: 47, positive_queries: 50, ..Default::default() };
    assert!(!meets_vector_release_gates(&metrics));
}
```

```js
assert.equal(result.completedCaseCount, 24);
assert.equal(result.passed, 24);
```

- [ ] **Step 2: Verify RED**

Run: `cargo test --features sqlite-vec --manifest-path src-tauri/Cargo.toml --test rag_broker_eval vector_quality_gate -- --nocapture && npm run agent:eval:smoke`

Expected: RED because `meets_vector_release_gates` does not exist; after implementation it returns `false` for the constructed metric. The existing smoke also fails until Task 4 fixes its root cause.

- [ ] **Step 3: Implement fixture/model gates and maintenance files**

Keep deterministic FTS gate separate from provisioned sqlite-vec model gate; neither may silently skip. Add 1k/10k/25k/50k synthetic scale fixtures generated under test temp directories, never from user vaults. Add issue/PR templates that require reproduction, privacy impact, migration and dependency-license disclosure, plus monthly grouped Dependabot updates. Do not add repository bureaucracy beyond these contribution paths.

Resolve the frozen `npm audit` blocker before release: `GHSA-rgw5-rvv9-x895` reaches `brace-expansion` through `minimatch` and the ESLint/TypeScript-eslint development chain. First update the smallest compatible dependency set; if no fixed version exists, document a maintainer-approved, time-bounded exception with affected commands and a removal date. Do not lower the audit severity, ignore the advisory globally, or claim the development-only path is harmless without that decision.

- [ ] **Step 4: Run the complete verification matrix**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml && cargo test --features sqlite-vec --manifest-path src-tauri/Cargo.toml && npm run lint && npm run format:check && npm run typecheck && npm run test && npm run rag:eval && npm run agent:eval:smoke && npm run audit:rust && npm audit`

Expected: every command exits 0. A release candidate additionally waits for all platform vector package smokes and the 50k performance report.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tests/rag_broker_eval.rs docs/eval scripts .github
git commit -m "test(ai): 建立检索与 Agent 发布质量门禁"
```

## Spec Coverage Review

- Chunk structure, source spans, Setext, fences and heading attribution are Task 1.
- Links truth, removal of `block_links`, and catalog cleanup are Tasks 1 and 5.
- All-platform default sqlite-vec, 512 dimensions, migration/up-down, cache triggers, KNN and no Rust full scan are Task 2.
- Partial indexing visibility and typed IPC are Task 3.
- Agent budgets, bounded history, calibration wording, dead guardrail/reranker cleanup and 24-case smoke closure are Task 4.
- `search_cache`, stale docs, packaging/CI and migration removal are Task 5.
- Retrieval metrics, performance, security/licensing and contribution maintenance are Task 6.

## Execution Handoff

Execute Tasks 1–6 in order. Tasks 1 and 2 may be prepared in parallel only in separate user-approved workspaces; this repository currently forbids creating a worktree without explicit user permission. Each task requires a clean task review before the next task changes an interface it consumes.
