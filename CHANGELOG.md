# Changelog

本项目的所有显著变更将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### Planned

- **v0.1.1**：纸墨体验阶段 0 扫尾、自定义 API Base URL、可选 Playwright E2E（见 [ROADMAP.md](./ROADMAP.md)）
- **v0.2**：双向链接、知识图谱、sqlite-vec、纸墨体验阶段 1

### Added

- 设计系统 [docs/design-system.md](./docs/design-system.md)：纸墨（B）为主、命令优先（C）为辅；赭石 accent、编辑区纸面、衬线正文、`Ctrl+Shift+A` 收起 AI 侧栏

### Changed

- 路线图：删除 v0.4 插件与「未来探索」冗项；**AI 自动标签**为待定特色；新增 **v0.1.1** 并将体验阶段与 v0.1.1 / v0.2 / v0.3 / v1.0 绑定（见 [ROADMAP.md](./ROADMAP.md)）
- 文档体系：新增 [docs/README.md](./docs/README.md)、[v0.1.1-epic](./docs/v0.1.1-epic.md)；统一 README / CONTRIBUTING / ARCHITECTURE / AGENTS / completion-prs 交叉引用

## [0.1.0] - 2026-05-25

首个可本地运行的 AI 原生 Markdown 笔记 MVP。数据以用户目录中的 `.md` 为权威来源，SQLite 为索引缓存。

### Added

- Tauri 2 + React 19 应用脚手架与 CI 工作流
- TipTap **核心 GFM** 编辑器、多标签、暗色/亮色主题（`gfm-schema.ts` 列明支持范围）
- 笔记目录（Vault）、`file_*` IPC、SQLite 索引与 FTS5 关键词搜索
- fastembed 语义搜索（`chunk_embeddings` BLOB + Rust 余弦 Top-K；评测见 `docs/eval/semantic-search.md`）
- YAML **frontmatter** 与 **tags** 索引（`file_tags`；不含正文 `#tag`）
- 文件监听与外部修改提示；**切换 vault 后自动重建** `FileWatcher`
- LLM 流式集成：OpenAI 兼容、**Anthropic Messages API**、Ollama
- 右栏 AI 面板：语义 **关联笔记** Top-K 注入上下文、共享 **provider** 选择
- 内联 AI：**接受 / 重试 / 回退**（`ai-stream` 节点 + 原文快照）
- `/` 命令流式写入 `ai-stream`；与侧栏使用同一 provider
- 可选联网搜索：**Bing**（`iris/bing-search` 凭据）或 **DuckDuckGo** 降级
- OS 凭据管理器：`credential_*` IPC（LLM + Bing）
- Quick Open（Ctrl+P）、文件 Sheet（Ctrl+Shift+E）、搜索面板（Ctrl+Shift+F）
- Markdown 往返测试扩充（`tests/markdown_roundtrip.test.ts`）
- 语义 Recall@5 fixture 集与 `#[ignore]` 集成测试（`semantic_recall_eval.rs`）

### Changed

- ROADMAP：文件导航表述与实现一致（Sheet + 快捷键）；v0.1 语义检索文档化（非 sqlite-vec 虚拟表）
- ARCHITECTURE：语义检索与 `chunk_embeddings` schema 与实现对齐
- ROADMAP v0.2：规划 **sqlite-vec** 中后期 MVP 升级路径

### Security

- API Key 仅通过操作系统凭据管理器存储，禁止写入日志/SQLite/明文配置

### Known limitations (v0.1.0)

- 无自定义 API Base URL 图形设置（`custom_base_url` 仅 IPC 预留）
- 无完整 GFM（脚注、数学公式、图片节点等见 `gfm-schema.ts`）
- 语义检索在超大 vault 上为全量余弦扫描（sqlite-vec 计划 v0.2）
- E2E 为 Vitest 场景占位，非 Playwright 驱动

---

## 版本发布说明

### 变更分类

- **Added** — 新增功能
- **Changed** — 现有功能的变更
- **Deprecated** — 即将移除的功能
- **Removed** — 已移除的功能
- **Fixed** — 漏洞修复
- **Security** — 安全相关修复

### 证据索引（v0.1.0 补齐 PR）

| PR | 主题 | 主要证据 |
|----|------|----------|
| C1 | 内联 AI 三按钮 | `AiStreamExtension`、`useInlineAi`、`tests/inline-ai.test.ts` |
| C2 | Provider 一致 | `useLlmProvider`、`tests/llm-provider.test.ts` |
| C3 | Anthropic API | `llm/anthropic.rs` |
| C4 | 关联笔记 | `ai-context.ts`、`AiPanel` |
| C5 | frontmatter/tags | `indexer/frontmatter.rs`、`tests/frontmatter_index.rs` |
| C6 | vault 监听 | `AppState::restart_file_watcher`、`tests/vault_watcher.rs` |
| C7 | Bing Key UI | `AiPanel`、`search_web.rs` |
| C8 | 语义评测文档 | `docs/eval/semantic-search.md` |
| C9 | GFM 往返测试 | `tests/markdown_roundtrip.test.ts`、`gfm-schema.ts` |
| C10 | ROADMAP/CHANGELOG | 本文件、`ROADMAP.md` |
