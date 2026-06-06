# 内联 AI 候选框设计规范

**日期**: 2026-06-06  
**状态**: 已实施

## 目标

翻译、改写等内联 AI 操作时：

1. **原文保留原位**，不被候选块替换或遮挡
2. **候选块出现在选区所在块下方**，便于对照
3. 生成过程中 **「放弃」始终可点**，可随时取消流式请求
4. 操作栏与视觉对齐 design-system（非等宽、轻量 accent 边）

## 交互

```
…前文…
[原文段落 — 轻量高亮]
┌─ AI 候选 ─────────────────────┐
│  translate · 生成中…          │
│  [放弃] [重试] [接受]          │
│  候选译文流式输出…              │
└───────────────────────────────┘
…后文…
```

| 操作 | 行为 |
|------|------|
| **接受** | 用候选文本替换 `sourceFrom–sourceTo` 原文范围；删除 `aiStream` 节点；清除高亮 |
| **放弃** | `llmAbort` + 仅删除 `aiStream`；原文与高亮清除（rollback） |
| **重试** | 清空候选、状态回到 `streaming`；重新 `llmGenerate`；原文不变 |

### 按钮可用性

| 状态 | 放弃 | 重试 | 接受 |
|------|------|------|------|
| streaming | ✓ | ✗ | ✗ |
| ready | ✓ | ✓ | ✓（有内容） |
| error | ✓ | ✓ | ✗ |

### 快捷键

- `Escape`：放弃（存在活跃 `aiStream` 时）
- `Mod+Enter`：接受（仅 `ready` 且有内容）

## 数据模型

`aiStream` 节点 attrs：

| 字段 | 说明 |
|------|------|
| `status` | `streaming` \| `ready` \| `error` |
| `originalText` | 启动时快照，供 retry prompt |
| `action` | 内联动作 id（如 `translate`、`rewrite`） |
| `sourceFrom` | 原文选区起始 doc position |
| `sourceTo` | 原文选区结束 doc position |

Slash 命令插入（`insertAiStreamAtCursor`）无原文对照，`sourceFrom/To` 为 0。

## 实现要点

- **插入**: `insertAiStreamBelowSelection` — 解析 `$from.blockRange($to)`，在块末 `insertPos` 插入节点，不 `deleteSelection`
- **高亮**: `aiSourceHighlight` Mark（`.iris-ai-source-highlight`）
- **NodeView**: `contentEditable={false}` + `mousedown` `preventDefault` 防止 ProseMirror 抢焦点
- **IPC**: 仍走现有 `llmGenerate` / `llmAbort`；`useInlineAi.dismiss` 负责 abort

## 边界（首版 YAGNI）

- 生成中用户手动改原文：accept 仍按 attrs 中 `sourceFrom/To` 替换；不检测 drift
- 并发内联 AI：沿用 `editorHasActiveAiStream` 门禁
- 多候选 A/B、侧栏 PatchPreview 式 diff：不在本规范范围

## 相关文件

- `src/components/editor/extensions/AiStreamExtension.ts`
- `src/components/editor/extensions/AiSourceHighlightExtension.ts`
- `src/components/editor/AiNodeView.tsx`
- `src/hooks/useInlineAi.ts`
- `tests/inline-ai.test.ts`
- `tests/ai-node-view.test.tsx`
