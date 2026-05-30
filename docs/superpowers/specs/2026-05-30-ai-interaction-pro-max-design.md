# AI 交互与对话区 Pro Max 设计规格

**日期**: 2026-05-30  
**状态**: 已实施  
**关联计划**: AI 交互 Pro Max 改造

## 目标

- 编辑区：**右键为主**触发选区 AI；`/` 仅文档级命令；取消选区自动浮动条。
- 剪贴板：全应用 `iris-clipboard`，禁止 `document.execCommand`。
- AI 对话：气泡现代化、选区限制在气泡内；研究/聊天结果必达消息时间线。
- 浮层：右键、`/`、`@` 共用 `IrisSurfaceMenu` 视觉，不复用命令面板列表样式。

## 入口分工

| 表面                | 无选区               | 有选区                                            |
| ------------------- | -------------------- | ------------------------------------------------- |
| 右键 `context_menu` | 剪贴板 + AI·文档     | 剪贴板 + AI·选区（改写/扩写/引用/检查/发送到 AI） |
| `/` `slash`         | 总结/大纲/头脑风暴等 | 仅文档级；底部提示「选区操作请使用右键菜单」      |

## 剪贴板契约

- 模块：`src/lib/iris-clipboard.ts`
- TipTap：`copyEditorSelection` / `cutEditorSelection` / `pasteIntoEditor`
- 纯文本字段：`copyTextFieldSelection` / `cutTextFieldSelection` / `pasteIntoTextField`

## 对话流

- `resolveAssistantDisplayContent`：server 正文 → 流式缓冲 → 工具摘要 → 明确错误文案。
- 研究：`ChatLine.kind === "research"` + `ResearchResultMessage`；「展开研究详情」打开 `ResearchFocusView`（`researchPanelExpanded`）。

## 删除项

- `FloatingToolbar.tsx`、`selection_toolbar`、`getEditorSelectionRect`、`editor-selection-rect.ts`
- AI 消息选区浮动 portal
- Slash 菜单对 `CommandListOption` 的依赖
