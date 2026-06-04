# 「/」与右键体系设计定稿

> **2026-05-30 更新**：选区浮动条已移除。现行行为见 [ai-interaction-pro-max-design.md](./2026-05-30-ai-interaction-pro-max-design.md)（右键为主、`IrisSurfaceMenu`）。

**日期**：2026-05-30  
**状态**：已由 Pro Max 改造 supersede 部分条目

## 目标

- 命令面板仅保留全局 AI（侧栏、联网、Skills、发送选区等）。
- 写作型 AI 主路径：`/` 菜单、选区浮动条、完全自定义右键（`iris_only`）。
- AI 对话区：消息选区迷你条 + 右键；Composer 剪贴板右键。

## 单一事实来源

[`src/lib/editor-actions.ts`](../../src/lib/editor-actions.ts) 定义 `scopes`、`surfaces`、`kind`；执行经 [`editor-action-executor.ts`](../../src/lib/editor-action-executor.ts)。

| Surface             | 编辑区      | AI 消息             | Composer       |
| ------------------- | ----------- | ------------------- | -------------- |
| `slash`             | 是          | —                   | —              |
| `selection_toolbar` | 是          | 迷你条（复制/引用） | —              |
| `context_menu`      | 剪贴板 + AI | 复制/引用到输入     | 复制/粘贴/全选 |

## 交互要点

- 编辑区 `contextmenu`：`preventDefault` + `IrisContextMenu`。
- 浮动条锚定选区上方（`getEditorSelectionRect` + fixed portal）。
- 流式 inline AI 期间禁用改写类动作。
- `send-selection-ai` 命令面板项在无选区时由 `App` toast 提示。

## 不在范围

- `/heading` 结构块菜单
- 消息「插入编辑区」IPC
- 命令面板内写作 AI 回归
