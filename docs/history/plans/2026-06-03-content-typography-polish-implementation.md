# Prose Polish v2 — 实施清单

> 对应设计：[2026-06-03-content-typography-polish-design.md](./2026-06-03-content-typography-polish-design.md)

## M0 字体

- [x] `index.html`：Noto Sans SC + Noto Serif SC + Inter + JetBrains Mono
- [x] `tailwind.config.js`：`font-prose`、`font-title`
- [x] `markdown-prose.css`：`--font-*` token

## M1 Prose token

- [x] `markdown-prose.css`：editor / conversation 表面、spacer、标题尺度
- [x] `globals.css`：移除重复 `.ai-msg` 与编辑区标题规则
- [x] `TipTapEditor`：`data-prose-surface="editor"`

## M2 编辑区

- [x] 文档标题居中 + `--prose-title-gap`
- [x] 正文 justify + line-height 1.62

## M3 AI 协作区

- [x] `AiMessageBubble` + `AiThinkingIndicator`
- [x] 移除 inset 流式左边条；助手/用户壳重做
- [x] `AiMessageList` 消息间距 `gap-4`

## 文档

- [x] `design-system.md` 定位与字体
- [x] 本清单

## 验证

- [x] `npm run lint`
- [x] `npm run typecheck`
- [x] `npm run test`（908 passed）
