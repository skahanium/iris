# Changelog

本项目的所有显著变更将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [0.2.0] - Unreleased

### Added

- **`[[wiki-link]]` 双向链接**：编辑器语法支持、自动补全、click 导航、links 表索引、turndown 往返序列化
- **反向链接面板**（`Ctrl+Shift+B`）：显示链接到当前笔记的所有源笔记
- **body `#tag` 解析**：正文 `#tag` 与 YAML frontmatter tags 合并索引
- **标签聚合视图**（`Ctrl+Shift+T`）：标签云 + 统计面板（笔记数、标签数）
- **知识图谱可视化**（`Ctrl+Shift+G`）：Canvas 力导向图，零外部依赖，节点大小按被引用数缩放
- **sqlite-vec 迁移**：`002_vec.sql` 虚拟表 + vec 优先搜索 + cosine fallback 双路径
- **纸墨阶段 1 UI**：引用卡完整形态（heading / "仅此次" / 折叠展开）、关联笔记芯片、`/` 命令菜单图标
- Rust 测试 55 个（+23 from v0.1.1），TypeScript 测试 43 个（+6 from v0.1.1）
- `links` 表加入 `001_core.sql` migration（ARCHITECTURE 文档已定义但此前未执行）

### Changed

- `index_file()` 流水线新增 wikilink 提取与 body tag 合并步骤
- AiPanel 引用卡从简单 quote 卡片升级为完整形态（纸墨 token、折叠、"仅此次"）
- 关联笔记从列表改为芯片（chip）展示
- `/` 命令菜单从纯文本改为图标 + 纸墨 border token

### Fixed

- FTS5 migration 移除 `content=''` 避免 contentless 模式导致 MATCH 查询失败

## [0.1.1] - 2026-05-25

### Added

- 设置面板（`Ctrl+,`）：LLM API Key、Bing 搜索 Key、自定义 API Base URL 收纳于统一设置 Sheet
- `src/components/ui/dialog.tsx`：标准 shadcn/ui Dialog 封装（基于 @radix-ui/react-dialog）
- Quick Open 改用 Dialog 组件，统一 chrome overlay 蒙版
- Rust 测试补全：路径穿越（4 cases）、索引流水线（7 cases）、数据库初始化（3 cases）、错误序列化（4 cases）
- frontmatter 边界用例测试（BOM、空 frontmatter、非法 YAML）
- chunker 边界用例测试（单段落、max_chars、空输入）
- `tests/prompts.test.ts`：内联 AI 和 `/` 命令 prompt 构建器测试

### Changed

- **移除**：AI 侧栏中的 API Key 输入区。Key 管理统一移至设置面板（`Ctrl+,`）
- 精简 AiPanel：移除 Key 管理相关 state 与 imports，侧栏仅保留对话功能
- QuickOpen 组件由自定义 overlay div 重构为 Dialog + DialogContent

### Fixed

- QuickOpen 开启后搜索框自动聚焦（Dialog 原生支持）

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
