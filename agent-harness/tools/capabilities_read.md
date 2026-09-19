# capabilities_read

<!-- iris:object T03 kind=rules file=true -->

`capabilities_read` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T03 -->

<!-- iris:object T03 kind=tool name=capabilities_read owner=M06 -->

## 用途与语义归属

读取当前 AI 能力摘要：联网开关、已启用模型池、以及**本次 Run 已经暴露**的工具清单；不读取凭据明文。语义归属为工具（`M6`），责任模块 `M06`（`C16` 工具目录、工具面与技能接入）。

关于「申请同授权内工具面扩展」（架构定义 §6.5）：`request_tools` 参数已于 2026-09-19 落地为**询问**而不是扩展——它回答「这些工具是否已在本次 Run 的工具面内」，并明确拒绝不在面内的项。**本工具在任何情况下都不开放工具、能力或权限**，也不安装新的工具面版本；因此本卡写的是「已实现的询问语义」，不是架构定义里更强的「申请后安装」语义。两者的差距如实保留在 `G01` 的第 3 项（`spawn_subagent` 参数消费）与架构 §6.5 的目标描述中。

## 参数与消费

目录 schema 声明 `request_tools`：可选的字符串数组，元素为要核对的工具名。处理器 `capabilities_read_tool(state, ctx, args)` 读取该参数并逐项回答：

| 输入情形                    | 回答                                                              | 稳定原因码                                  |
| --------------------------- | ----------------------------------------------------------------- | ------------------------------------------- |
| 参数缺省                    | 返回的摘要与不带该参数时**逐字节相同**（不出现 `requestedTools`） | —                                           |
| 命中本 Run 工具面           | 列入 `requestedTools.available`                                   | —                                           |
| 命中目录但不在本 Run 工具面 | 列入 `unavailable`，明确拒绝                                      | `capability_request_not_in_current_surface` |
| 目录中不存在该名称          | 列入 `unavailable`，与「不在面内」区分                            | `capability_request_unknown_tool`           |
| 数组元素不是字符串          | 列入 `unavailable`                                                | `capability_request_entry_must_be_a_string` |
| 参数不是数组                | 整项拒绝，`available` 为空                                        | `capability_request_must_be_a_string_array` |

**任何一支都不改变工具面**：`tools[]` 与不带参数时完全一致。这是 `G01` 要求的「无效参数必须明确拒绝而不是静默忽略」，也是不得悄悄扩权的边界。

输入来自派发上下文：`ctx.available_tool_names`（本 Run 的工具面）与 `ctx.web_search_enabled`；模型池来自数据库中的 LLM 配置。

返回 `CapabilitySnapshot`：`kind`（固定 `capabilities`）、`web_search_enabled`、`models[]`（`provider_id`、`model`、`configured`）、`tools[]`（`name`、`requires_confirmation`、`access_level`、`cost_class`、`output_policy`、`evidence_policy`），以及**仅当传入 `request_tools` 时**出现的 `requestedTools`。

源码事实（与「能力查询」的用途直接相关，须如实记录）：

1. 工具清单来自 `ctx.available_tool_names` 与全目录的交集，因此**只报告本 Run 当前工具面内的工具**，不把全目录或未授权能力泄露给模型；`available_tool_names` 为空时 `tools` 为空数组。
2. 工具项包含元数据，**不包含 `input_schema`**。模型因此无法凭本工具构造未暴露工具的合法参数；`request_tools` 同样只回报「在不在面内」，不回报 schema，因此这一缺口仍然存在。
3. `configured` 只表达「凭据可用或该 provider 不需要 Key」，同样是布尔事实，不含密钥材料。

目标合同（`G01` 剩余部分）：接受目标工具名申请，由 `C16` 与 `C04` 验证后在**下一轮**安装新版本工具面。本轮实现的是其中的**明确拒绝与明确回答**，**不包含**安装新工具面。

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
- 参数无效（2026-09-19 落地）：`request_tools` 非数组 → `capability_request_must_be_a_string_array`；数组元素非字符串 → `capability_request_entry_must_be_a_string`；名称未登记 → `capability_request_unknown_tool`；已登记但不在本 Run 工具面 → `capability_request_not_in_current_surface`。四支都**明确拒绝并给稳定原因**，且都不改变工具面——这正是 `G01` 要求消除的「静默忽略」；判据见 `capabilities_read_answers_request_tools_without_widening_the_surface` 与 `capabilities_read_rejects_a_malformed_request_tools_value`。
- 具体缺口引用：`G01` 的差异 3（`spawn_subagent` 参数消费）仍 open，归 `Q06`／`D06`；差异 1、2 已收口。
- 用户侧：不得把读能力失败表达为「模型能力降级」（`N17`）。

## 暴露规则

- `default_enabled_without_skill = true`；目标工具面中属普通助手对话「基础 4 个」之一。
- 例外：`ContextMode::ExplicitReferences` 且检索范围不受限时被 `constrain_for_run_context` 隐藏。
- 架构定义 §6.5 要求「对于当前面未包括、但在既有授权内的能力」由本工具承担申请。2026-09-19 落地的是**回答与拒绝**：本工具报告某项是否已在面内并给出稳定原因，**不由本工具安装新工具面**。因此 §6.5 的「申请后下一轮安装」语义仍未被本卡宣告生效，差距由 `G01` 差异 3 与架构 §6.5 的目标描述承载。

## 源码落点

- 目录定义：[tool_catalog/root.rs](../../src-tauri/src/ai_runtime/tool_catalog/root.rs)（`Dispatchable`、`ReadProfile`、`max_results = Some(1)`、`runtime`／`small_snapshot`／`runtime_fact`）。
- 派发分支与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/runtime.rs](../../src-tauri/src/ai_runtime/tool_dispatch/runtime.rs)（`capabilities_read_tool`）。
- 快照构造：[runtime_context.rs](../../src-tauri/src/ai_runtime/runtime_context.rs)（`capability_snapshot()`、`model_pool_snapshots()`、`CapabilitySnapshot`、`ToolCapabilitySnapshot`）。
- 工具面输入：[tool_dispatch/context.rs](../../src-tauri/src/ai_runtime/tool_dispatch/context.rs)（`available_tool_names` 注释：必须只报告这些工具）。
- 全目录投影（对照：`all_catalog_tools_as_specs` 过滤 Planned）：同文件。
- 权限与能力分类：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)、[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)。
- 现有测试：`tool_dispatch/runtime.rs` 内四个派发测试——空工具面返回空 `tools`；只报告当前工具面（联网关闭时不宣称 `web_search`，无 Run 面时不宣称 `search_hybrid`）；`capabilities_read_answers_request_tools_without_widening_the_surface`（缺省参数逐字节不变、三类回答分类正确、工具面不变）；`capabilities_read_rejects_a_malformed_request_tools_value`（畸形入参明确拒绝且不报告任何已授予）。目录侧另有 `capabilities_read_declares_the_parameters_its_dispatcher_consumes`。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，`request_tools` 已在 schema 声明并由处理器消费，四类回答与拒绝路径各有测试）。`verification.state = none`。

差异归属：`G01` 差异 1、2 已收口，差异 3（`spawn_subagent` 的 `context_hint`／`max_rounds`）仍 open，归 `Q06`／`D06`。未执行真实模型验收；不声明合同正确、**不声明「申请后安装新工具面」可用**——本轮实现的是询问与明确拒绝，不是扩展。

## 相关合同

- `K10` 工具面版本与派发观察：工具清单、版本与「下一轮更新」语义的唯一权威。
- `K05` 授权范围与冻结确认：`request_tools` 落地时不得绕开授权；本卡只引用。
- `K04` 供应商能力状态与续轮保真：与 `C10` 的能力状态表达区分——本工具读的是 Iris 侧能力摘要，不是供应商协议能力证明。
- 相邻工具：`T01` `system_time_now`、`T02` `app_context_read`；`T04` `skills_list`（技能面事实与工具面事实分离）。
- 需求依据：`N19`（开箱即用工具与用户付费服务并存，能力可查询、可申请）。

<!-- iris:end T03 -->
