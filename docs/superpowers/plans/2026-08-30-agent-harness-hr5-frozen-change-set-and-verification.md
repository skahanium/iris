# Agent Harness HR-5 冻结变更集与确认后验证 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不新增数据库表、写入引擎或 Provider 的前提下，使现有 Durable Apply 能一次冻结并确认至多 6 个有序操作/6 个文件，安全地顺序执行、可恢复，并在成功后进行严格受限的本地只读验证。

**Architecture:** 扩展已有 `FrozenChangePlan`，使一个确认记录承载有序操作列表；继续使用现有 `agent_run_confirmations`、`agent_run_steps`、Run 事件与 `NormalRunToolExecutor`，而不是引入第二个事务/执行状态机。模型仍负责确认前规划，Host 负责冻结、哈希链、顺序执行、权限与恢复；确认后的模型只能在冻结目标上调用 `read_note`，其预算由同一份 `RunBudgetPolicy` 冻结。

**Tech Stack:** Rust、Tauri 2、SQLite/rusqlite、serde/serde_json、Tokio、现有 Rust 单元与命令层测试；不新增依赖。

## Global Constraints

- 不创建 worktree，不连接真实 Provider，不新增数据库表、迁移、IPC 字段、Provider 或依赖。
- 保持 `.md` 为权威笔记来源；确认前后均不得绕过既有权限、审计、Vault 路径或文档策略检查。
- 旧版单操作计划、单阶段 checkpoint 与已存 Run 必须可读取；新计划写入 schema v2，旧数据只读兼容。
- 新计划最多 6 个有序操作、最多 6 个规范化目标文件；同一 `tool_call_id` 不得重复。
- 确认后绝不执行 Web、外部、runtime 或新的写入；最多 2 次模型、4 次 `read_note`，且路径必须等于被冻结目标。
- 每项行为先写可观察的失败回归并确认 RED，再写最小实现使其 GREEN；不以 fixture ID 偶合替代真实断言。
- 所有用户可见文案保持中文、陈述事实；部分执行必须报告已执行与未执行的操作，绝不声称整组已完成。

---

## 文件职责与变更边界

| 文件                                                                                                  | 职责                                                                         |
| ----------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| `src-tauri/src/ai_runtime/frozen_change_plan.rs`                                                      | v2 有序操作载体、确定性哈希、v1 读取兼容、6/6 与哈希链验证。                 |
| `src-tauri/src/ai_runtime/frozen_change_plan_tests.rs`                                                | 计划语义的纯单元回归。                                                       |
| `src-tauri/src/ai_runtime/agent_tool_loop.rs`                                                         | 在一个模型轮次内识别并原子提交确认型工具批次；不执行其中任何写入。           |
| `src-tauri/src/ai_runtime/agent_tool_loop_tests.rs`                                                   | 多确认调用只产生一个确认批次、混合调用拒绝、预算行为。                       |
| `src-tauri/src/ai_runtime/run_tool_loop.rs`                                                           | 复用现有执行器完成批量预检、冻结、按序执行和目标限定只读验证。               |
| `src-tauri/src/ai_runtime/agent_run_repository.rs`                                                    | 将 checkpoint 扩展为可恢复的操作游标，继续使用既有持久化表。                 |
| `src-tauri/src/ai_runtime/run_engine/mod.rs`                                                          | 为确认后验证注入较小的同类 ToolLoop 预算，并保持普通终局与严格事实终局分离。 |
| `src-tauri/src/ai_runtime/run_engine/recovery.rs`                                                     | 根据操作前缀的基准/预期哈希分类恢复、绝不重放已执行前缀。                    |
| `src-tauri/src/commands/assistant_commands.rs`                                                        | 把确认消费、批量执行、部分结果、确认后验证和终态化串接到已有命令生命周期。   |
| `src-tauri/src/ai_runtime/run_engine_tests.rs`、`src-tauri/src/commands/assistant_commands.rs` 内测试 | 多文件成功、第二项漂移、重启恢复、重复确认和验证越权的集成回归。             |
| `agent-harness/*.md`、`ARCHITECTURE.md`、`ROADMAP.md`                                                 | 仅在实现和测试通过后，把已部署 HR-5 事实、边界与证据同步到唯一文档入口。     |

## Task 1: 定义可兼容的冻结变更集合同

**Files:**

- Modify: `src-tauri/src/ai_runtime/frozen_change_plan.rs`
- Test: `src-tauri/src/ai_runtime/frozen_change_plan_tests.rs`

**Consumes:** 既有扁平 `FrozenChangePlanInput` 和数据库中 v1 `plan_json`。

**Produces:** 兼容旧扁平 `FrozenChangePlanInput` 的 `FrozenChangeOperationInput`、`FrozenChangeSetInput`、`FrozenChangeOperation` 与 `plan.operations()`；所有新执行者只通过该合同读取操作。

- [ ] **Step 1: 写入 v2 有序性与上限的失败测试。**

  在 `frozen_change_plan_tests.rs` 用两个 `insert_text_at_cursor` 操作构造一份计划，断言 `operations().len() == 2`、操作与 `tool_call_id` 顺序保持输入顺序、`relative_paths()` 是稳定去重后的全局目标；再构造第 7 个操作、第 7 个不同文件和重复 `tool_call_id`，各断言 `InvalidChangePlan`。测试输入必须为真实的 `baseContentHashes`/`expectedPostContentHashes`，不得复用相同 fixture ID 伪装多操作。

- [ ] **Step 2: 执行 RED。**

  Run: `cargo test frozen_change_plan_tests --lib -- --nocapture`

  Expected: 新测试因没有 `operations()`/v2 输入或未拒绝 7/重复操作而失败；已有单操作测试仍可编译。

- [ ] **Step 3: 最小实现 v2 冻结合同。**

  保留旧 `FrozenChangePlanInput` 的读取与哈希兼容，并为新计划增加：

  ```rust
  pub(crate) struct FrozenChangeOperationInput {
      pub(crate) tool_call_id: String,
      pub(crate) operation: String,
      pub(crate) relative_paths: Vec<String>,
      pub(crate) base_content_hashes: Vec<(String, String)>,
      pub(crate) expected_post_content_hashes: Vec<(String, String)>,
      pub(crate) change: Value,
      pub(crate) rollback_summary: String,
  }

  pub(crate) struct FrozenChangeSetInput {
      pub(crate) confirmation_id: String,
      pub(crate) run_id: String,
      pub(crate) session_id: i64,
      pub(crate) request_id: String,
      pub(crate) vault_id: String,
      pub(crate) operations: Vec<FrozenChangeOperationInput>,
      pub(crate) expires_at_unix_ms: i64,
  }
  ```

  `freeze` 必须验证 `1..=6` 操作、每项非空/哈希规则、`tool_call_id` 唯一、全局路径规范化后为 `1..=6`，并计算 canonical JSON 哈希。新持久化 JSON 形状为 `schemaVersion: 2`、公共身份字段、`operations`、`affectedFileCount`、`expiresAtUnixMs`；数组顺序不得 canonical-sort。`from_persisted_plan_json` 必须识别无 `schemaVersion` 的旧扁平 JSON，并转换成一项 v2 内存表示后重算旧格式哈希，确保已批准 v1 `plan_hash` 仍可校验。禁止放宽旧计划原有验证。

- [ ] **Step 4: 写入同路径哈希链与旧计划兼容的失败测试。**

  添加：同一文件的第二项 `base` 不等于前一项 `expected` 时拒绝；相等时接受；用当前测试里的旧扁平 JSON 重新水合后断言其 hash 与旧 hash 一致、`operations().len() == 1`。覆盖 `application://` 非文件目标仅在原先允许空哈希的操作类型保持兼容，不能让 Markdown 写入逃过哈希验证。

- [ ] **Step 5: 最小实现哈希链与读取兼容。**

  在冻结验证中按操作顺序维护 `BTreeMap<path, expected_hash>`：再次触及同一路径时，下一项 base 必须等于已登记 expected；不同路径仍绑定首次读到的 base。为执行和恢复提供：

  ```rust
  pub(crate) fn operations(&self) -> &[FrozenChangeOperation];
  pub(crate) fn all_base_content_hashes(&self) -> Vec<(String, String)>;
  pub(crate) fn all_expected_post_content_hashes(&self) -> Vec<(String, String)>;
  ```

  旧扁平 JSON 在读取时转换为单项 `operations()`，但不保留“第一项”访问器，以防新代码重新依赖单操作语义。

- [ ] **Step 6: 执行 GREEN 与定向格式检查。**

  Run: `cargo test frozen_change_plan_tests --lib -- --nocapture && cargo fmt --all -- --check`

  Expected: 全部通过；失败测试在实现前确实失败且不含未使用项。

## Task 2: 让同一模型轮次原子请求一个确认批次

**Files:**

- Modify: `src-tauri/src/ai_runtime/agent_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Test: `src-tauri/src/ai_runtime/agent_tool_loop_tests.rs`
- Test: `src-tauri/src/ai_runtime/run_tool_loop.rs` 内测试模块

**Consumes:** `FrozenChangePlan::operations()` 与既有 `ToolSpec::requires_confirmation`。

**Produces:** `ToolLoopExecutor::request_change_set`，其只做预检/持久化确认；模型的多个确认型工具调用转成一个 `ConfirmationRequired` 事件。

- [ ] **Step 1: 写入模型轮次批量确认的失败测试。**

  在 `agent_tool_loop_tests.rs` 创建 provider，它在第一轮返回两个 `requires_confirmation: true` 的不同写工具调用。断言 executor 收到一整个有序切片、普通 `execute` 从未被调用、循环停止且没有第二个 provider turn。再写“一个确认写调用与一个普通读取混在同一 response”回归，断言没有任何调用被执行或冻结，且返回稳定的 `mixed_confirmation_batch` 安全错误。

- [ ] **Step 2: 执行 RED。**

  Run: `cargo test agent_tool_loop_tests:: --lib -- --nocapture`

  Expected: 测试因 trait 没有批次入口、循环仍逐一执行而失败。

- [ ] **Step 3: 最小扩展通用循环而不写入领域分支。**

  为 `ToolLoopExecutor` 增加：

  ```rust
  fn request_change_set<'a>(
      &'a self,
      run_id: &'a str,
      calls: &'a [ToolCall],
      first_step: u32,
  ) -> Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'a>>;
  ```

  在每个模型 response 校验工具 surface、schema、预算和重复限制后，若出现确认型调用：只有“该轮全部工具调用均需确认”才调用一次 `request_change_set`，否则 fail closed；确认型调用数量计入既有 `max_confirmed_change_calls` 与总工具预算。返回 `CONFIRMATION_PENDING_ERROR`，保持 Run 处于 `AwaitingConfirmation`，不得再做模型调用或派发任何写入。

- [ ] **Step 4: 写入 `NormalRunToolExecutor` 的失败回归。**

  用真实 catalog 的两个 `replace_selection` 调用和同一个 `AssistantRunAccepted`，断言：产生一个 confirmation、其 JSON 有两项、只有一个 `ConfirmationRequired`，事件中两个 `ToolStarted` 的顺序与模型调用顺序一致，磁盘内容在确认前完全不变。另测 7 个调用被拒绝且没有 pending confirmation。

- [ ] **Step 5: 最小实现批次预检和冻结。**

  `NormalRunToolExecutor::request_change_set` 必须按输入顺序对每项做现有的 catalog、参数、权限、文档策略和 `ToolExecutionGate` 检查，并记录各自 `ToolStarted`。仅当所有项都可冻结时，使用一个确认 ID 构造一份 `FrozenChangePlan`，调用已有 `request_frozen_confirmation` 一次，摘要明确“将按顺序修改 N 个目标、执行 M 项操作”，并为每项写审计记录。失败时不得产生 pending confirmation；若已写入 `ToolStarted`，以各项失败完成事件闭合其生命周期。

- [ ] **Step 6: 执行 GREEN。**

  Run: `cargo test agent_tool_loop_tests:: run_tool_loop::tests:: --lib -- --nocapture`

  Expected: 批量确认测试及原有单操作确认回归全部通过。

## Task 3: 顺序执行、精确停顿和部分结果报告

**Files:**

- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/agent_run_repository.rs`
- Modify: `src-tauri/src/commands/assistant_commands.rs`
- Test: `src-tauri/src/commands/assistant_commands.rs` 内测试
- Test: `src-tauri/src/ai_runtime/run_tool_loop.rs` 内测试模块

**Consumes:** 已批准的 v2 `FrozenChangePlan` 和现有 `agent_run_steps` checkpoint。

**Produces:** 已成功/失败的有序 `ToolCallResult` 列表和事实性终态摘要；每项 dispatch 前重验，并用同一 checkpoint 表记录可恢复游标。

- [ ] **Step 1: 写入顺序成功与第二项漂移的失败命令层回归。**

  使用临时 vault 创建 `a.md`、`b.md`，冻结两项更新并走真实 `assistant_run_start → confirmation approve`。成功断言两个文件按计划改写、两条 ToolCompleted 按序出现、最终文案列出 2/2。漂移测试在第一项成功后改写 `b.md`，断言第一项保留、第二项从未 dispatch、Run 的终态报告为“已执行 1/2；第 2 项未执行，目标已变化”，不得写“已执行你确认的变更”。

- [ ] **Step 2: 执行 RED。**

  Run: `cargo test assistant_commands::tests::confirmed --lib -- --nocapture`

  Expected: 新测试因执行器只读取/执行计划第一项、无部分报告或 checkpoint 游标而失败。

- [ ] **Step 3: 扩展 checkpoint，但不改表结构。**

  把 `DurableApplyCheckpoint` 写为 schema v2，并增加 `next_operation_index: usize` 与 `operation_count: usize`。旧 schema v1 反序列化为 `0/1`；v2 验证 `operation_count in 1..=6` 且 `next_operation_index <= operation_count`。`append_checkpoint_step` 只允许：

  ```text
  Approved(0) -> Dispatching(0) -> Applied(1)
  Applied(i)  -> Dispatching(i) -> Applied(i + 1)
  Applied(M)  -> Completed(M)
  ```

  checkpoint 绑定必须比较计划 hash、全部有序 hash 身份与当前游标；恢复入口只允许未完成的 `Approved`、`Dispatching` 或 `Applied(i < M)`。

- [ ] **Step 4: 最小实现顺序执行报告。**

  用 `execute_confirmed_frozen_change_set(&FrozenChangePlan)` 替代单操作执行：对 `operations().iter().enumerate().skip(next_operation_index)`，先对**该操作**的 base hash、工具名称、参数路径、权限、文档策略和 checkpoint 游标重验，随后记 `Dispatching(i)`、dispatch、记 `Applied(i+1)` 与对应 ToolCompleted。任何重验失败/派发失败都停止后续操作，返回包含已完成前缀和安全 reason 的报告，不回滚已完成前缀，不自动尝试剩余项。

  `assistant_commands` 必须仅对“没有写入成功的不可恢复错误”走现有失败终态；有已完成前缀的结果必须走事实性完成路径，持久化明确部分结果，避免 UI 显示无限运行或全成功谎报。重复确认必须复用已消费 confirmation/checkpoint，绝不重复 dispatch。

- [ ] **Step 5: 执行 GREEN。**

  Run: `cargo test assistant_commands::tests::confirmed run_tool_loop::tests::confirmed --lib -- --nocapture`

  Expected: 成功、漂移、重复确认和既有单操作执行回归均通过。

## Task 4: 进程重启时按已执行前缀恢复而非重放

**Files:**

- Modify: `src-tauri/src/ai_runtime/run_engine/recovery.rs`
- Modify: `src-tauri/src/ai_runtime/agent_run_repository.rs`
- Test: `src-tauri/src/ai_runtime/run_engine_tests.rs`

**Consumes:** v2 checkpoint 游标、每项有序 base/expected 哈希和已有 Durable Apply recovery。

**Produces:** 前缀感知的 `ResumeAvailable`、`AlreadyApplied`、`ManualReview` 分类；旧单操作恢复保持不变。

- [ ] **Step 1: 写入前缀恢复的失败测试。**

  构造两项计划及 checkpoint `Applied(next_operation_index=1)`：
  1. `a.md` 等于第一项 expected、`b.md` 等于第二项 base，重启后断言 `ResumeAvailable`，恢复只允许第二项；
  2. 两项都等于 expected，断言 `AlreadyApplied` 与终态 2/2；
  3. 第一项 expected、第二项既非 base 也非 expected，断言 `ManualReviewRequired`，绝不派发；
  4. v1 单操作 checkpoint 仍能分类与恢复。

- [ ] **Step 2: 执行 RED。**

  Run: `cargo test run_engine_tests::durable --lib -- --nocapture`

  Expected: 旧分类只能接受“全部 base”或“全部 expected”，所以前缀情形失败。

- [ ] **Step 3: 最小实现前缀分类。**

  将恢复校验逐项执行：已应用前缀必须等于每项 expected，未执行后缀必须等于其 base；同一路径重叠时按计划哈希链的相邻状态比对。当前内容全等 expected 才 `AlreadyApplied`；唯一满足 checkpoint 游标的 prefix/base 才 `ResumeAvailable`；其他所有组合为 `ManualReview`。恢复补写 ToolCompleted 和 checkpoint 时按操作顺序补齐缺失项，绝不构造一个代表多操作的虚假完成事件。

- [ ] **Step 4: 执行 GREEN。**

  Run: `cargo test run_engine_tests::durable --lib -- --nocapture`

  Expected: 四个恢复情形和原有恢复测试通过，且没有自动 Web/Provider 重放。

## Task 5: 确认后的同类受限 ToolLoop 验证

**Files:**

- Modify: `src-tauri/src/ai_runtime/run_contract.rs`
- Modify: `src-tauri/src/ai_runtime/agent_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/run_engine/mod.rs`
- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Modify: `src-tauri/src/commands/assistant_commands.rs`
- Test: `src-tauri/src/ai_runtime/agent_tool_loop_tests.rs`
- Test: `src-tauri/src/commands/assistant_commands.rs` 内测试

**Consumes:** 已完成的整组操作结果、冻结路径与同一 Run 的 `RunBudgetPolicy`。

**Produces:** 仅适用于成功整组的 `execute_post_confirmation_verification_with_sink`；无可用模型时使用 Host 事实报告完成，不把已执行写入变成失败。

- [ ] **Step 1: 写入验证预算和越权阻断的失败测试。**

  断言 DurableApply policy 精确包含 `post_confirmation_max_model_turns == 2` 与 `post_confirmation_max_local_tool_calls == 4`，普通预算不被放大。用 scripted provider 在确认后依次请求 `web_search`、`read_note`(未冻结路径)、`replace_selection`，分别断言无网络、无越界文件读取、无写入；用两个目标 `read_note` 后自然结束，断言最多 2 provider turns、4 本地调用并形成最终报告。

- [ ] **Step 2: 执行 RED。**

  Run: `cargo test agent_tool_loop_tests::post_confirmation assistant_commands::tests::post_confirmation --lib -- --nocapture`

  Expected: 当前 policy 的确认后模型额度为 0，且执行器尚无目标路径限制，测试失败。

- [ ] **Step 3: 最小实现同一 ToolLoop 的受限预算。**

  在 `RunBudgetPolicy` 增加 `post_confirmation_max_local_tool_calls` 并将 DurableApply 固定为 `2/4`，其余 profile 固定 `0/0`；同步 legacy materialization，使旧 schema 只能在等价旧字段下归一化，绝不接受伪造的放大额度。让 `AgentToolLoop` 接受一个明确的验证 budget override（max model turns 2、max local calls 4、其他每类与总工具为 0），复用原有转录、重复调用与输出限额，不复制循环。

- [ ] **Step 4: 最小实现执行器目标限制与终态。**

  在 `NormalRunToolExecutor` 加入可选 `verification_targets: BTreeSet<String>`。设置该字段时：工具 surface 仅含 `read_note`；参数 `path` 必须精确等于一个冻结相对路径；所有搜索、列表、Web、外部、runtime、写工具在 dispatch 前返回 `post_confirmation_verification_out_of_scope`，不调用 Provider/磁盘写入。`RunEngine` 提供一个仅改变 budget 与验证提示词的包装入口，仍调用现有 `execute_tool_loop_with_sink_internal`；提示词必须包含实际已执行操作和目标路径，并明确“只能核对这些文件，不得提出或执行额外修改”。

  整组成功时命令层优先调用该入口；只有模型网关不可用或能力不支持工具时，安全降级为 Host 生成的完成报告，包含“已执行 N/N 项；未进行模型复核”，不得因复核不可用把已执行写入标为失败。部分执行一律不启动验证循环。

- [ ] **Step 5: 执行 GREEN。**

  Run: `cargo test agent_tool_loop_tests::post_confirmation assistant_commands::tests::post_confirmation --lib -- --nocapture`

  Expected: 成功验证、全部越权阻断、预算边界和降级报告通过。

## Task 6: 端到端回归、事实文档与自查

**Files:**

- Modify: `agent-harness/01-authority-and-invariants.md`
- Modify: `agent-harness/03-target-architecture.md`
- Modify: `agent-harness/04-adaptive-agent-loop-and-tool-contracts.md`
- Modify: `agent-harness/05-implementation-roadmap.md`
- Modify: `agent-harness/06-evaluation-performance-and-acceptance.md`
- Modify: `agent-harness/A-capability-evidence-register.md`
- Modify: `agent-harness/B-task-capability-and-risk-matrix.md`
- Modify: `ARCHITECTURE.md`
- Modify: `ROADMAP.md`
- Test: `scripts/docs-facts-check.mjs` 既有检查

**Consumes:** 已通过的实现与命名测试。

**Produces:** 已部署事实与目标边界一致的唯一 Harness 文档体系；不把未来 HR-6/HR-7 写成已完成。

- [ ] **Step 1: 运行覆盖 HR-5 退出条件的定向回归。**

  Run:

  ```bash
  cargo test frozen_change_plan_tests --lib -- --nocapture
  cargo test agent_tool_loop_tests:: --lib -- --nocapture
  cargo test run_engine_tests::durable --lib -- --nocapture
  cargo test assistant_commands::tests::confirmed --lib -- --nocapture
  cargo test assistant_commands::tests::post_confirmation --lib -- --nocapture
  ```

  Expected: 覆盖 6/6 限制、批量确认、成功、第二项漂移、前缀恢复、重复确认、验证越权与旧 v1 读取；不触发真实 Provider。

- [ ] **Step 2: 仅依据通过的证据更新文档。**

  在 Harness 文档明确：一个 `FrozenChangePlan` 最多 6 操作/6 文件；一次确认；每项执行前 hash/权限重验；部分执行不自动重放；确认后最多 2 模型/4 个冻结目标 `read_note`，无 Web/外部/runtime/写入。`ARCHITECTURE.md` 只叙述此轮已经部署的事实；`ROADMAP.md` 将 HR-5 标为已完成并链接命名测试/提交，HR-6/HR-7 仍为后续。补充评测矩阵中的多文件、漂移、重启和验证越权案例。

- [ ] **Step 3: 执行文档、格式和静态质量门禁。**

  Run:

  ```bash
  npm run docs:check
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  npm run lint
  npm run format:check
  npm run typecheck
  git diff --check
  ```

  Expected: 全部退出码为 0；若任一检查失败，先修正对应问题并重跑该检查及受影响定向测试。

- [ ] **Step 4: 完成自查与提交。**

  逐项对照 HR-5 退出条件，检查：没有新增表/迁移/IPC/Provider；没有第二个写入引擎；v1 计划/checkpoint 可读取；确认后的执行器不能向冻结集外读写或联网；错误与部分报告不夸大完成状态。复读 `git diff` 与 `git status --short`，确认没有无关改动后执行：

  ```bash
  git add src-tauri/src/ai_runtime src-tauri/src/commands/assistant_commands.rs agent-harness ARCHITECTURE.md ROADMAP.md docs/superpowers/plans/2026-08-30-agent-harness-hr5-frozen-change-set-and-verification.md
  git commit -m "feat(ai): 完成 Harness HR-5 冻结变更集与验证"
  git push origin branch-v1.3.0
  ```

  Expected: 产生一个中文 Conventional Commit，远端 `branch-v1.3.0` 包含该提交。

## 自查记录

- 覆盖核对：Task 1 覆盖 6 操作/6 文件、有序 hash、v1 兼容；Task 2 覆盖一次确认与确认前无写；Task 3 覆盖顺序执行、漂移停止、重复确认和部分报告；Task 4 覆盖重启/恢复；Task 5 覆盖 2/4 验证、只读目标边界与降级；Task 6 覆盖文档、质量门禁与提交。没有把 HR-6/HR-7 的领域清理或真实 Provider 试点混入 HR-5。
- 占位检查：本文没有 `TODO`、`TBD`、"以后实现"或“适当处理”式步骤；每项实现均给出精确文件、接口、测试行为与命令。
- 合同一致性：`FrozenChangePlan.operations()` 是所有批量执行和恢复的唯一操作序列；`DurableApplyCheckpoint.next_operation_index` 与 `operation_count` 是唯一恢复游标；验证预算仅来自持久化 `RunBudgetPolicy`，验证执行器仅复用 `NormalRunToolExecutor` 的 dispatch/权限/审计路径。
