# scheduled_task_list

<!-- iris:object T33 kind=rules file=true -->

`scheduled_task_list` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T33 -->

<!-- iris:object T33 kind=tool name=scheduled_task_list owner=M06 -->

## 用途与语义归属

查询已登记任务。语义归属「工具（`M6` / 计划服务）」，责任模块 `M06`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.3。

它是计划服务的**只读入口**：只返回 `scheduled_tasks` 表中已登记的行，不触发执行、不改 `enabled`／`schedule`、不产生新的登记。计划工具的目标合同是「沿已有登记语义，**不承诺后台自动执行**」（架构 §6.3 对 `T32` 的取舍），因此本卡片不把「查得到登记」写成「任务会被执行」。

相邻工具：`T32` `scheduled_task_create`（登记）、`T34` `scheduled_task_delete`（删除登记）。

## 参数与消费

| 参数               | 类型    | 必填 | 默认    | 消费事实                                                                                                |
| ------------------ | ------- | ---- | ------- | ------------------------------------------------------------------------------------------------------- |
| `include_disabled` | boolean | 否   | `false` | 直接作为 SQL 绑定参数：为真时返回全部登记，为假时只返回 `enabled = 1` 的行（`WHERE ?1 OR enabled = 1`） |

目录定义中未声明 `required`，因此零参数调用是正常路径。

返回 `{ "tasks": [ … ], "count": <n> }`，每条形如 `{ "id", "title", "prompt", "schedule", "enabled", "updated_at" }`。源码事实：

1. 排序与截断固定在 SQL 内：`ORDER BY updated_at DESC LIMIT 50`，没有游标或分页参数。
2. `enabled` 由 `i64 != 0` 转成布尔值。
3. `prompt`（登记时的任务描述全文）随结果回传模型，因此本工具读取的是**已登记内容**，不只是登记元数据。
4. 行映射失败的行被 `flatten()` 静默丢弃，`count` 是成功映射的行数；读取失败的行不会作为错误项单独报告（**待核对**：是否需要显式缺口表达，未登记为 `G*`／`Q*`）。
5. 50 条截断不附带「还有更多」标志；`include_disabled = false` 下的空结果是「没有启用的登记」，与「完全没有登记」在结果上不可区分。
6. 目录声明 `max_results = Some(50)`；当前生产路径未见读取该字段做截断的消费者，实际截断来自上述 SQL。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["schedule.read"]`；`access_level = ReadProfile` 是展示元数据，不得扩大 Run 授权。
- 权限原子（`C04`）：`AppStateRead`，Low，`supported = true`。
- 目录声明 `requires_confirmation = false`，且风险级别为 Low，因此预检决策为 `Allow` → `AutoAllowed`：本工具不需要逐次确认，也不进入冻结确认路径（`K05`）。
- 处理器不消费 `ToolDispatchContext`：不检查写目标、文档策略、检索范围或 Skill 范围。本工具读的是应用状态表而不是笔记，因此上述笔记范围约束不适用于它；Run 取消仍由派发入口检查。
- **待核对（静态边界）**：`schedule.read` 未出现在 [run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs) 的 `required_capabilities` 构造中，授权快照的输入 `RunPolicyRequest.requested_capabilities` 在 [agent_run_repository.rs](../../src-tauri/src/ai_runtime/agent_run_repository.rs) 的 `policy_request_for_session` 中固定为空向量。据此静态阅读，当前普通 Run 路径上没有产生 `schedule.read` 的代码，本工具是否可达未运行验证；该事实未登记为 `G*`／`Q*`。

## 副作用

**无文件写入、无数据库写入、无配置写入。** 处理器通过 `with_read_conn` 执行一次 SELECT 并在内存中组装结果。

- 不写索引、不产生版本快照、不落证据账本（`register_local_tool_evidence` 的登记分支不含本工具）。
- 工具审计只记录参数与结果的**形状摘要**（`shape=object, keys=N`），不记录 `title`／`prompt`／`schedule` 的取值（[tool_audit.rs](../../src-tauri/src/ai_runtime/tool_audit.rs) 的 `sanitize_arguments`／`sanitize_result` 回落分支）。
- 对照记录：MCP provider 列举的 list 写库（`F04`／`Q05`）已从源码消除，关闭仍等 `D04`；不在本工具路径上。

## 预算

- `cost_class` 未声明，按访问级别回落：`ReadProfile` → `ToolBudgetClass::Runtime`，因此计入 `max_runtime_tool_calls`（当前三类预设均为 4），并同时占用总工具调用上限 `max_tool_calls`（当前 24）；不占用 local／network／external-read 额度（数值权威在 `K06`）。
- 不在 `is_discovery()` 名单内，不占用「每模型回合最多 2 个发现调用」的配额。
- 派发包裹在 30 秒超时中；不在自动重试白名单（`dispatch_tool_with_retry` 只重试 `web_search`／`web_fetch`）。
- 相同参数重复超过 2 次会被循环以 `tool_call_repeated` 拒绝；已成功的同指纹调用以 `tool_call_already_succeeded` 拒绝。
- 普通工具结果的字符预算为 8 000（`MAX_TOOL_RESULT_CHARS`），50 条含 `prompt` 的结果可能被有界裁剪。

## 幂等

纯读取，重复调用不产生副作用。结果随时间与登记变化：新建或删除计划任务后同参数结果不同，`updated_at` 也会变。消费者不得把一次结果当作计划登记表的稳定快照，也不得把空列表当作「计划服务不可用」。

## 取消

派发前执行 `ctx.ensure_run_active()`；已取消的 Run 不执行查询。查询与结果组装在单次同步连接内完成，没有执行中途的取消检查点，也不留下部分结果。

## 失败反馈

- `include_disabled` 类型不符：由参数守卫拦住，返回 `{"error":"tool_arguments_invalid"}`——**不是字段级说明**；字段级参数反馈是架构 §3 M5 对「工具存在但参数无效」的目标要求，本卡片不声明其已实现。
- 数据库不可读或查询失败：`AppResult` 错误经派发层成为失败的工具结果（`success = false`、`error` 文本），不返回空列表冒充成功。
- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`。
- 空结果是有效观察，不得当作传输故障重复同一请求。
- 用户侧：单次失败不得表达为「模型能力降级」（`N17`）。

## 暴露规则

- 扩展能力目录项（`surface=extended`），位于目录的 `root` 组；目录声明 `default_enabled_without_skill = false`（该字段当前只有测试消费者，生产暴露由能力与上下文决定）。
- 运行期须在冻结能力中含 `schedule.read`；是否进入某一轮工具面由 `C16` 按授权与上下文决定并记录工具面版本（`K10`）。
- 本工具不需要确认，因此在 `only_auto = false`（Durable）与 `only_auto = true`（其他）两种工具面装配下都不会被确认过滤掉，但仍受能力过滤约束。
- 在「显式引用且检索范围不受限」的上下文中被隐藏：`constrain_for_run_context` 的允许清单不含本工具。
- 受限子任务的工具白名单（`CHILD_SAFE_TOOLS`）不含本工具，因此子 Run 看不到它。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/root.rs](../../src-tauri/src/ai_runtime/tool_catalog/root.rs)（`scheduled_task_list`，`Dispatchable`、`ReadProfile`、`max_results = Some(50)`）。
- 派发与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/schedule.rs](../../src-tauri/src/ai_runtime/tool_dispatch/schedule.rs)（`scheduled_task_list_tool` 及其 SQL）。
- 表结构：[migrations/021_skill_lifecycle_metadata.sql](../../src-tauri/migrations/021_skill_lifecycle_metadata.sql)（`scheduled_tasks`）。
- 能力与权限映射：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)、[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`permission_profile_for_tool`）。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，处理器与固定 SQL 在位；该读取路径不消费笔记范围检查）。`verification.state = none`。

不确定处（**待核对**）：`schedule.read` 在当前授权路径上的产生位置；50 条截断与行映射失败的静默丢失是否需要显式表达；`prompt` 原文回传模型的边界。以上均未登记为 `G*`／`Q*`，本卡片不声明合同正确、不声明计划任务可用，也不声明后台自动执行。

## 相关合同

- `K05` 授权范围与冻结确认：登记关系为 `consumes`；本工具不进入确认路径，但调用仍以授权判定为准（只引用，不复制其定义）。
- `K10` 工具面版本与派发观察；`K06` 统一预算账本（Runtime 分类额度的唯一权威）。
- 相关对象：`M06`、`M02`、`C04`、`C16`、`C17`、`C13`；相邻工具 `T32` `scheduled_task_create`、`T34` `scheduled_task_delete`。
- 需求依据：`N17`（单次工具错误不得表达为模型能力降级）；语义归属与「不承诺后台自动执行」的取舍见架构 §6.3；目录目标与现状差异见 `G01`。

<!-- iris:end T33 -->
