# Markdown 导出语义

本文说明 Iris 编辑器如何将 TipTap 文档写回 `.md`，以及与 Obsidian / 标准 GFM 的差异。

---

## 热路径（保存 / Ctrl+S）

1. `serializeOpenNote` → `editorDocToMarkdown`（`prosemirror-markdown`）
2. 失败时回退 `editorBodyHtmlToMarkdown`（HTML + Turndown）

详见 `src/lib/editor-pm-serialize.ts`。

---

## Callout（`> [!type] Title`）

| 阶段 | 行为 |
| ---- | ---- |
| **分类** | `markdown-contract` 将 Obsidian callout 标为 `render_only`（可编辑，非 `preserve_only`） |
| **导入** | `ingestMarkdownForEditor` → blockquote + `data-callout-original-raw`（打开时的原文） |
| **Schema** | `CalloutBlockquoteExtension` 保留 `calloutType` 与 `calloutOriginalRaw` |
| **导出** | **未编辑**的 callout → **原样写回** `calloutOriginalRaw`；编辑后才按结构生成 `> [!type] …` |

普通引用块（无 `data-callout-type`）仍导出为标准 `>` blockquote。

共享逻辑：`src/lib/callout-markdown.ts`。

---

## preserve_only（原样保留）

脚注定义、原始 HTML、contract 标记为 `preserve_only` 的片段 → `preserveBlock` 节点 → 导出时写入 `originalRaw`，**不参与** callout 或 Turndown 改写。

---

## 非目标

- 不要求与任意第三方 Markdown 方言 **字节级** 一致（表格对齐空格、列表缩进风格等允许规范化）。
- 不把 callout 改为只读 preserve 块（会损害编辑体验且与 contract 冲突）。

---

## 相关测试

- `tests/editor-pm-serialize.test.ts` — PM 热路径与 callout
- `tests/callout-markdown.test.ts` — callout 字符串 helper
- `tests/editor-real-roundtrip.test.ts` — 完整笔记往返
