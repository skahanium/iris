# system_time_now

<!-- iris:object T01 kind=rules file=true -->

`system_time_now` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T01 -->

<!-- iris:object T01 kind=tool name=system_time_now owner=M06 -->

## 用途与语义归属

读取可信的本机当前日期、时间、星期与时区，回答「今天／现在／星期几」类问题时优先使用；不需要联网，也不读取任何用户资料。语义归属为 M6 → 系统时钟，责任模块 `M06`（目录中的 `owner` 登记为 `M06`）。它服务的是需要时间基准的任务（时效性提问、相对日期换算），不是外部世界事实的取证工具：外部事实仍须走联网与检索链路。

## 参数与消费

目录 schema 声明为空对象 `{"type":"object","properties":{}}`，**不声明任何参数**。派发入口 `runtime_impl::system_time_now_tool()` 不接收 `args`，也不读取任何字段；因此本工具没有「暴露但未消费」的参数。

返回结构对应 `RuntimeTimeContext`：`kind`（固定 `system_time`）、`local_date`、`local_time`、`local_datetime`、`weekday_zh`、`weekday_en`、`utc_offset`、`timezone`。时区取环境变量 `TZ`，未设置时回退为当前 UTC 偏移字符串——这是源码事实，不是「系统时区名一定可得」的承诺。

系统提示中另有 `local_date_line_zh()` 生成的「【本机日期】」行；本工具与它同源，**不替代**运行时事实区块。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["runtime.read"]`；`run_intake` 对每个 Run 恒定加入 `runtime.read`，注释明确「代理运行时事实总是本地可用」。
- 权限原子（`C04`）：`Atom::RuntimeContextRead`，风险等级 Low，`supported = true`，`requires_confirmation = false`。
- 不受联网开关约束；不读凭据明文，不受 Vault 检索范围或文档策略约束（该读取不落到任何笔记路径）。
- 子任务继承：该授权只读、低风险，随父级冻结能力快照进入子运行；子运行不能借它扩权。

## 副作用

**无文件写入、无数据库写入。** 处理器只序列化一次时钟读取的结果；不触碰 vault、不创建证据、不改变 Run 状态。输出策略为 `small_snapshot`，证据策略为 `runtime_fact`。

## 预算

- 执行元数据 `cost_class = "runtime"`，`budget_class()` 归入 `ToolBudgetClass::Runtime`，与 `app_context_read`、`capabilities_read` 共用同一类别上限。
- 当前三个生产预设（Standard／Delegated／DurableApply）的 `max_runtime_tool_calls` 都是 4，`max_tool_calls` 都是 24；`Direct` 预设把各分类额度置 0。这些是**主循环分类额度**，不是任何全局统一总数。
- 不进「每模型回合最多 2 个发现调用」的限制：`is_discovery()` 不包含本工具。
- 派发包裹在 30 秒超时中（`DEFAULT_TOOL_DISPATCH_TIMEOUT`）。
- 数值修改属于架构变更，须同步 `K06`；本卡不新设数值，也不承诺额度是否充裕（`Q19` 讨论的是网络额度，与本类别不同）。

## 幂等

读取操作，重复调用不产生重复副作用。但**返回值随时间变化**：同一 Run 内两次调用可以在秒级不同，跨日运行时 `local_date` 会变。因此消费者不得把工具名当作可缓存的稳定结果；需要一致时间基准时应复用同一次观察，而不是假定两次调用相同。

## 取消

派发前执行 `ctx.ensure_run_active()`，若该 Run 已请求取消则返回取消错误而不读取时钟。处理器本身是同步、无 `await` 点的一次读取，因此不存在「执行中途取消」的窗口；超时与取消属两个不同通道。本工具不留下部分结果。

## 失败反馈

- 超时：派发层返回 `{"error":"tool_dispatch_timeout","failure_class":"timeout","message":"tool system_time_now timed out after 30s"}`。
- 参数无效：本工具无参数，正常路径不产生参数错误；若模型传入未声明字段，当前参数校验允许额外字段透传（schema 未声明 `additionalProperties`），处理器忽略它们——这是**静默接受**，与 `G01` 中「无效参数必须明确拒绝而不是静默忽略」的要求不一致。
- 可恢复性：若真的发生超时，按 `C14` 的可恢复错误处理，反馈给模型后由模型决定是否重试；当前重试白名单（`is_retryable_tool_error`）只覆盖 `web_search`／`web_fetch`，本工具不自动重试。
- 用户侧：不得因本工具的单次失败提示「模型能力降级」（`N17`）。

## 暴露规则

- 目录 `default_enabled_without_skill = true`，属于基础工具。
- 目标工具面：普通助手对话的「基础 4 个」之一（时间、应用上下文、能力查询、技能列表）。
- 例外：`ContextMode::ExplicitReferences` 且检索范围不受限时，`ToolRegistry::constrain_for_run_context` 的允许清单只含 `read_note`、`get_outline`、`get_context_packets`、`get_backlinks`、`web_search`、`web_fetch`、`spawn_subagent`，本工具在该组合下被隐藏。
- 无 Planned 状态，不存在「有名字但未实现」的暴露问题。

## 源码落点

- 目录定义：[tool_catalog/root.rs](../../src-tauri/src/ai_runtime/tool_catalog/root.rs)（第 8 行起，`Dispatchable`、`ReadProfile`、`max_results = Some(1)`）。
- 派发分支：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`"system_time_now" => runtime_impl::system_time_now_tool()`）。
- 处理器：[tool_dispatch/runtime.rs](../../src-tauri/src/ai_runtime/tool_dispatch/runtime.rs)（`system_time_now_tool`）。
- 事实来源：[runtime_context.rs](../../src-tauri/src/ai_runtime/runtime_context.rs) 的 `current_time_context()`（`RuntimeTimeContext` 字段与 `TZ` 回退）。
- 权限分类：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`RuntimeContextRead`，Low，supported）。
- 能力与预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`runtime.read`、`runtime` 成本类）。
- 预算数值：[run_contract.rs](../../src-tauri/src/ai_runtime/run_contract.rs)（`RunBudgetPolicy` 三个预设）。
- 现有测试：[tool_catalog/tests.rs](../../src-tauri/src/ai_runtime/tool_catalog/tests.rs)（预算分类断言）、[tool_dispatch/runtime.rs](../../src-tauri/src/ai_runtime/tool_dispatch/runtime.rs) 内的派发测试。

## 当前状态

`implementation.state = present`（静态源码事实：目录项为 `Dispatchable`，派发分支与处理器均在位）。`verification.state = none`。

事实不等于合同正确或用户任务可用：本轮未执行真实模型验收，也未核对真实运行中的时区表达是否满足使用场景。未知项：`TZ` 缺失时 `timezone` 回退为偏移字符串（如 `+08:00`）是否足以支撑需要具名时区的回答，尚无证据；`weekday_en`、`local_datetime` 目前没有已知消费者（`build_runtime_context_prompt` 只读 `local_date`、`weekday_zh`、`local_time`、`utc_offset`、`timezone`），是否应保留**待核对**。

## 相关合同

- `K10` 工具面版本与派发观察：本工具的暴露、派发状态与观察由该合同约束。
- `K05` 授权范围与冻结确认：本工具的授权来自冻结能力快照，不在运行中扩权（本卡只引用该合同，不复制其定义）。
- `K06` 统一预算账本：`runtime` 分类额度的唯一权威账本。
- 相邻工具：`T02` `app_context_read`、`T03` `capabilities_read`——三者同属 `runtime.read` 能力与 `Runtime` 预算类别的运行时事实快照组。
- 需求依据：`N01`（基础对话与追问可靠）要求时间类问题有可用能力。

<!-- iris:end T01 -->
