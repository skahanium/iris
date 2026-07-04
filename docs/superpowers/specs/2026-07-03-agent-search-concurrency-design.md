# Agent Search Concurrency and Evidence Pipeline Design Spec

日期：2026-07-03
状态：Ready for implementation planning
范围：AI harness 工具调度、WebEvidenceBroker、冷启动上下文、权限审计、证据注册、Subagent 定位

## 1. 背景

Iris 当前的 Agent 系统已经具备完整的 harness、工具面、权限、会话、证据链、MCP Web evidence provider 与 subagent 能力。问题不在于缺少“更多 agent”，而在于低延迟路径还没有形成清晰体系：

- 同一轮 LLM 返回多个普通工具调用时，harness 逐个串行执行。
- `web_search` 内部会为一个 query 规划多个搜索 query，但这些 planned queries 当前逐个串行执行。
- `spawn_subagent` 是完整 harness 递归调用，适合复杂问题拆解，不适合作为本地搜索与 Web 搜索并行器。
- 冷启动上下文只做本地 evidence assembly，遇到明显实时外部事实时，通常要先等 LLM 第一轮决定调用 `web_search`，再进入工具执行。

这导致联网查询的主观等待时间偏长，也让“什么时候该用 subagent、什么时候只是工具并行”这个概念边界变模糊。

本设计目标不是给现有代码打几个并发补丁，而是把 Agent 加速建立在一套清晰、可审计、可测试的执行体系上。

## 2. 目标

1. 降低 Agent 在“本地 evidence + Web evidence + 多工具调用”场景的首答延迟。
2. 保持现有功能语义：权限、确认、审计、证据注册、citation、checkpoint/resume/cancel 不被破坏。
3. 明确 subagent 与工具并行的边界：subagent 用于问题拆解与多角度验证，工具并行用于低延迟 I/O。
4. 建立工具执行分类体系，避免未来新增工具时因为默认并行而埋下安全债。
5. 保守引入冷启动 Web 预取，只处理强联网意图，不替用户或模型擅自扩大证据范围。

## 3. 非目标

- 不重写 harness。
- 不把 MCP 变成通用工具平台。
- 不让 subagent 承担普通搜索并行。
- 不新增数据库 migration、IPC、前端协议或外部依赖。
- 不绕过 `ToolPolicy`、`PermissionDecision`、写入确认、工具审计或 session evidence 注册。
- 不在日志、trace、audit 中写入 API Key、Token、用户笔记正文、完整网页正文或敏感路径。
- 不改变用户显式选择 evidence packet 的语义。

## 4. 核心设计原则

### 4.1 并行是执行属性，不是工具描述文本

是否能并行不能靠工具名散落判断，也不能靠 `requires_confirmation == false` 粗暴推断。应新增集中式工具效果分类：

```rust
pub enum ToolExecutionClass {
    ParallelRead,
    SequentialRead,
    Mutation,
    HarnessControl,
}
```

分类含义：

- `ParallelRead`：无写入副作用、无需用户确认、可并行调度，结果只影响后续 evidence ledger。
- `SequentialRead`：逻辑上只读，但依赖顺序、快照、外部限制或运行时状态，不进入并行批次。
- `Mutation`：写入文件、内存、任务、Git、导入导出、计划任务等，必须串行并遵守确认。
- `HarnessControl`：`spawn_subagent`、`conclude_reasoning` 这类 harness 自身控制工具，由 harness 专门处理。

所有 catalog 工具都必须被分类。新增工具如果没有分类，测试应失败。

### 4.2 并行批次保持原始顺序语义

harness 不应简单把所有 read-only 工具提前执行，再执行所有 write 工具。正确模型是按原始 tool call 顺序扫描：

```text
read A, read B, write C, read D
=> 并行执行 [A, B]
=> 串行处理 C
=> 执行 [D]
```

这样既能缩短独立读工具耗时，又不会改变 LLM 明确表达的读写顺序。

### 4.3 权限和审计先于并行

每个工具进入并行批次前仍必须完成：

1. `parse_tool_call_arguments`
2. `catalog_find`
3. `evaluate_tool_execution`
4. policy/permission denied 的工具直接写回 denied tool result 和 audit
5. 只有 gate 通过且分类为 `ParallelRead` 时才进入并行批次

并行只是 dispatch 方式，不是授权方式。

### 4.4 证据账本按确定性顺序合并

并行批次中的工具共享同一个 `cold_start_packets` 快照。批次完成后，按原始 tool call 顺序：

1. 写入 tool message
2. 写入 `tool_results_json`
3. emit `ToolComplete`
4. audit dispatched tool
5. ingest tool packets into `EvidenceLedger`

这样可以避免同一批次工具互相看到半更新 evidence ledger，也保证 trace、checkpoint、LLM message 顺序稳定。

### 4.5 Web 预取是 evidence optimization，不是隐式工具调用

冷启动 Web 预取只是在 harness 前补充初始 evidence packets。它不是 LLM 发起的 `web_search` 工具调用，因此：

- 不写成普通 tool call audit。
- 不出现在 assistant tool call 链里。
- 可以写入 trace/debug metadata，但必须脱敏。
- 成功 packets 进入 `EvidenceLedger` 与 `session_evidence` 注册链路，获得正常 citation label。
- 失败或超时静默降级，不让用户看到“工具失败”。

## 5. 子系统设计

### 5.1 Tool Execution Classifier

新增模块：`src-tauri/src/ai_runtime/tool_effects.rs`

职责：集中回答“这个工具是否能并行自动执行”。

初始分类建议：

`ParallelRead`：

- `search_hybrid`
- `search_semantic`
- `search_keyword`
- `get_regulation`
- `get_context_packets`
- `system_time_now`
- `app_context_read`
- `capabilities_read`
- `web_search`
- `read_note`
- `list_vault`
- `get_outline`
- `get_backlinks`
- `get_block_links`
- `memory_read`
- `scheduled_task_list`
- `vault_version_list`
- `skills_list`
- `git_read_status`
- `git_read_diff`
- `git_read_log`
- `secret_exists`
- `fs_read_authorized_folder`
- `doc_extract_citations`

`Mutation`：

- `memory_write`
- `scheduled_task_create`
- `scheduled_task_delete`
- `vault_create_note`
- `vault_rename_move`
- `vault_delete_to_trash`
- `vault_asset_write`
- `insert_text_at_cursor`
- `replace_selection`
- `fs_import_to_vault`
- `fs_export`
- `fs_write_authorized_export`
- `doc_normalize_markdown`
- `git_write_commit`

`HarnessControl`：

- `spawn_subagent`
- `conclude_reasoning`

`SequentialRead`：初始可为空，作为未来保守落点。

约束：任何 `requires_confirmation == true` 的工具不能分类为 `ParallelRead`。任何 `HARNESS_ONLY_TOOL_NAMES` 不能分类为 `ParallelRead`。

### 5.2 Harness 普通工具批次执行

改造点：`src-tauri/src/ai_harness/harness/run.rs`

现状：`other_calls` 在 `for tool_call in &other_calls` 中串行 parse、gate、dispatch、audit、message push。

目标：提取小型执行单元，保持 run.rs 可读：

- `PreparedToolCall`：保存 tool call、args、catalog entry、execution gate、permission decision。
- `ToolExecutionOutcome`：保存 result、output string、preview、status。
- `prepare_tool_call_for_execution(...)`：完成 parse、catalog、permission gate，并生成应立即写回的 parse/policy error。
- `dispatch_parallel_read_batch(...)`：对已准备好的 parallel read calls 执行 `join_all`。
- `record_tool_execution_outcome(...)`：按原始顺序统一写 messages、tool_results、trace、audit、evidence ledger。

执行算法：

```text
for each other_call in original order:
  if call requires confirmation:
    flush current parallel batch
    keep existing confirmation behavior
    continue

  prepare call: parse, catalog, permission gate
  if parse/policy error:
    flush current parallel batch
    record error result
    continue

  if max_tool_calls_per_round reached:
    flush current parallel batch
    stop

  if class == ParallelRead:
    append to current batch
  else:
    flush current parallel batch
    execute this call serially with current behavior

flush final batch
```

计数规则：与当前行为保持一致。parse error、not implemented、policy denied、实际 dispatch 都计入 `tools_this_round`。跳过确认工具不计入自动执行额度。

### 5.3 WebEvidenceBroker planned query 并行

改造点：`src-tauri/src/ai_runtime/web_evidence_broker.rs`

现状：

```rust
for planned_query in plan_search_queries(&input.query) {
    for fetch in collect_search_provider_fetches(db, &planned_query).await { ... }
}
```

目标：planned query 之间并行，provider candidates 内部现有并行保持不变。

要求：

- 输出 normalize、failure suppression、truncate、page fetch 顺序保持现有语义。
- 错误仍转为 failed evidence item，不 panic。
- `WebEvidenceUsage` 聚合逻辑保持完整。
- 保持 `max_search_results` 与 `max_fetches` 上限。

### 5.4 冷启动 Web 预取

改造点：`src-tauri/src/commands/ai_commands.rs`

新增 helper：`build_initial_context_packets(...)`

职责：在普通 agent 请求进入 harness 前构建初始 packets：

1. 本地 context packets 仍使用 `build_context_packets_cached`。
2. 在满足强条件时，同时启动 Web evidence prefetch。
3. 合并 local + web packets 后交给 `EvidenceLedger::new`。

触发条件必须全部满足：

- 当前请求 `web_search == true`。
- policy 层允许 web evidence capability。
- `selected_packet_ids` 为空，用户没有显式锁定证据包。
- `should_prefetch_web(message)` 返回 true。

`should_prefetch_web` 的 v1 强信号：

- 包含 `http://` 或 `https://`。
- 中文：`联网`、`搜索`、`网页`、`最新`、`近期`、`今天`、`现在`、`当前`、`新闻`、`时事`。
- 英文：`web`、`search`、`online`、`latest`、`recent`、`today`、`current`、`news`、`2025`、`2026`。

不作为 v1 预取信号：`什么是`、`谁是`、`对比`、`compare`、`who is`、`what is`。这些可能是本地知识库问题，应交给 LLM 第一轮和工具层并行处理。

超时和降级：

- Web 预取 timeout：10 秒。
- 超时、provider missing、provider error、empty result：返回 local packets，不中断请求。
- 预取结果最多取 `max_search_results = 8`，`max_fetches` 采用 task policy 当前 `max_fetch_per_round`，但不超过 3。

### 5.5 Evidence 与 session 注册

冷启动 Web packets 合并后必须进入现有路径：

```text
packets -> EvidenceLedger::new -> selected packet filter -> register_packets_from_context_packets -> register_session_evidence -> citation_label backfill
```

重要边界：如果用户传入 `selected_packet_ids`，不要预取，不要向 filtered packets 添加新 evidence。显式 evidence selection 优先级高于自动补全。

### 5.6 Subagent 定位

保留现有 subagent 实现：

- 完整 harness 递归调用。
- 最大 depth 限制。
- write-write conflict detection。
- 子任务 token budget。
- `SubagentReport` 输出。

补充定位文档或 prompt 描述：

- 适合：多角度研究、复杂问题拆解、A/B 对比、独立验证、跨文档综合。
- 不适合：单纯为了同时跑本地搜索和 Web 搜索。
- 搜索并行由工具层和 Web broker 负责。

## 6. 权限与安全模型

### 6.1 权限不因并行放宽

并行前后，工具授权路径不变：

```text
ToolCatalogEntry -> ToolPolicyContext -> PermissionDecision -> ToolExecutionGate -> Dispatch
```

任何 denied 工具都不能进入并行 future。任何 requires confirmation 工具都不能被自动 dispatch。

### 6.2 写入工具保持串行

写入工具涉及 `.md` 文件、vault、memory、Git、导入导出、计划任务。它们必须继续串行，因为需要：

- 用户确认。
- CAS/hash conflict 检查。
- 版本快照。
- 回收站或索引刷新。
- 工具 audit 与 permission audit 的确定顺序。

### 6.3 Web 预取不绕过联网授权

没有 `web_search` 授权时，不预取。预取不应创建新的隐式联网权限通道。

### 6.4 日志与审计脱敏

新增 trace 或 debug 字段只能记录：

- 是否触发 prefetch。
- timeout / success / degraded 的状态。
- duration_ms。
- packet count。
- provider id 的非敏感标识。

不能记录完整 query、完整 URL、网页正文、headers、tokens、API key 或用户笔记正文。

## 7. 可观测性

不新增数据库 schema。利用现有 trace phase 和 audit：

- 工具批次内仍对每个工具发出 `ToolStart` 与 `ToolComplete`。
- 可在 `status` 或 `message` 中标记 `parallel_batch`，但不要破坏前端现有展示。
- 冷启动 Web 预取可用 tracing debug/warn 记录脱敏统计，也可在 context status 内部附带非公开元信息。

## 8. 兼容性

- LLM message 格式保持不变：每个 tool call 仍有一个对应 tool message。
- checkpoint/resume 保持不变：保存的是已经按顺序写入的 messages、tool_calls、tool_results、evidence_packets。
- 前端无需协议变更。
- 失败降级回现有串行/本地路径，不影响用户功能。

## 9. 风险与缓解

| 风险 | 缓解 |
| --- | --- |
| 并行读工具改变 evidence 可见顺序 | 批次内使用同一 cold_start 快照，完成后按原始顺序 ingest |
| 误把写工具分类为并行 | 分类测试强制 `requires_confirmation` 工具不得并行，catalog 覆盖测试防漏 |
| Web 预取误触发浪费时间 | 只使用强联网信号，10 秒 timeout，失败静默降级 |
| MCP provider 限流 | planned query 并发数量仅为 `plan_search_queries` 规模，保留现有 provider/fetch 上限 |
| 审计顺序不稳定 | dispatch 可并行，record 必须按原始 call 顺序 |
| 用户手选 evidence 被自动扩展 | 有 `selected_packet_ids` 时完全跳过预取 |

## 10. 验收标准

- 多个只读工具同轮调用时能并行 dispatch，并按原始顺序写回 tool messages。
- 读-写-读混合调用中，写工具仍保持确认/串行屏障。
- `web_search=false` 时不会预取，也不会 dispatch `web_search`。
- planned queries 并行后，Web evidence 输出 shape 与现有 tool response 兼容。
- 预取成功的 Web packets 能正常进入 citation 和 session evidence。
- 预取失败不会中断普通回答。
- subagent 功能无回归。
- `cargo test`、`cargo clippy --all-targets -- -D warnings`、必要前端类型检查通过。
