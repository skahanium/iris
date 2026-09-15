# capabilities_read

<!-- iris:object T03 kind=rules file=true -->

`capabilities_read` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T03 -->

<!-- iris:object T03 kind=tool name=capabilities_read owner=M06 -->

## 用途与语义归属

读取当前 AI 能力摘要：联网开关、已启用模型池、以及**本次 Run 已经暴露**的工具清单；不读取凭据明文。语义归属为工具（`M6`），责任模块 `M06`（`C16` 工具目录、工具面与技能接入）。

目标合同比现状更大：架构定义要求本工具承担「读取能力**及申请同授权内工具面扩展**」。其中「申请扩展」的 `request_tools` 参数**尚未实现**，差异登记在 `G01`（「`capabilities_read` 的目标合同 `request_tools` 参数需实现消费」）。本卡因此同时写现状与目标差距，不把目标写成已实现。

## 参数与消费

现状：目录 schema 声明为空对象 `{"type":"object","properties":{}}`，**不声明任何参数**；处理器 `capabilities_read_tool(state, ctx)` 不读取 `args`。在 `src-tauri/src` 与 `src` 下检索 `request_tools` **无任何命中**，即该参数既不在 schema 中，也没有消费方。

输入来自派发上下文：`ctx.available_tool_names`（本 Run 的工具面）与 `ctx.web_search_enabled`；模型池来自数据库中的 LLM 配置。

返回 `CapabilitySnapshot`：`kind`（固定 `capabilities`）、`web_search_enabled`、`models[]`（`provider_id`、`model`、`configured`）、`tools[]`（`name`、`requires_confirmation`、`access_level`、`cost_class`、`output_policy`、`evidence_policy`）。

源码事实（与「能力查询」的用途直接相关，须如实记录）：

1. 工具清单来自 `ctx.available_tool_names` 与全目录的交集，因此**只报告本 Run 当前工具面内的工具**，不把全目录或未授权能力泄露给模型；`available_tool_names` 为空时 `tools` 为空数组。
2. 工具项包含元数据，**不包含 `input_schema`**。模型因此无法凭本工具构造未暴露工具的合法参数；这与「申请同授权内工具面扩展」的目标合同是否完备，**待核对**。
3. `configured` 只表达「凭据可用或该 provider 不需要 Key」，同样是布尔事实，不含密钥材料。

目标合同（`G01`）：接受目标工具名申请，由 `C16` 与 `C04` 验证后在**下一轮**安装新版本工具面；名称未知或权限不足返回明确原因；不执行业务动作、不授予新权限（架构定义 §6.5）。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["runtime.read"]`；`run_intake` 对每个 Run 恒定加入该能力。
- 权限原子（`C04`）：`Atom::RuntimeContextRead`，Low，`supported = true`，`requires_confirmation = false`。
- 现状下本工具不产生任何副作用，因而不触发写入授权；目标合同下的 `request_tools` 也不得授予新权限，涉及新权限时转现有确认流程（架构定义 §6.5）。
- 凭据边界：`model_pool_snapshots()` 只调用凭据可用性查询，不读取明文。
- 子任务继承：只读、低风险，随父级冻结能力快照进入子运行；子运行依赖同一工具面事实，不能借本工具放宽自己的工具面。

## 副作用

**无文件写入、无数据库写入。** 数据库只读：`llm::config::load(&state.db)` 与凭据可用性查询；`capability_snapshot()` 只做序列化映射。输出策略 `small_snapshot`，证据策略 `runtime_fact`。

（对照记录：已知的「读取路径写库」问题位于 MCP provider 列举（`F04`／`Q05`），**不在本工具路径上**；本卡不据此声称整个读取面都已只读。）

## 预算

- `cost_class = "runtime"`，归入 `ToolBudgetClass::Runtime`；当前预设 `max_runtime_tool_calls = 4`、`max_tool_calls = 24`（主循环分类额度，不是全局统一总数；数值权威在 `K06`）。
- 不进每回合 2 次发现调用上限。
- 派发包裹在 30 秒超时中。
- 目标合同若真的实现「申请并安装下一轮工具面」，其自身开销（工具面版本更新、模型轮次）**尚无预算设计**；这属于 `G01` 的未完成部分，本卡不预判数值。

## 幂等

纯读取，重复调用不产生副作用。返回值可漂移：`web_search_enabled` 随总联网开关变化，`models` 随启用模型池与凭据配置变化，`tools` 随本 Run 的工具面版本变化——`K10` 允许工具面在同一授权范围内于下一轮更新并记录新版本，因此两次调用可能落在不同工具面版本上。消费者不得把一次快照当作整轮不变的事实。

## 取消

派发前执行 `ctx.ensure_run_active()`；已取消的 Run 不再构造快照。处理器同步执行（一次配置与凭据查询），无执行中途取消窗口。不留下部分结果。

## 失败反馈

- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", ...}`。
- 参数无效：现状无参数。若模型提出 `request_tools`，参数校验只按 schema 校验已声明字段（允许额外字段透传），处理器忽略它——模型会得到一个**看似成功但没有申请效果**的结果。这正是 `G01` 要求消除的行为：要实现在下一轮消费，要么明确拒绝并说明原因（架构定义 §6.5「名称未知或权限不足返回明确原因」），不允许静默忽略。
- 具体缺口引用：`G01`（阻断门槛 `at=acceptance`，所需证据为工具卡逐项实现状态与目录分类的一致性核对）。
- 用户侧：不得把读能力失败表达为「模型能力降级」（`N17`）。

## 暴露规则

- `default_enabled_without_skill = true`；目标工具面中属普通助手对话「基础 4 个」之一。
- 例外：`ContextMode::ExplicitReferences` 且检索范围不受限时被 `constrain_for_run_context` 隐藏。
- 架构定义 §6.5 要求「对于当前面未包括、但在既有授权内的能力」由本工具承担申请；在 `request_tools` 落地前，目标暴露规则与现状之间的差距由 `G01` 承载，本卡不宣告该规则已生效。

## 源码落点

- 目录定义：[tool_catalog/root.rs](../../src-tauri/src/ai_runtime/tool_catalog/root.rs)（`Dispatchable`、`ReadProfile`、`max_results = Some(1)`、`runtime`／`small_snapshot`／`runtime_fact`）。
- 派发分支与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/runtime.rs](../../src-tauri/src/ai_runtime/tool_dispatch/runtime.rs)（`capabilities_read_tool`）。
- 快照构造：[runtime_context.rs](../../src-tauri/src/ai_runtime/runtime_context.rs)（`capability_snapshot()`、`model_pool_snapshots()`、`CapabilitySnapshot`、`ToolCapabilitySnapshot`）。
- 工具面输入：[tool_dispatch/context.rs](../../src-tauri/src/ai_runtime/tool_dispatch/context.rs)（`available_tool_names` 注释：必须只报告这些工具）。
- 全目录投影（对照：`all_catalog_tools_as_specs` 过滤 Planned）：同文件。
- 权限与能力分类：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)、[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)。
- 现有测试：`tool_dispatch/runtime.rs` 内两个派发测试——空工具面返回空 `tools`；只报告当前工具面（联网关闭时不宣称 `web_search`，无 Run 面时不宣称 `search_hybrid`）。

## 当前状态

`implementation.state = partial`（静态源码事实：目录项为 `Dispatchable`、读取处理器在位，但目标合同要求的 `request_tools` 参数与消费方在源码中不存在）。`verification.state = none`。

差异归属：`G01`（工具目录目标与现状的差异；`at=acceptance`，修复工作包可以开始）。未执行真实模型验收；不声明合同正确、不声明工具面申请可用。

## 相关合同

- `K10` 工具面版本与派发观察：工具清单、版本与「下一轮更新」语义的唯一权威。
- `K05` 授权范围与冻结确认：`request_tools` 落地时不得绕开授权；本卡只引用。
- `K04` 供应商能力状态与续轮保真：与 `C10` 的能力状态表达区分——本工具读的是 Iris 侧能力摘要，不是供应商协议能力证明。
- 相邻工具：`T01` `system_time_now`、`T02` `app_context_read`；`T04` `skills_list`（技能面事实与工具面事实分离）。
- 需求依据：`N19`（开箱即用工具与用户付费服务并存，能力可查询、可申请）。

<!-- iris:end T03 -->
