# Iris 内容展示格调升级（Prose Polish v2）设计

> 定稿日期：2026-06-03。实现：`markdown-prose.css`、`globals.css`、`AiMessageBubble`、字体 CDN。

## 产品北极星

- **编辑区**：出版感阅读——居中扉页标题（Noto Serif SC）、正文 Noto Sans SC、两端对齐、紧凑空行（spacer ≈ 55% 行高）。
- **AI 协作区**：与笔记并列的一等对话面；精致消息壳、15px 正文、无 inset 流式左边条。
- **评判**：Iris 高端知识工作台气质，不以第三方笔记应用为验收标尺。

## 字体（F1 分层）

| Token          | 栈                                 | 用途                    |
| -------------- | ---------------------------------- | ----------------------- |
| `--font-ui`    | Inter + 系统无衬线                 | Chrome、按钮            |
| `--font-prose` | Noto Sans SC, Inter, 苹方/雅黑     | 编辑正文、AI Markdown   |
| `--font-title` | Noto Serif SC, Noto Sans SC, serif | 仅 `DocumentTitleField` |
| `--font-mono`  | JetBrains Mono                     | 代码                    |

## Prose Token（`:root`）

| Token                      | 值        |
| -------------------------- | --------- |
| `--prose-size-editor`      | `1rem`    |
| `--prose-size-chat`        | `15px`    |
| `--prose-line-height`      | `1.62`    |
| `--prose-line-height-chat` | `1.52`    |
| `--prose-letter-spacing`   | `0.01em`  |
| `--prose-block-gap`        | `0.6em`   |
| `--prose-title-gap`        | `2.25rem` |
| `--prose-spacer-ratio`     | `0.55`    |

## 表面修饰符

- `[data-prose-surface="editor"]`：justify 正文、spacer、章节标题尺度。
- `[data-prose-surface="conversation"]`：不 justify；略紧块间距；任务列表/表格与编辑区同规范。

## AI 消息壳

- 用户：`--ai-user-bg`、圆角 `12px`、`max-w-[88%]`。
- 助手：`surface-elevated` 轻底、四边圆角 `12px`、无 `inset` 左边条。
- 流式空态：`ai-thinking-row` 单行 + pulse。

## 文档同步

- [design-system.md](../design-system.md) 已更新协作区定位与字体表。
- [notion-master.md](../design-system/notion-master.md) 保留为历史参考。
