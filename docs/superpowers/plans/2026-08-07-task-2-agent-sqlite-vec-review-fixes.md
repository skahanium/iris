# Task 2 Agent sqlite-vec Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让真实 Agent/RAG retrieval broker 只通过 sqlite-vec v3 KNN 执行三类语义检索，在候选产生前执行全部硬 scope，并让桌面发布无法绕过 sqlite-vec 加载烟测。

**Architecture:** `hybrid_retrieve_with_diagnostics` 在确认 v3 后只生成一次 query embedding，再把同一向量与 `RetrievalScope` 传给 chunk、anchor、regulation 三个 KNN consumer。三张 vec0 表都保存 `file_id` 元数据，KNN 的 `MATCH + k` 查询用相同的 scoped file-id 子查询约束 exact path、prefix、required tags 与 `.classified`；FTS 独立执行并在 vector unavailable 时继续返回可见诊断与候选。

**Tech Stack:** Rust、rusqlite、sqlite-vec 0.1.9、SQLite migration、Vitest、GitHub Actions、Node.js packaging script。

## Global Constraints

- 保持应用版本 `1.2.18`，不新增依赖，不创建 worktree。
- 默认 Cargo feature 必须包含 `sqlite-vec`；不得保留发布打包禁用开关。
- 三条 Agent vector 路径不得读取全部 embedding BLOB 或在 Rust 中计算 cosine。
- v2 cache 继续作为 canonical source；v3 继续只由 migration 回填和 SQL trigger 镜像。
- 缺 feature、扩展加载失败或 v3 migration 缺失必须显式 unavailable；FTS 不受影响。

---

### Task 1: Agent broker KNN 与 scope RED

**Files:**

- Modify: `src-tauri/src/ai_runtime/retrieval_broker/diagnostics.rs`
- Modify: `src-tauri/src/ai_runtime/retrieval_broker/vector.rs`

**Interfaces:**

- Consumes: `RetrievalRequest.scope`、v2 canonical caches、三张 v3 vec0 表。
- Produces: `hybrid_retrieve_with_diagnostics_with_embedder(conn, request, embedder)` 测试入口；生产入口仍为 `hybrid_retrieve_with_diagnostics(conn, request)`。

- [x] **Step 1: 写真实 broker 失败测试**

```rust
let outcome = hybrid_retrieve_with_diagnostics_with_embedder(
    conn,
    &scoped_vector_request,
    |_| Ok(query_vector.clone()),
)?;
assert!(outcome.packets.iter().any(|packet| packet.source_path.as_deref() == Some("allowed/needle.md")));
assert!(outcome.diagnostics.iter().filter(|item| item.backend.as_deref() == Some("sqlite-vec")).count() == 3);
```

种入多于 `candidate_limit` 的 scope 外更近向量，并让 scope 内 needle 同时满足 path/prefix、required tag 且非 `.classified`；三类 v3 consumer 均必须在 broker 结果或诊断中出现。

- [x] **Step 2: 确认 RED**

Run: `cargo test --locked --manifest-path src-tauri/Cargo.toml ai_runtime::retrieval_broker::diagnostics_impl::tests::agent_vector -- --nocapture`

Expected: FAIL，因为当前 broker 标记 `cosine-rust` 且 scope 在候选截断后执行。

- [x] **Step 3: 最小 KNN 实现**

```rust
let query_embedding = embedder(&request.query)?;
append_layer_result_with_meta(
    "vector_chunks",
    search_vector_chunks(conn, &query_embedding, candidate_limit, &request.scope),
    packets,
    diagnostics,
    Some("sqlite-vec".into()),
    Some(EMBEDDING_MODEL_ID.into()),
);
```

三个查询均使用 `embedding MATCH ? AND k = ? AND file_id IN (SELECT ...)`，随后 join canonical cache 并校验当前 model、512 维、source fingerprint 与 2048-byte BLOB。

- [x] **Step 4: 确认 GREEN**

Run: `cargo test --locked --manifest-path src-tauri/Cargo.toml ai_runtime::retrieval_broker::diagnostics_impl::tests::agent_vector -- --nocapture`

Expected: PASS，三层 backend 均为 `sqlite-vec`，scope 外近邻无法吞掉 needle。

### Task 2: v3 元数据与 unavailable/FTS 契约

**Files:**

- Modify: `src-tauri/migrations/061_sqlite_vec_v3.sql`
- Modify: `src-tauri/src/storage/migrate.rs`
- Modify: `src-tauri/src/ai_runtime/retrieval_broker/diagnostics.rs`

**Interfaces:**

- Consumes: `semantic_anchors.file_id`、`regulation_index.file_id`。
- Produces: `vec_anchors_v3.file_id` 与 `vec_regulations_v3.file_id` INTEGER metadata；统一 vector unavailable diagnostic。

- [x] **Step 1: 扩充 migration/broker RED 断言**

```rust
assert_eq!(vec_anchor_file_id, anchor_file_id);
assert_eq!(vec_regulation_file_id, regulation_file_id);
assert_eq!(vector_diagnostic.status, RetrievalLayerStatus::Unavailable);
assert!(outcome.packets.iter().any(|packet| packet.retrieval_reason == "fts_keyword_match"));
```

- [x] **Step 2: 运行并确认 RED**

Run: `cargo test --locked --manifest-path src-tauri/Cargo.toml sqlite_vec_v3 -- --nocapture`

Expected: FAIL，因为两张结构化 vec0 表尚无 `file_id`。

- [x] **Step 3: 修改 061 与 trigger**

```sql
CREATE VIRTUAL TABLE vec_anchors_v3 USING vec0(
    anchor_id INTEGER PRIMARY KEY,
    embedding float[512] distance_metric=cosine,
    file_id INTEGER
);
```

regulation 同样增加 `file_id`，回填和 INSERT/UPDATE trigger 都从 canonical source 表查询对应 file id。

- [x] **Step 4: 运行并确认 GREEN**

Run: `cargo test --locked --manifest-path src-tauri/Cargo.toml sqlite_vec_v3 -- --nocapture`

Expected: PASS，三张 vec0 表的 file id 镜像一致。

### Task 3: 发布 feature fail-closed 契约

**Files:**

- Modify: `scripts/package-local.mjs`
- Modify: `package.json`
- Modify: `.github/workflows/package-desktop.yml`
- Modify: `tests/package-local-script-contract.test.ts`
- Modify: `tests/github-actions-workflows.test.ts`
- Modify: `src-tauri/tests/embedding_model_smoke.rs`

**Interfaces:**

- Consumes: Cargo default feature `sqlite-vec`。
- Produces: 无禁用参数的 macOS/Windows 发布命令；无 feature 时仍存在且必失败的 ignored smoke。

- [x] **Step 1: 更新契约测试并确认 RED**

```ts
expect(source).not.toContain("--no-sqlite-vec");
expect(source).not.toContain("--sqlite-vec");
expect(workflow).toContain("node scripts/package-local.mjs mac");
expect(pkg().scripts["package:local:win:vec"]).toBeUndefined();
```

Run: `npm run test -- tests/package-local-script-contract.test.ts tests/github-actions-workflows.test.ts`

Expected: FAIL，旧脚本仍允许禁用并把 Windows 默认为 disabled。

- [x] **Step 2: 实现统一默认 feature 与 smoke gate**

删除 packaging feature 参数和 `package:local:win:vec`；本地 package 在构建前运行 ignored smoke。Rust smoke 测试不再由 `cfg(feature)` 整体消失，而在 `cfg(not(feature))` 分支显式 panic。

- [x] **Step 3: 验证 GREEN 与 no-default fail-closed**

Run: `npm run test -- tests/package-local-script-contract.test.ts tests/github-actions-workflows.test.ts`

Expected: PASS。

Run: `cargo test --locked --no-default-features --manifest-path src-tauri/Cargo.toml --test embedding_model_smoke bundled_sqlite_vec_loads_and_applies_v3_index_migration -- --ignored --nocapture`

Expected: FAIL，消息明确说明发布缺少 `sqlite-vec` feature。

### Task 4: 回归、自审、报告与提交

**Files:**

- Modify: `.superpowers/sdd/task-2-report.md`

- [x] **Step 1: 运行关联回归**

Run: broker/engine 默认与 `--no-default-features` 测试、package workflow contract、`cargo fmt --all -- --check`、双 feature `cargo clippy --all-targets -- -D warnings`。

- [x] **Step 2: 静态自审**

Run: `rg -n "cosine_similarity|bytes_to_f32|cosine-rust" src-tauri/src/ai_runtime/retrieval_broker`

Expected: 无命中。

- [x] **Step 3: 追加报告并提交**

记录 RED/GREEN、精确命令、默认与 no-default 特性行为、unsafe 状态、未完成项；使用中文 Conventional Commit 提交所有本轮文件。
