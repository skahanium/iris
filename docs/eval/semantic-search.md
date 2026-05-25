# 语义搜索评估（v0.1.0）

> **文档索引**：[docs/README.md](../README.md) · **排期**：[ROADMAP § v0.2 语义检索升级](../../ROADMAP.md#搜索与索引中后期-mvp--语义检索升级)

## v0.1 实现路径（非 sqlite-vec 虚拟表）

Iris v0.1 的语义检索采用**本地嵌入 + SQLite BLOB + Rust 全量余弦**，与 ROADMAP 早期「sqlite-vec 虚拟表」表述不同，以本文档与 [ROADMAP.md](../../ROADMAP.md) 为准。

| 环节 | 实现 |
|------|------|
| 嵌入模型 | [fastembed](https://github.com/Anush008/fastembed-rs) `AllMiniLML6V2`（384 维） |
| 存储 | `chunk_embeddings.embedding` BLOB（`f32` 小端序列化） |
| 分块 | `chunk_markdown`，约 2000 字符/块，见 `indexer/chunker.rs` |
| 检索 | `embedding::engine::semantic_search`：对全部 chunk 计算余弦相似度后排序取 Top-K |
| IPC | `search_semantic(query, limit?)`，默认 `limit=5` |
| 重建索引 | `search_reindex` / 扫描 vault 时 `store_chunk_embeddings` |

**数据流：**

```
.md 文件 → scan_vault → chunks 表
                    → fastembed → chunk_embeddings BLOB
用户查询 → embed(query) → 与每条 chunk 余弦 → Top-K SemanticHit { path, title, snippet, score }
```

**v0.2+ 方向**：见 [ROADMAP.md](../../ROADMAP.md)「v0.2 — 搜索与索引 · 语义检索升级」：引入 **sqlite-vec 虚拟表**，迁移 BLOB 嵌入并改造 `search_semantic`，在万级 chunk 下替代全表 Rust 余弦扫描。

---

## 指标

- **Recall@5**：每条查询的期望笔记是否出现在语义 Top-5 的 `path` 中（按 chunk 命中，同文件多 chunk 任一命中即算成功）。
- **目标**：≥ **0.6**（20 条中至少 12 条命中）。

---

## 评测集

- **Fixture vault**：[`fixtures/semantic-vault/`](./fixtures/semantic-vault/)（20 篇标注笔记，中文主题互不混淆）
- **自动化**：`src-tauri/tests/semantic_recall_eval.rs`（`#[ignore]`，需下载嵌入模型）

```bash
cd src-tauri
cargo test semantic_recall_at_5_on_fixture_vault -- --ignored --nocapture
```

---

## 评测结果（fixture vault，2026-05-25）

| # | 查询 | 期望 path | Top-1 path | 命中@5 |
|---|------|-----------|------------|--------|
| 1 | 性能优化 帧率 reindex profiling | `perf-meeting.md` | `perf-meeting.md` | 是 |
| 2 | SQLite 元数据与 FTS 索引 | `sqlite-arch.md` | `fts-keyword.md` | 是 |
| 3 | Tauri 2 桌面应用 | `tauri-stack.md` | `tauri-stack.md` | 是 |
| 4 | TipTap ai-stream 流式 | `tiptap-editor.md` | `tiptap-editor.md` | 是 |
| 5 | iris/bing-search 凭据 | `credentials-security.md` | `credentials-security.md` | 是 |
| 6 | all-MiniLM-L6-v2 嵌入 | `embedding-model.md` | `embedding-model.md` | 是 |
| 7 | search_semantic 关联笔记 | `semantic-search-impl.md` | `semantic-search-impl.md` | 是 |
| 8 | Bing 失败 DuckDuckGo | `web-search-fallback.md` | `web-search-fallback.md` | 是 |
| 9 | frontmatter tags 表 | `frontmatter-tags.md` | `frontmatter-tags.md` | 是 |
| 10 | FileWatcher notify 监听 | `file-watcher.md` | `file-watcher.md` | 是 |
| 11 | Anthropic content_block_delta | `anthropic-api.md` | `anthropic-api.md` | 是 |
| 12 | htmlToMarkdown round-trip | `markdown-roundtrip.md` | `markdown-roundtrip.md` | 是 |
| 13 | 内联 AI 接受回退 | `inline-ai.md` | `inline-ai.md` | 是 |
| 14 | 双向链接 力导向图 | `knowledge-graph-v02.md` | `knowledge-graph-v02.md` | 是 |
| 15 | AGPL-3.0 依赖许可 | `agpl-license.md` | `agpl-license.md` | 是 |
| 16 | chunk_markdown 分块 | `chunking-strategy.md` | `chunking-strategy.md` | 是 |
| 17 | files_fts unicode61 | `fts-keyword.md` | `fts-keyword.md` | 是 |
| 18 | Ollama 11434 本地 | `ollama-local.md` | `ollama-local.md` | 是 |
| 19 | AES-256-GCM 加密 | `vault-encryption.md` | `vault-encryption.md` | 是 |
| 20 | Recall@5 0.6 目标 | `eval-recall.md` | `eval-recall.md` | 是 |

**汇总**：Recall@5 = **20/20 = 1.00**（fixture 集；阈值 ≥ 0.6，达标）

**说明**：过于笼统的中文查询（如仅「性能优化会议记录」）在实测中可能误召回它篇；评测查询宜包含笔记中的 distinctive 关键词（见 `semantic_recall_eval.rs` 中的 `EVAL_QUERIES`）。

---

## 人工复现步骤

1. 将 `docs/eval/fixtures/semantic-vault` 设为 Iris 笔记目录，或复制到测试目录
2. 应用内执行全文/语义重建索引（`search_reindex` 或打开 vault 触发 `scan_vault`）
3. 在搜索面板或 `search_semantic` IPC 逐条验证上表查询
4. 用户真实 vault 的 Recall 可能低于 fixture，发布前建议在目标语料上抽检

---

## 与产品功能的关联

- **搜索面板**：语义 Tab 调用 `search_semantic`
- **AI 侧栏（PR-C4）**：提问前对 vault 语义 Top-5（排除当前文件）注入 system 上下文
