# Markdown 索引契约

> 更新日期：2026-07-18

编辑器（TipTap + contract ingest + PM serialize）与 Rust 索引器对 Markdown 的理解 intentionally 不完全相同。本文档记录**当前事实契约**，避免静默漂移。

## 生产往返路径

- **打开**：`ingestMarkdownForEditor` → TipTap
- **保存**：`editorDocToMarkdown`（`editor-pm-serialize.ts`）
- **测试标准**：`tests/helpers/tiptap-serialize-harness.ts` 中的 `fullNoteRoundTrip`

遗留 Marked→Turndown 路径仅用于历史 contract 测试，**不是**生产保存路径。

## LLM Run 采样参数

- Assistant Run / Model Gateway 请求当前**固定不传** `temperature`（`None`）。
- 网关 body 层已支持 `Option<f64>`；设置页与 `LlmRoutingConfig` **未**暴露该控件。
- 产品若需要可调采样，应经 routing 配置透传，而不是在网关硬编码默认值。

## 编辑器有、索引器无（或不同）

| 构造 | 编辑器 | Rust 索引器 |
|---|---|---|
| `![[media\|alias]]` wiki embed | `wikiMediaEmbed` 节点 | 不索引（产品契约：仅渲染，不进链接图） |
| Body `#tag` | 无专门语义 | 合并进 frontmatter tags |
| Callout `> [!type]` | 可编辑 blockquote | 进 FTS 原文，无特殊结构 |
| `data-iris-indent` | 段落/标题缩进 | 进 FTS 原文 |

## Frontmatter

- **JS**：宽松行解析；复杂 YAML 原样保留
- **Rust**：`serde_yaml` 严格解析；无效 YAML 导致索引失败
- **保存策略**：用户可保存 JS 能打开的笔记；索引失败时标记 degraded 并待修复

## Code fence 与分块

- **Wikilink / image 提取**：跳过 fence 与 inline code
- **Chunker**：自 2026-07-18 起使用 `FenceState`，fence 内 `#` 不作为标题边界

## 变更纪律

修改 TipTap schema 或 indexer 语义时：

1. 更新本文件
2. 更新 `fullNoteRoundTrip` 或等价生产 harness 测试
3. 若与标准 Markdown 有差异，在 schema 或扩展文件加文档注释
