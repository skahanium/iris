# Iris 审计修复与全面清理 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 GPT 审计和自审发现的所有 P0/P1 缺陷，清理约 1400 行死代码，统一数据类型，修复质量门禁，使项目恢复正常状态。

**Architecture:** 分 7 个 Task，按优先级排列：P0 三件（AI 对话短路、文件夹创建、静默保存）→ P1 四件（SSE 解析、tool_confirm、TokenUsage 统一、doctest）→ P2 清理（死代码、lint/format、数据原则文档）。每个 Task 可独立验证。

**Tech Stack:** React 19, TypeScript, TipTap, Tauri 2.x, Rust, SQLite

---

## 文件变更总览

### 前端修改

- `src/components/ai/UnifiedAssistantPanel.tsx` — 修复 AI 对话短路
- `src/components/file/VaultNavigator.tsx` — 修复文件夹创建/重命名
- `src/components/editor/TipTapEditor.tsx` — 修复 mount 时静默保存
- `src/components/settings/LlmRoutingSection.tsx` — 修复 lint error

### 后端修改

- `src-tauri/src/commands/ai_commands.rs` — 修复 context_assemble 和 tool_confirm
- `src-tauri/src/ai_runtime/model_gateway.rs` — 修复 SSE 解析、统一 TokenUsage
- `src-tauri/src/ai_runtime/mod.rs` — 修复 doctest、删除重复 TokenUsage、移除 eval 模块
- `src-tauri/src/ai_runtime/citation_workflow.rs` — 删除死代码
- `src-tauri/src/ai_runtime/writing_workflow.rs` — 修复重复 doc comment
- `src-tauri/src/knowledge/mod.rs` — 移除 graph 模块声明
- `src-tauri/src/security/secure_delete.rs` — 移除 #[allow(dead_code)]
- `src-tauri/Cargo.toml` — tempfile 移到 dev-dependencies

### 删除文件（前端）

- `src/lib/content-hash.ts` — 从未导入
- `src/lib/ai/prompt-display.ts` — 从未导入
- `src/lib/ai/packet-types.ts` — 从未导入
- `src/hooks/useAiPanelLlmStream.ts` — deprecated，从未导入
- `src/components/ai/KnowledgeInbox.tsx` — 从未导入
- `src/components/ai/ProfileManager.tsx` — 从未导入
- `src/components/ai/WorkflowIndicator.tsx` — 从未导入
- `src/hooks/useInlineSuggestion.ts` — stub，无后端实现
- `src/components/editor/InlineSuggestion.tsx` — 配套 stub 组件

### 删除文件（后端）

- `src-tauri/src/ai_runtime/eval.rs` — 412 行，仅自测，生产代码从未调用
- `src-tauri/src/knowledge/graph.rs` — 432 行，生产代码从未调用

---

## Task 1: 修复 AI 对话短路 [P0]

**问题:** `runKnowledgeChat` 每次都因 `execution_plan` 存在而短路进入 `awaiting_confirmation`，用户必须手动批准才能继续。根因是 `context_assemble` 无条件返回 `Some(execution_plan)`。

**Files:**

- Modify: `src-tauri/src/commands/ai_commands.rs:104-109`
- Modify: `src/components/ai/UnifiedAssistantPanel.tsx:638-664`

- [ ] **Step 1: 修复 `context_assemble` 条件性返回 execution_plan**

在 `ai_commands.rs` 中，将 `execution_plan: Some(execution_plan)` 改为仅在多步查询时返回 plan：

```rust
// 仅当存在多个子查询时才返回 execution_plan，单步查询直接执行
let execution_plan = if plan.sub_queries.len() > 1 {
    Some(crate::ai_runtime::execution_plan::execution_plan_from_context_plan(&plan))
} else {
    None
};
```

- [ ] **Step 2: 验证 `plan_context` 返回类型确认 `sub_queries` 字段存在**

读取 `src-tauri/src/ai_runtime/context_planner.rs` 确认 `ContextPlan` 结构体有 `sub_queries` 字段。

- [ ] **Step 3: 运行 Rust 测试验证**

Run: `cargo test --lib` in `src-tauri/`
Expected: PASS

- [ ] **Step 4: 运行前端 typecheck 验证**

Run: `npm run typecheck`
Expected: PASS

---

## Task 2: 修复文件夹创建/重命名 [P0]

**问题:** `handleFolderCreate` 和 `handleFolderRename` 不接受参数，只读从未更新的 state，导致文件夹创建永远 return。

**Files:**

- Modify: `src/components/file/VaultNavigator.tsx:225-254`

- [ ] **Step 1: 修复 `handleFolderCreate` 接受参数**

将 `handleFolderCreate` 改为接受 `name: string` 参数：

```typescript
const handleFolderCreate = useCallback(
  async (name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    const parentPath = newFolderParent ?? "";
    const folderPath = parentPath ? `${parentPath}${trimmed}` : trimmed;
    try {
      await folderCreate(folderPath);
      setNewFolderParent(null);
      refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "创建文件夹失败");
    }
  },
  [newFolderParent, refresh],
);
```

- [ ] **Step 2: 修复 `handleFolderRename` 接受参数**

将 `handleFolderRename` 改为接受 `newName: string` 参数：

```typescript
const handleFolderRename = useCallback(
  async (newName: string) => {
    const trimmed = newName.trim();
    if (!trimmed || !folderRenameTarget) return;
    try {
      await folderRename(folderRenameTarget, trimmed);
      setFolderRenameTarget(null);
      refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "重命名文件夹失败");
    }
  },
  [folderRenameTarget, refresh],
);
```

- [ ] **Step 3: 清理不再需要的 state**

移除 `newFolderName` 和 `folderRenameNewName` state 及其 setter（不再需要，PromptDialog 自己管理输入状态）。

- [ ] **Step 4: 运行 typecheck 验证**

Run: `npm run typecheck`
Expected: PASS

---

## Task 3: 修复打开笔记静默修改 .md [P0]

**问题:** `TipTapEditor` mount 后立即调用 `emitTitle()`，触发 `handleTitleChange` → `patchNoteTitleInMarkdown` → 标 dirty → 保存。这会在用户未做任何编辑时静默修改 .md 文件（尤其将旧 `# H1` 迁移为 frontmatter 标题）。

**Files:**

- Modify: `src/components/editor/TipTapEditor.tsx:254-282`

- [ ] **Step 1: 跳过 mount 时的 emitTitle**

在 `useEffect` 中，将 `emitTitle()` 调用移到一个初始跳过标志之后：

```typescript
useEffect(() => {
  if (!editor) return;

  let skipInitial = true; // 跳过 mount 时的自动 emit

  const emitTitle = () => {
    if (skipInitial) return;
    const next = noteTitleFromEditor(editor);
    if (next === lastEmittedTitleRef.current) return;
    lastEmittedTitleRef.current = next;
    onTitleChangeRef.current?.(next);
  };

  // 不再在 mount 时调用 emitTitle()

  const onTransaction = ({
    transaction,
  }: {
    transaction: { docChanged: boolean };
  }) => {
    if (transaction.docChanged) emitTitle();
  };

  editor.on("transaction", onTransaction);

  // 下一帧解除跳过，允许后续用户编辑触发 emit
  const frame = requestAnimationFrame(() => {
    skipInitial = false;
  });

  return () => {
    editor.off("transaction", onTransaction);
    cancelAnimationFrame(frame);
  };
}, [editor]);
```

- [ ] **Step 2: 验证 handleTitleChange 仍然正常工作**

确认用户手动编辑标题时，`emitTitle` 仍然正常触发（skipInitial 在下一帧后为 false）。

- [ ] **Step 3: 运行前端测试**

Run: `npm run test`
Expected: PASS

---

## Task 4: 修复 SSE 流解析 [P1]

**问题:** `send_streaming_request` 对每个 TCP chunk 直接 `text.lines()`，没有 carry buffer；工具调用 delta 要求所有字段同时存在；无去重。

**Files:**

- Modify: `src-tauri/src/ai_runtime/model_gateway.rs:536-673`

- [ ] **Step 1: 添加 carry buffer**

参考 `anthropic.rs` 的实现，在 `send_streaming_request` 中添加行缓冲：

```rust
let mut carry = String::new();

while let Some(chunk_result) = stream.next().await {
    let chunk =
        chunk_result.map_err(|e| AppError::msg(format!("Stream read error: {}", e)))?;

    carry.push_str(&String::from_utf8_lossy(&chunk));

    while let Some(pos) = carry.find('\n') {
        let line: String = carry.drain(..=pos).collect();
        let line = line.trim_end_matches('\n').trim_end_matches('\r');
        // ... 处理 line ...
    }
}
// 处理 carry 中剩余数据
```

- [ ] **Step 2: 修复工具调用 delta 增量累积**

将工具调用解析改为增量累积模式：

```rust
// 在循环外维护 tool_calls 累积器
let mut tool_call_deltas: std::collections::HashMap<String, ToolCallBuilder> = std::collections::HashMap::new();

// 在解析 delta 时：
if let Some(tc_deltas) = json["choices"][0]["delta"]["tool_calls"].as_array() {
    for tc_delta in tc_deltas {
        let index = tc_delta["index"].as_u64().unwrap_or(0) as usize;
        let id = tc_delta["id"].as_str();
        let name = tc_delta["function"]["name"].as_str();
        let args = tc_delta["function"]["arguments"].as_str();

        let entry = tool_call_deltas.entry(index.to_string()).or_insert_with(|| ToolCallBuilder::new());

        if let Some(id) = id { entry.id = Some(id.to_string()); }
        if let Some(name) = name { entry.name = Some(name.to_string()); }
        if let Some(args) = args { entry.arguments.push_str(args); }
    }
}
```

- [ ] **Step 3: 在流结束后组装 tool_calls 并去重**

```rust
let tool_calls: Vec<ToolCall> = tool_call_deltas
    .into_values()
    .filter_map(|b| b.build())
    .collect();
```

- [ ] **Step 4: 运行 Rust 测试**

Run: `cargo test --lib` in `src-tauri/`
Expected: PASS

---

## Task 5: 修复 tool_confirm 空实现 [P1]

**问题:** `tool_confirm` 审批后不执行任何工具，只 emit `{ executed: true }`。当前 `execute_ai_send_message` 在 line 297-315 发射 `ai:tool_confirm_request` 给前端，但不存储 pending tool call。`tool_confirm` 收到确认后无法取回原始 tool call 信息。

**Files:**

- Modify: `src-tauri/src/commands/ai_commands.rs:292-332, 437-479`
- Modify: `src-tauri/src/lib.rs` 或 `src-tauri/src/state.rs` — 添加 pending tool calls 存储

- [ ] **Step 1: 在 AppState 中添加 pending tool calls 存储**

在 `AppState` 中添加 `pending_tool_calls: Mutex<HashMap<String, PendingToolCall>>`：

```rust
pub struct PendingToolCall {
    pub tool_name: String,
    pub arguments: String,
    pub request_id: String,
}
```

- [ ] **Step 2: 在 emit confirm_request 时存储 pending tool call**

在 `execute_ai_send_message` 的 line 308-310 之前，将 tool call 存入 pending map：

```rust
// Store pending tool call for later confirmation
state.pending_tool_calls.lock().unwrap().insert(
    tool_call.id.clone(),
    PendingToolCall {
        tool_name: tool_call.function.name.clone(),
        arguments: tool_call.function.arguments.clone(),
        request_id: request_id.clone(),
    },
);
```

- [ ] **Step 3: 实现真正的 tool_confirm 执行**

```rust
#[tauri::command]
pub async fn tool_confirm(
    state: State<'_, Arc<AppState>>,
    app_handle: tauri::AppHandle,
    request_id: String,
    tool_call_id: String,
    decision: String,
    modified_args: Option<serde_json::Value>,
) -> AppResult<serde_json::Value> {
    if decision == "reject" {
        // Clean up pending
        state.pending_tool_calls.lock().unwrap().remove(&tool_call_id);
        return Ok(serde_json::json!({
            "request_id": request_id,
            "tool_call_id": tool_call_id,
            "status": "rejected",
        }));
    }

    // Retrieve pending tool call
    let pending = state.pending_tool_calls.lock().unwrap().remove(&tool_call_id);
    let Some(pending) = pending else {
        return Err(AppError::msg(format!("no pending tool call for id: {tool_call_id}")));
    };

    // Use modified args if provided, otherwise use original
    let args_str = if let Some(args) = modified_args {
        serde_json::to_string(&args).unwrap_or_default()
    } else {
        pending.arguments
    };

    // Execute the tool using the same dispatch as execute_tool_auto
    let result = execute_tool_auto(&state, AiScene::KnowledgeLookup, &pending.tool_name, &args_str).await;

    let output = match result {
        Ok(val) => serde_json::json!({
            "request_id": request_id,
            "tool_call_id": tool_call_id,
            "status": "executed",
            "output": val,
        }),
        Err(e) => serde_json::json!({
            "request_id": request_id,
            "tool_call_id": tool_call_id,
            "status": "error",
            "error": e.to_string(),
        }),
    };

    app_handle.emit("ai:tool_result", &output)
        .map_err(|e| AppError::msg(format!("failed to emit tool result: {}", e)))?;

    Ok(output)
}
```

- [ ] **Step 4: 运行 Rust 测试**

Run: `cargo test --lib` in `src-tauri/`
Expected: PASS

---

## Task 6: 统一 TokenUsage 类型 & 修复 doctest [P1]

**问题:** `TokenUsage` 定义了 3 次，字段不兼容。doctest 使用错误的 crate name 和缺失字段。

**Files:**

- Modify: `src-tauri/src/ai_runtime/mod.rs:147-166, 658-664`
- Modify: `src-tauri/src/ai_runtime/writing_workflow.rs:24-29, 41-42`
- Modify: `src-tauri/src/ai_runtime/model_gateway.rs:126-135`

- [ ] **Step 1: 保留 model_gateway.rs 的 TokenUsage 为唯一定义（5 字段版本）**

在 `mod.rs` 中删除 `TokenUsage` 定义，改为 re-export `model_gateway::TokenUsage`：

```rust
pub use model_gateway::TokenUsage;
```

- [ ] **Step 2: 删除 writing_workflow.rs 的 TokenUsage**

删除 `writing_workflow.rs` 中的 `TokenUsage` 定义，使用 `super::TokenUsage` 或 `crate::ai_runtime::TokenUsage`。

- [ ] **Step 3: 修复 doctest**

````rust
/// ```rust
/// use iris_lib::ai_runtime::{ContextPacket, SourceType, TrustLevel};
///
/// let packet = ContextPacket {
///     id: "pkt_001".to_string(),
///     source_type: SourceType::Note,
///     source_path: Some("notes/sqlite.md".to_string()),
///     title: "SQLite 入门".to_string(),
///     heading_path: None,
///     source_span: None,
///     content_hash: "abc123".to_string(),
///     excerpt: "SQLite 是一个嵌入式数据库...".to_string(),
///     retrieval_reason: "vector_chunk".to_string(),
///     score: 0.92,
///     trust_level: TrustLevel::UserNote,
///     citation_label: "[C0]".to_string(),
///     stale: false,
///     web: None,
/// };
/// assert_eq!(packet.source_type, SourceType::Note);
/// ```
````

- [ ] **Step 4: 修复 writing_workflow.rs 重复 doc comment**

删除 `generate_patch_id()` 上的重复 doc comment 行。

- [ ] **Step 5: 运行 cargo test 验证 doctest**

Run: `cargo test --doc` in `src-tauri/`
Expected: PASS

- [ ] **Step 6: 运行 cargo clippy 验证**

Run: `cargo clippy --all-targets -- -D warnings` in `src-tauri/`
Expected: PASS

---

## Task 7: 死代码清理 & 质量门禁修复 [P2]

**问题:** 约 1400 行死代码、未使用的文件、lint error、format 不一致。

**Files:**

- Delete: `src/lib/content-hash.ts`
- Delete: `src/lib/ai/prompt-display.ts`
- Delete: `src/lib/ai/packet-types.ts`
- Delete: `src/hooks/useAiPanelLlmStream.ts`
- Delete: `src/components/ai/KnowledgeInbox.tsx`
- Delete: `src/components/ai/ProfileManager.tsx`
- Delete: `src/components/ai/WorkflowIndicator.tsx`
- Delete: `src/hooks/useInlineSuggestion.ts`
- Delete: `src/components/editor/InlineSuggestion.tsx`
- Delete: `src-tauri/src/ai_runtime/eval.rs`
- Delete: `src-tauri/src/knowledge/graph.rs`
- Modify: `src-tauri/src/ai_runtime/citation_workflow.rs` — 删除死代码 `generate_suggestion_id()`
- Modify: `src-tauri/src/security/secure_delete.rs` — 移除 `#[allow(dead_code)]`（保留文件，安全工具有价值）
- Modify: `src-tauri/src/ai_runtime/mod.rs` — 移除 `pub mod eval` 声明
- Modify: `src-tauri/src/knowledge/mod.rs` — 移除 `pub mod graph` 声明
- Modify: `src-tauri/Cargo.toml` — tempfile 移到 dev-dependencies
- Modify: `src/components/settings/LlmRoutingSection.tsx:278` — 修复 lint error
- Modify: `src/components/editor/TipTapEditor.tsx` — 移除 InlineSuggestion 相关导入和渲染
- Modify: `src/App.tsx` — 如果传递了 `enableInlineSuggestion` prop 则移除
- Format: `cargo fmt --all`
- Format: `npm run format`

- [ ] **Step 1: 删除未使用的前端文件**

删除以下 9 个文件：

- `src/lib/content-hash.ts`
- `src/lib/ai/prompt-display.ts`
- `src/lib/ai/packet-types.ts`
- `src/hooks/useAiPanelLlmStream.ts`
- `src/components/ai/KnowledgeInbox.tsx`
- `src/components/ai/ProfileManager.tsx`
- `src/components/ai/WorkflowIndicator.tsx`
- `src/hooks/useInlineSuggestion.ts`
- `src/components/editor/InlineSuggestion.tsx`

- [ ] **Step 2: 删除未使用的后端文件**

删除以下 2 个文件：

- `src-tauri/src/ai_runtime/eval.rs`
- `src-tauri/src/knowledge/graph.rs`

并移除对应的 module 声明：

- `src-tauri/src/ai_runtime/mod.rs` 中的 `pub mod eval;`
- `src-tauri/src/knowledge/mod.rs` 中的 `pub mod graph;`

- [ ] **Step 3: 从 TipTapEditor.tsx 移除 InlineSuggestion 相关代码**

移除 `useInlineSuggestion` 的导入和调用，移除 `InlineSuggestion` 组件的导入和渲染。

- [ ] **Step 4: 删除 citation_workflow.rs 中的死代码**

删除 `generate_suggestion_id()` 函数（lines 33-44）和 `#[allow(dead_code)]`。

- [ ] **Step 5: 修复 secure_delete.rs**

移除 `#[allow(dead_code)]` 注解（保留文件，安全工具有价值）。

- [ ] **Step 6: 修复 Cargo.toml tempfile 依赖**

将 `tempfile = "3"` 从 `[dependencies]` 移到 `[dev-dependencies]`（如果 `[dev-dependencies]` 已有则直接删除 `[dependencies]` 中的条目）。

- [ ] **Step 7: 修复 LlmRoutingSection.tsx lint error**

将 `const { [providerId]: _removed, ...rest } = routing.providers;` 改为使用 `_` 前缀或解构不绑定变量：

```typescript
const { [providerId]: _, ...rest } = routing.providers;
```

- [ ] **Step 8: 运行 cargo fmt**

Run: `cargo fmt --all` in `src-tauri/`

- [ ] **Step 9: 运行 npm format**

Run: `npm run format`

- [ ] **Step 10: 运行完整质量门禁验证**

```bash
# 前端
npm run typecheck
npm run lint
npm run format:check
npm run test

# 后端
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Expected: 全部 PASS

---

## 数据原则说明（不改代码，仅记录）

`session_messages` 和 `knowledge_deposits` 表将 AI 对话和收件箱内容存储在 SQLite 中，且无法从 .md 文件重建。这是产品需求（AI 对话历史不是笔记），但与 AGENTS.md 中"SQLite 是缓存和索引"的定义存在张力。建议在 AGENTS.md 中补充说明：

> AI 运行时数据（session_messages、knowledge_deposits）属于应用状态而非用户笔记数据，不要求从 .md 可重建。用户笔记（files 表）仍以 .md 为唯一权威来源。

此修改在本计划中不执行，留给人类维护者决定。
