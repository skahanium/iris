# Agent Search Concurrency and Evidence Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` for independent tasks or `superpowers:executing-plans` for inline execution. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Iris Agent 的搜索与工具执行加速建立为清晰的工具分类、批次调度、Web evidence 并行与保守冷启动预取体系，同时保持权限、确认、审计、证据链和 subagent 语义不变。

**Architecture:** 新增集中式 tool effects 分类，harness 按原始 tool call 顺序构造连续只读批次并行执行；WebEvidenceBroker 并行 planned queries；AI command 冷启动阶段在强联网意图下并行拉取 Web evidence，并通过现有 EvidenceLedger 和 session_evidence 路径注册。

**Tech Stack:** Rust 2021, Tauri 2.x, Tokio, futures-util, SQLite/rusqlite, existing Iris AI harness, existing cargo test/clippy toolchain.

---

## Task 1: 建立工具执行分类体系

**Files:**

- Create: `src-tauri/src/ai_runtime/tool_effects.rs`
- Modify: `src-tauri/src/ai_runtime/mod.rs`
- Test: unit tests in `tool_effects.rs`

- [ ] **Step 1: 写分类测试**

在 `tool_effects.rs` 中先写测试，覆盖这些断言：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::tool_catalog::TOOL_CATALOG;

    #[test]
    fn every_catalog_tool_has_execution_class() {
        for entry in TOOL_CATALOG.iter() {
            let class = tool_execution_class(entry.name);
            assert_ne!(class, ToolExecutionClass::Unknown, "{}", entry.name);
        }
    }

    #[test]
    fn confirmation_tools_are_never_parallel_reads() {
        for entry in TOOL_CATALOG.iter() {
            if entry.requires_confirmation {
                assert_ne!(tool_execution_class(entry.name), ToolExecutionClass::ParallelRead);
            }
        }
    }

    #[test]
    fn harness_control_tools_are_not_parallel_reads() {
        assert_eq!(tool_execution_class("spawn_subagent"), ToolExecutionClass::HarnessControl);
        assert_eq!(tool_execution_class("conclude_reasoning"), ToolExecutionClass::HarnessControl);
    }

    #[test]
    fn known_search_tools_are_parallel_reads() {
        for name in ["search_hybrid", "search_semantic", "search_keyword", "web_search"] {
            assert_eq!(tool_execution_class(name), ToolExecutionClass::ParallelRead, "{name}");
            assert!(is_parallel_read_tool(name));
        }
    }
}
```

Run: `cargo test tool_effects --manifest-path src-tauri/Cargo.toml`

Expected: fail because module does not exist.

- [ ] **Step 2: 实现 `ToolExecutionClass` 与分类函数**

实现：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionClass {
    ParallelRead,
    SequentialRead,
    Mutation,
    HarnessControl,
    Unknown,
}

pub fn is_parallel_read_tool(name: &str) -> bool {
    tool_execution_class(name) == ToolExecutionClass::ParallelRead
}

pub fn tool_execution_class(name: &str) -> ToolExecutionClass {
    match name {
        "search_hybrid" | "search_semantic" | "search_keyword" | "get_regulation"
        | "get_context_packets" | "system_time_now" | "app_context_read"
        | "capabilities_read" | "web_search" | "read_note" | "list_vault"
        | "get_outline" | "get_backlinks" | "get_block_links" | "memory_read"
        | "scheduled_task_list" | "vault_version_list" | "skills_list"
        | "git_read_status" | "git_read_diff" | "git_read_log" | "secret_exists"
        | "fs_read_authorized_folder" | "doc_extract_citations" => ToolExecutionClass::ParallelRead,

        "memory_write" | "scheduled_task_create" | "scheduled_task_delete"
        | "vault_create_note" | "vault_rename_move" | "vault_delete_to_trash"
        | "vault_asset_write" | "insert_text_at_cursor" | "replace_selection"
        | "fs_import_to_vault" | "fs_export" | "fs_write_authorized_export"
        | "doc_normalize_markdown" | "git_write_commit" => ToolExecutionClass::Mutation,

        "spawn_subagent" | "conclude_reasoning" => ToolExecutionClass::HarnessControl,

        _ => ToolExecutionClass::Unknown,
    }
}
```

- [ ] **Step 3: 导出模块**

在 `src-tauri/src/ai_runtime/mod.rs` 中加入：

```rust
pub mod tool_effects;
```

- [ ] **Step 4: 验证**

Run: `cargo test tool_effects --manifest-path src-tauri/Cargo.toml`

Expected: pass.

## Task 2: 提取 harness 普通工具执行记录单元

**Files:**

- Modify: `src-tauri/src/ai_harness/harness/run.rs`
- Test: existing harness tests, plus focused unit tests if helpers are pure enough

- [ ] **Step 1: 在 `run.rs` 内新增小型结构体**

在 tests 前、helper 函数区附近新增内部结构：

```rust
struct PreparedToolExecution<'a> {
    tool_call: &'a ToolCall,
    tool_name: &'a str,
    args: serde_json::Value,
    entry: &'a crate::ai_runtime::tool_catalog::ToolCatalogEntry,
    gate: crate::ai_runtime::tool_execution_pipeline::ToolExecutionGate<'a>,
    decision: crate::ai_runtime::permission_decision::PermissionDecisionOutcome,
}

struct CompletedToolExecution<'a> {
    prepared: PreparedToolExecution<'a>,
    result: crate::ai_runtime::ToolCallResult,
}
```

如果生命周期让代码过重，可以让结构体持有 `tool_call: ToolCall`、`tool_name: String`，优先保证清晰和编译稳定。

- [ ] **Step 2: 提取 record helper**

把当前串行循环中 dispatch 之后的重复逻辑提取为：

```rust
fn record_completed_tool_execution(
    state: &AppState,
    app_handle: &AppHandle,
    input: &HarnessRunInput,
    harness_rounds: u32,
    messages: &mut Vec<LlmMessage>,
    tool_results_json: &mut Vec<serde_json::Value>,
    evidence_ledger: &mut EvidenceLedger,
    execution: &CompletedToolExecution<'_>,
) -> AppResult<()> { ... }
```

该 helper 必须保留现有行为：

- success 时 `ingest_tool_packets`。
- output serialization 失败时回 `{}`。
- error 时 tool message content 仍是 `{ "error": ... }` shape。
- emit `HarnessPhase::ToolComplete`。
- 调用 `audit_dispatched_tool`。
- push `tool_results_json`。

- [ ] **Step 3: 先用 helper 替换原串行路径**

不引入并行，只替换原循环的记录逻辑。

Run: `cargo test harness --manifest-path src-tauri/Cargo.toml`

Expected: pass.

## Task 3: 实现连续只读工具批次并行

**Files:**

- Modify: `src-tauri/src/ai_harness/harness/run.rs`
- Use: `crate::ai_runtime::tool_effects::is_parallel_read_tool`

- [ ] **Step 1: 写批次顺序测试**

在能直接测试 helper 的位置新增纯函数测试。若需要，先提取：

```rust
fn split_parallel_read_segments<'a>(calls: &'a [ToolCall]) -> Vec<Vec<&'a ToolCall>>
```

测试：

```rust
#[test]
fn read_write_read_calls_form_separate_read_batches() {
    let calls = vec![
        ToolCall::new("a", "search_hybrid", "{}"),
        ToolCall::new("b", "web_search", r#"{"query":"x"}"#),
        ToolCall::new("c", "replace_selection", "{}"),
        ToolCall::new("d", "read_note", r#"{"path":"a.md"}"#),
    ];
    let segments = split_parallel_read_segments(&calls);
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].iter().map(|tc| tc.id.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    assert_eq!(segments[1].iter().map(|tc| tc.id.as_str()).collect::<Vec<_>>(), vec!["d"]);
}
```

- [ ] **Step 2: 新增 pending batch 容器**

在原 `for tool_call in &other_calls` 循环附近加入：

```rust
let mut pending_parallel_reads: Vec<PreparedToolExecution<'_>> = Vec::new();
```

并新增 `flush_parallel_reads(...)` async helper，内部用 `join_all` dispatch，返回后按 pending 顺序调用 `record_completed_tool_execution`。

- [ ] **Step 3: 扫描循环改为 barrier 模型**

规则：

- 遇到 `registry.requires_confirmation(...)`：先 flush，再 `continue`。
- 达到 `max_tool_calls_per_round`：先 flush，再 `break`。
- parse error / catalog missing / permission denied：先 flush，再按当前错误路径写回。
- `is_parallel_read_tool(tool_name)`：准备后加入 pending batch，不立即 await。
- 其他工具：先 flush，再按当前串行 dispatch。

- [ ] **Step 4: 验证消息顺序**

补测试或人工断言：同一轮中 `web_search` 比 `search_hybrid` 先完成，也必须按原 tool call 顺序 push tool messages。

Run: `cargo test harness --manifest-path src-tauri/Cargo.toml`

Expected: pass.

## Task 4: 并行化 WebEvidenceBroker planned queries

**Files:**

- Modify: `src-tauri/src/ai_runtime/web_evidence_broker.rs`

- [ ] **Step 1: 提取 query fetch 聚合 helper**

新增私有 async helper：

```rust
async fn collect_search_fetches_for_planned_queries(
    db: &Database,
    queries: Vec<String>,
) -> Vec<Result<SearchProviderFetch, String>> {
    let futures = queries.into_iter().map(|query| async move {
        collect_search_provider_fetches(db, &query).await
    });
    join_all(futures).await.into_iter().flatten().collect()
}
```

- [ ] **Step 2: 替换串行循环**

将 `collect_web_evidence_with_usage` 中 planned query 双层循环替换为：

```rust
let planned_queries = plan_search_queries(&input.query);
for fetch in collect_search_fetches_for_planned_queries(db, planned_queries).await {
    match fetch { ... existing body ... }
}
```

保留 match 体和后续 normalize/truncate/page fetch。

- [ ] **Step 3: 测试现有 broker 行为**

Run: `cargo test web_evidence_broker --manifest-path src-tauri/Cargo.toml`

Expected: pass.

## Task 5: 增加保守冷启动 Web 预取

**Files:**

- Modify: `src-tauri/src/commands/ai_commands.rs`

- [ ] **Step 1: 添加 `should_prefetch_web` 测试**

在 `ai_commands.rs` tests 中加入：

```rust
#[test]
fn web_prefetch_requires_strong_web_intent() {
    assert!(should_prefetch_web("请联网搜索最新 Rust 版本"));
    assert!(should_prefetch_web("https://example.com 这篇文章说了什么"));
    assert!(should_prefetch_web("latest OpenAI model news"));
    assert!(!should_prefetch_web("什么是知识管理"));
    assert!(!should_prefetch_web("对比我笔记里的 A 和 B"));
}
```

- [ ] **Step 2: 实现 `should_prefetch_web`**

实现大小写归一化和强信号匹配。中文直接 `contains`，英文使用 lower-case。

- [ ] **Step 3: 新增初始上下文 helper**

新增：

```rust
#[allow(clippy::too_many_arguments)]
async fn build_initial_context_packets(
    state: &AppState,
    vault: &Path,
    scene: AiScene,
    note_path: Option<&str>,
    file_id: Option<i64>,
    message: &str,
    user_scope: &ContextScopeDto,
    build_opts: ContextBuildOptions,
    web_search: bool,
    selected_packet_ids: Option<&[String]>,
    max_web_fetches: usize,
) -> AppResult<(Vec<crate::ai_runtime::ContextPacket>, crate::ai_runtime::ContextStatus)> { ... }
```

逻辑：

- local path 调用 `build_context_packets_cached`。
- 如果不满足预取条件，直接返回 local。
- 满足时用 `tokio::join!` 并行 local 与 `tokio::time::timeout(Duration::from_secs(10), collect_web_evidence_with_usage(...))`。
- Web 成功时用 `web_evidence_items_to_packets(message, &output.items)` 转 packets 并 extend。
- Web 失败或超时仅 tracing debug/warn，不返回 error。

- [ ] **Step 4: 替换 agent 请求里的 context build**

在当前 `build_context_packets_cached(...)` 调用点替换为 `build_initial_context_packets(...).await`。

注意：`selected_packet_ids` 非空时传入 helper 并跳过预取。

- [ ] **Step 5: 验证 session evidence 注册链路**

确认合并后的 packets 仍进入：

```rust
EvidenceLedger::new(packets)
register_packets_from_context_packets(&filtered_packets)
register_session_evidence(...)
```

不得在 helper 内直接写 session evidence，避免双写。

Run: `cargo test ai_commands --manifest-path src-tauri/Cargo.toml`

Expected: pass.

## Task 6: 补充 subagent 定位说明

**Files:**

- Modify: `src-tauri/src/ai_runtime/tool_catalog/read.rs`
- Optional Modify: `src-tauri/src/ai_runtime/environment.rs`

- [ ] **Step 1: 调整 `spawn_subagent` description**

将描述明确为：

```text
将复杂子问题委派给独立 agent 并行研究。适用于多角度论证、复杂任务拆解和交叉验证；不用于普通搜索加速，普通本地/Web 搜索并行由工具层处理。
```

- [ ] **Step 2: 不改变 schema 与权限**

不要改 `input_schema`、`ToolImplementationStatus::HarnessOnly`、depth policy 或 coordinator。

Run: `cargo test tool_policy --manifest-path src-tauri/Cargo.toml`

Expected: pass.

## Task 7: 完整验证

**Files:** no source changes unless tests expose a real regression.

- [ ] **Step 1: Rust tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all tests pass.

- [ ] **Step 2: Rust clippy**

Run:

```powershell
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: no warnings.

- [ ] **Step 3: TypeScript typecheck**

Run:

```powershell
npm run typecheck
```

Expected: no TypeScript errors. This change should not touch IPC/types, but run it to catch accidental front-end impact.

- [ ] **Step 4: Manual smoke checklist**

Check in app:

- 本地普通提问仍能回答。
- `web_search=false` 时不会联网。
- 明确“联网搜索最新...”时首轮上下文能带入 Web evidence 或静默降级。
- 同一轮 search + web_search 工具调用不阻塞成串行等待。
- 写入工具仍弹确认。
- subagent 仍能在复杂任务中生成 `SubagentReport`。

## Task 8: 文档与交付说明

**Files:**

- Optional Modify: `docs/README.md`
- Optional Modify: `ARCHITECTURE.md`

- [ ] **Step 1: 只在实现完成后更新索引**

若实现落地，给 `docs/README.md` 的 AI 专题加入 spec/plan 链接。

- [ ] **Step 2: 不修改 ROADMAP 排期承诺**

本工作属于 v1.2.3-alpha 内部架构质量提升，不在 ROADMAP 新增版本承诺，除非 maintainer 明确要求。

- [ ] **Step 3: 提交信息**

建议 commit：

```text
feat(ai): 并行化 Agent 只读工具与联网证据预取
```

## Implementation Notes

- 不使用 `apply_patch`。
- 不新建 worktree，除非用户审批。
- 不新增依赖。
- 优先小步提交：分类体系、harness 调度、broker 并行、冷启动预取、文档分别提交。
- 如果并行调度导致生命周期或 borrow 复杂度过高，优先使用 owned struct 降低 Rust 生命周期技术债，不要为了省 clone 写难维护的借用迷宫。
