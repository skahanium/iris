# spawn_subagent

<!-- iris:object T31 kind=rules file=true -->

`spawn_subagent` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T31 -->

<!-- iris:object T31 kind=tool name=spawn_subagent owner=M05 -->

## 用途与语义归属

**受限只读委派**：把独立阅读或分析交给一层子运行，并取回带来源与用量的报告。语义归属为工具，责任模块 `M05`（Agent 执行控制），由 `C15` 子任务协调承接。

目标合同：受限只读委派；**启用该能力时暴露**；`context_hint` 应被消费；`max_rounds` 只能收窄 `C13` 分配的上限、不能提权；子任务网页核验应具备获准的 `web_fetch`；报告返回实际来源、未解决问题与用量；取消固定 50／75 的伪置信概率（架构 §6.3，`Q06`）。

当前实现与目标合同有三处明确差异（登记于 `Q06`、`Q07`，本卡只如实记录，不声称已修正）：

1. **`context_hint` 未被消费**；
2. **`max_rounds` 不作为轮次控制输入**，子运行采用冻结预算（0 安全默认或 `Delegated` 预设）；
3. **报告置信度按是否有 evidence ID 固定填 50 或 75**，未经校准；`findings`／`open_questions` 在当前实现中恒为空数组。

此外 `Q07` 记录：子工具面含 `web_search` 但**没有 `web_fetch`**，限制标准网页正文核验；`read_note` 等本地读取工具仍可用，因此不能推论为「无法读取任何正文」。

## 参数与消费

目录 schema（顶层）：`task`（string）、`tasks`（array，`minItems = 1`、`maxItems = 3`）、`role`（string）、`context_hint`（string）、`max_rounds`（integer，default 2）、`allowed_tools`（string array）、`resource_locks`（array）。schema **没有全局 `required`**：`task` 与 `tasks` 是互斥的两个入口（[subagent_coordinator.rs](../../src-tauri/src/ai_runtime/subagent_coordinator.rs) 的 `spawn_subagent_catalog_declares_single_or_bounded_batch_tasks` 断言了这一点）。

每个 `tasks` 元素（batch item）另有自己的 schema：`task`（required）、`role`、`context_hint`、`allowed_tools`、`resource_locks`——**batch item 没有 `max_rounds`**；`role`／`context_hint`／`allowed_tools`／`resource_locks` 落在 batch item 或顶层时，顶层版本只对单任务入口生效（batch 分支只读取 `tasks` 数组）。

逐项消费事实：

| 参数             | 消费情况                                                                                                                                                                                         |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `task`           | **消费**。与 `tasks` 互斥：两者同时出现报 `child_run_task_and_tasks_mutually_exclusive`；都缺失报 `child_run_task_required`；trim 后为空报 `child_run_task_invalid`                              |
| `tasks`          | **消费**。非数组报 `child_run_batch_tasks_invalid`；空数组报 `child_run_batch_empty`；超过 3 项报 `child_run_batch_limit_exceeded`                                                               |
| `role`           | **消费**。默认 `"subagent"`，写入子运行系统提示词的角色位                                                                                                                                        |
| `allowed_tools`  | **消费并收窄**。非数组或含非字符串报 `child_run_allowed_tools_invalid`；结果与「继承的工具面 ∩ `CHILD_SAFE_TOOLS`」求交，**只能收窄不能加宽**                                                    |
| `resource_locks` | **消费**。解析 `resource_locks[].resource_type`／`resource_id`／`access`（缺省 `note`／`read`）；`access = write` 的锁把该子任务判为不可运行（`child_run_write_lock_forbidden`），不提供写入协调 |
| `context_hint`   | **未消费**（`Q06`）。声明存在、执行路径不读取，也不拒绝                                                                                                                                          |
| `max_rounds`     | **未消费**（`Q06`）。执行采用冻结的 `AgentToolLoop::from_child_policy(&budget_policy)`，轮次来自预算策略而非该参数；既不消费也不拒绝                                                             |

返回：`{ "subagentBatchReport": { "items": [<SubagentReport>, …] } }`。每个报告含 `subagentId`、`summary`、`findings`、`evidenceIds`、`confidence`、`openQuestions`、`errors`、`budget`（`modelTurns`／`toolCalls`／`promptTokens`／`completionTokens`／`totalTokens`）。批次工具结果 `success` 的判定是**任一报告无错误**——因此「部分成功」在结果层表现为整体 `success = true`，失败项只在 `errors` 里；全部失败时才 `success = false` 且 `error = child_run_batch_failed`。

持久化身份：子任务 ID 由父 run 身份与调用身份派生（`subagent:<64 位十六进制>`，批量再带 `:N` 序号），不使用供应商提供的原始调用 ID；报告文本在写库前经敏感内容与长度裁剪。

## 授权

- 能力合同（`C16`）：`["harness.child_run"]`。该能力由 Intake 按用户消息判定加入：消息含 `subagent`／`子任务`／`委派`／`分工`／`并行`／`交叉验证` 等标记时（[run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs) 的 `needs_child_run`），即「用户显式要求委派或并行工作」时启用。**子 agent 不会仅因为父级有只读工具就被加入。**
- 权限原子（`C04`）：`Atom::HarnessChildRun`，Risk = Low，`supported = true`，`requires_confirmation = false` ⇒ `AutoAllowed`。
- 目录标记：`implementation = HarnessOnly`（由循环内的 `execute_child_run_batch` 处理，不经 `dispatch_tool_inner` 的普通分支），但 `is_exposable_tool` 对它放行，因此它可以进入工具面并被执行门评估——**「HarnessOnly」不等于「不可暴露」**，这是本工具与 `conclude_reasoning`（同样 HarnessOnly 但已从目标工具面退出）的差别。
- 预算档位：冻结能力含 `harness.child_run` 时，Run 预算档位解析为 `Delegated`——这是唯一的子运行档位（[run_contract.rs](../../src-tauri/src/ai_runtime/run_contract.rs) 的 `for_envelope`）。
- 子运行继承：只读白名单（见「暴露规则」）；`subagent_depth` 必须为 0，否则每个子任务以 `subagent_depth_exceeded` 失败——**一层委派**由深度门保证，子运行不能再调用本工具。
- 显式引用场景：`constrain_for_run_context` 的白名单保留本工具（`harness.child_run` 已授权时）。
- 不依赖联网开关；但子任务若发起网页搜索，仍走同一授权与预算边界，不能绕过（架构 §7.5）。

## 副作用

**不写用户笔记、不写 Git、不创建确认计划**；子运行也不获得任何写入工具（`CHILD_SAFE_TOOLS` 只含只读与 `web_search`）。

它确实产生以下状态变化，必须如实记录：

- 追加运行事件：每个子任务各一条「工具开始」与「工具完成」记录（结果摘要受限于安全文本）；
- 登记证据：子运行的本地检索会登记本地证据条目，并把 evidence ID 放进报告；
- 消耗额度：占父级子运行次数配额，并实际调用模型（产生 token 用量）。
- 部分执行：批量中未获配额／深度超限／提供方不可用／含写锁的子任务**不执行**，以 `errors` 报告；已启动的子任务不因同批其他项失败而回滚（`join_all` 并发，无批次回滚）。

## 预算

- 父级本次调用的分类：目录没有 execution metadata、`requires_confirmation = false`、访问等级 `ReadIndex` ⇒ `ToolBudgetClass::Local`（[capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs) 的 `budget_class`）。因此它消耗 `max_local_tool_calls`（当前预设 12）与 `max_tool_calls`（24）——**委派调用占本地只读额度**，这是容易忽略的事实。
- 子运行次数配额：`max_child_runs`。`Standard` 与 `DurableApply` 预设为 **0**，`Delegated` 为 **3**。`reserve_child_runs` 在启动前按「本批可运行子任务数」一次性预留，超出即整批以 `child_run_limit_exceeded` 失败（不会部分预留后静默降级）。同一 Run 内多次调用累计计数。
- 单个子运行额度（`Delegated`）：`child_max_model_turns = 2`、`child_max_tool_calls = 6`、`child_input_tokens_per_turn = 2_000`、`child_output_tokens_per_turn = 1_024`。子循环由 `AgentToolLoop::from_child_policy` 构造，只使用这组子档位数值。
- `Q06` 的事实：`max_rounds` 参数**不影响**上述上限；模型传入的轮次数被忽略。
- 用量呈现：子运行的 `modelTurns`／`toolCalls`／token 用量出现在每个报告的 `budget` 里。**当前实现未把这些子用量累加进父循环的 token 计数器**（父循环的 `total_tokens` 等只对自己发起的模型调用相加）；该差异与架构「C13 保留父子层级明细，同时控制顶层总消耗」的目标口径不同，属**当前实现事实**，本卡据此不作「顶层总账已经统一」的结论（相关风险同样见 `Q19` 关于网络额度的口径说明）。
- 单次派发 30 秒超时对本工具**不构成有效约束**：子运行在超时包裹内等待，批量并发子任务可能使总时长显著超过 30 秒，而超时以整个派发为单位——**该交互的实际表现待核对**（本卡不据此设新数值门槛）。
- 数值权威在 `K06`；本卡只记录现值，不新设门槛。

## 幂等

**不是副作用幂等的写操作**（它不写用户数据，因此没有重复变更风险），但重复调用**不廉价**：每次都会重新预留子运行配额并产生新的模型调用与 token 消耗。

- 去重键：无。同一 `task` 文本重复提议会各占一次子运行配额。
- 循环层机械保护：同回合内同指纹调用超过 `MAX_REPEAT_CALLS` 返回 `tool_call_repeated`；已成功同指纹调用返回 `tool_call_already_succeeded`。由于子任务参数通常不同，这两条对本工具的实际约束有限。
- 报告身份稳定：同一父 run 与同一调用身份派生出同一 `subagent:<hash>` ID，因此同一调用不会生成两个不同的子任务身份。
- 子报告的 `confidence` 按「是否有 evidence ID」固定为 50／75（`Q06`），**不是**对结论正确性的校准，不能当作幂等性或可靠性依据。

## 取消

取消在派发入口生效：父级按 `is_abort_requested(run_id)` 检查后返回 `Cancelled`（[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs) 的 `execute_builtin_tool_call` 开头）。

子运行自身的取消表现：

- 子循环内每个工具派发前仍执行 `ctx.ensure_run_active()`，因此取消会阻止子运行**后续**的工具调用；
- 子运行被取消时，报告以 `child_run_cancelled` 归类（`sanitize_child_run_error` 只保留稳定错误码，不把供应商文本写入报告）；
- 批量是并发 `join_all`，**没有按子任务的取消检查点**：取消不会单独中断某一个已在执行的子任务，也不会留下「部分子报告」以外的中间状态；
- 已消耗的额度不因取消返还（子运行计数器已前进）。

## 失败反馈

可恢复与不可恢复的分界（按架构 §3 M5 的分类）：

- **可恢复（参数类）**：`task`／`tasks` 冲突或缺失、batch 超限、`allowed_tools` 形状错误——返回稳定错误码（如 `child_run_batch_limit_exceeded`），模型可修正后重提。字段级说明是 `G01` 的目标，当前只给稳定码。
- **需用户操作**：无确认要求；但若 Run 未获 `harness.child_run`，工具根本不在面上，模型应改为自行完成或说明限制（不得伪造「已委派」）。
- **不可恢复／受限（配置与能力类）**：`child_run_provider_unavailable`（父 Run 没有可用的子运行 provider）、`subagent_depth_exceeded`（深度非 0）、`child_run_limit_exceeded`（配额不足）、`child_run_write_lock_forbidden`（请求写锁）。这些都不重试，按事实报告。
- **子运行内部错误**：一律归一为稳定码（`child_run_failed`／`child_run_cancelled`／`child_run_limit_exceeded`／`child_run_web_evidence_required`），不把供应商错误字符串写入报告——与「报告不复制供应商文本」的既有约定一致。
- **空报告字段**：`findings` 与 `openQuestions` 当前恒为空数组，尽管目标合同要求报告实际来源与未解决问题；模型只能从 `summary`／`evidenceIds`／`budget` 判断，这对「父级综合前检查报告来源、缺口与用量」的目标是**能力缺口**（`Q06` 所需证据的一部分）。
- **用户侧表达**：委派失败不得表达为「模型能力降级」（`N17`）；是否在用户侧说明限制，取决于最终交付是否受影响（架构 §3 M5 的「用户表达与内部记录」）。

## 暴露规则

- 目录 `default_enabled_without_skill = true`（且不需确认，因此进入核心无技能只读名单的过滤条件成立），但**实际暴露由能力决定**：只有 Run 冻结能力含 `harness.child_run` 时才进入工具面（`C16` 的 `is_authorized_by`）。这与目标合同「启用该能力时暴露」一致。
- 无范围显式引用场景下仍保留（白名单含本工具）。
- **子运行工具面**（`child_tool_surface`，由父工具面 ∩ 白名单 ∩ `allowed_tools` 得到）：`search_hybrid`、`search_semantic`、`search_keyword`、`get_regulation`、`get_context_packets`、`system_time_now`、`app_context_read`、`capabilities_read`、`web_search`、`read_note`、`list_vault`、`get_outline`、`get_backlinks`、`vault_version_list`、`git_read_status`、`git_read_diff`、`git_read_log`、`doc_extract_citations`。
- **白名单不含 `web_fetch`**：子任务能搜索网页线索，但不能在子运行内读取网页正文（`Q07`）。白名单也不含 `spawn_subagent`（一层委派）与任何写入／记忆／计划工具。
- Planned 项不暴露；本工具不是 Planned。

## 源码落点

- 目录定义：[tool_catalog/read.rs](../../src-tauri/src/ai_runtime/tool_catalog/read.rs)（`spawn_subagent`：`ReadIndex`、`HarnessOnly`、`requires_confirmation = false`）。
- 协调器与任务契约：[subagent_coordinator.rs](../../src-tauri/src/ai_runtime/subagent_coordinator.rs)（`SubAgentTaskSpec::batch_from_tool_call`、`from_task_args`、`child_tool_surface`、`CHILD_SAFE_TOOLS`、`MAX_SUBAGENT_BATCH_TASKS = 3`、`report_success` 的 `confidence`、`report_error_with_budget`、`safe_batch_report_for_persistence`）。
- 循环内执行：[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)（`execute_builtin_tool_call` 的 `spawn_subagent` 分支、`execute_child_run_batch`、`execute_one_child_run`、`reserve_child_runs`、`child_batch_tool_result`、`sanitize_child_run_error`、`invalid_child_batch_result`）。
- 子循环与子预算：[agent_tool_loop.rs](../../src-tauri/src/ai_runtime/agent_tool_loop.rs)（`from_child_policy`、`execute_child`）、[run_contract.rs](../../src-tauri/src/ai_runtime/run_contract.rs)（`Delegated` 预设、`max_child_runs`、`child_*` 字段）。
- 能力与暴露：[run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs)（`needs_child_run`）、[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`harness.child_run`）、[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`HARNESS_ONLY_TOOL_NAMES`、`is_exposable_tool`）、[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs)（`tools_for_authorized_capabilities`、`constrain_for_run_context`）。
- 权限：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`HarnessChildRun`、`permission_profile_for_tool`）。
- 现有测试：[subagent_coordinator.rs](../../src-tauri/src/ai_runtime/subagent_coordinator.rs)（batch schema、ID 派生、白名单不含变更与递归控制）；[src-tauri/src/ai_runtime/agent_tool_loop_tests.rs](../../src-tauri/src/ai_runtime/agent_tool_loop_tests.rs)（畸形参数到达有界执行器并归属本工具的结构化错误）；[normal_run_service_tests.rs](../../src-tauri/src/ai_runtime/normal_run_service_tests.rs)（子工具面不含 `spawn_subagent`）；[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs)（仅在精确能力授权时暴露）。

## 当前状态

`implementation.state = partial`（静态源码事实：目录项、协调器、批量执行、只读白名单、配额预留、报告归一化与审计均在位；但暴露参数存在未消费项，报告字段与置信度未达目标合同）。`verification.state = none`。

**已登记的缺口**：`Q06`（`context_hint`／`max_rounds` 未消费、固定置信度 50／75）、`Q07`（子工具缺少标准网页正文抓取）；`G01` 把未消费参数列入「无效参数必须明确拒绝而不是静默忽略」的目标。**本卡核对补充、尚未登记为 `G*` 的差异**：`findings`／`openQuestions` 恒空；子运行 token 用量未累加进父循环计数器；30 秒派发超时与并发子运行的关系待核对。

未执行真实模型验收、未做子 agent 收益对照实验，不声明委派有效或成本可接受（架构 §9、讨论第十五、十六节）。

## 相关合同

- `K06` 统一预算账本：子运行配额与分类额度的唯一权威；父子层级明细的目标口径在此合同。
- `K10` 工具面版本与派发观察：本工具是 HarnessOnly 但可暴露的例外项，其派发状态记录按该合同。
- `K14` 证据与来源事实：子报告的证据 ID 与「子报告不是未经验证的最终事实」的边界。
- 相邻工具：`T11` `web_search`（子运行可用的网页线索发现）、`T12` `web_fetch`（**子运行不可用**，`Q07`）、`T05` `search_hybrid`／`T06` `read_note`（子运行的主要本地读取手段）、`T26` `doc_extract_citations`（子运行可用的文本辅助）。
- 需求依据：`N16`（可恢复错误不得一次即终止）、`N12`（授权覆盖子任务）；委派定位见 `L06` 与架构 §7.5。
- 依据文档：[docs/agent-architecture.md](../../docs/agent-architecture.md) §3 M5（`C15`）、§6.3、§7.5；[AGENT-REFORM-DISCUSSION-2026-09-14.md](../../AGENT-REFORM-DISCUSSION-2026-09-14.md) 第十六节与第十八节（子 agent 为受限可选能力，收益与缺口分开评价）。

<!-- iris:end T31 -->
