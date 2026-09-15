# app_context_read

<!-- iris:object T02 kind=rules file=true -->

`app_context_read` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T02 -->

<!-- iris:object T02 kind=tool name=app_context_read owner=M01 -->

## 用途与语义归属

读取获准的当前应用上下文，让模型知道本次请求落在哪个 vault、哪篇笔记、附件有多少，从而不再靠猜测用户当前的工作位置。语义归属为工具（`M1`／`M3`），责任模块登记为 `M01`（`C01` 请求与引用绑定）。

它读的是**请求身份与引用的周边事实**，不是笔记正文：正文读取属于 `T06` `read_note`。快照中不存在笔记内容、聊天历史、密钥或凭据明文。

## 参数与消费

目录 schema 声明为空对象 `{"type":"object","properties":{}}`，**不声明任何参数**。处理器 `app_context_read_tool(state, ctx)` 不读取 `args`，其全部输入来自派发上下文 `ToolDispatchContext`：`note_path`、`file_id`、`attachment_count`，以及 `AppState::vault_path()`。

返回结构对应 `AppContextSnapshot`：`kind`（固定 `app_context`）、`vault_path`、`note_path`、`note_title`、`file_id`、`selection_present`、`selection_excerpt`、`attachment_count`。

**目标合同与现状的差异**：架构定义把本工具的语义归属写为「读取获准的当前应用上下文」，而 `app_context_snapshot()` 把 `selection_present` 固定为 `false`、`selection_excerpt` 固定为 `None`。也就是说，**选区信息在派发上下文中已经丢弃**（`ToolDispatchContext` 没有选区字段），尽管 `run_context.rs` 的 `RuntimeContextInput` 存在 `selection_excerpt` 并用于系统提示。因此「获准的选区是否应可通过本工具读取」在源码中**没有实现，也没有登记为缺口**——本卡把它标为**待核对**：需要 `C01` 明确「应用上下文」是否包含选区身份，若包含则须登记为 `G*` 或补实现。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["runtime.read"]`；`run_intake` 对每个 Run 恒定加入 `runtime.read`，运行时事实不授予文件系统或网络访问。
- 权限原子（`C04`）：`Atom::RuntimeContextRead`，Low，`supported = true`，`requires_confirmation = false`。
- 不因联网开关关闭而失效；不读取任何路径下的正文，因此不触发文档策略的外发判定，也不受 Vault 检索范围限制。`note_path` 是请求上下文里已经确定的引用，不是模型可选的目标。
- 子任务继承：只读、低风险，随父级冻结能力快照进入子运行。

## 副作用

**无文件写入、无数据库写入。** 唯一的数据库读取是 `note_title()` 对 `files.title` 的一次 `SELECT`（按 `note_path` 精确匹配），不改行、不升级配置、不写审计以外的状态。输出策略 `small_snapshot`，证据策略 `runtime_fact`。凭据安全：`credential_service`／凭据明文不进入本工具路径。

## 预算

- `cost_class = "runtime"`，归入 `ToolBudgetClass::Runtime`，与 `T01`、`T03` 共用同一类别上限（当前预设 `max_runtime_tool_calls = 4`，`max_tool_calls = 24`；属主循环分类额度，不是全局统一总数）。
- 不进每回合 2 次发现调用上限（`is_discovery()` 不含本工具）。
- 派发包裹在 30 秒超时中。
- 具体数值的唯一权威是 `K06`；本卡不新设数值。

## 幂等

纯读取，重复调用不产生副作用。**返回值不保证稳定**：`note_path`／`file_id`／`attachment_count` 随 Run 的引用绑定，跨 Run 或用户切换笔记后会变；`vault_path` 随 vault 切换而变。取值应以「同一次 Run 内的一次观察」为单位，不得把两次调用的结果当作同一份一致性快照。

## 取消

派发前执行 `ctx.ensure_run_active()`；该 Run 已请求取消时返回取消错误而不构造快照。处理器为同步调用（含一次同步数据库读），没有执行中途的取消窗口；超时通道独立于取消通道。不留下部分结果。

## 失败反馈

- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", ...}`。
- 缺失可选字段不是错误：`note_path` 为 `None` 时 `note_path` 与 `note_title` 返回 `null`，属正常「无当前笔记」表达。
- `note_title()` 内部忽略查询失败（`.ok()`），因此标题缺失与标题查询报错在模型侧不可区分；若需要区分，**待核对**是否应返回明确状态而不是 `null`。
- 参数无效：本工具无参数；未声明字段当前被静默忽略（guardrails 不校验 `additionalProperties`），与 `G01` 的「无效参数必须明确拒绝」要求不一致。
- 用户侧表达：不得因单次失败提示能力降级（`N17`）。

## 暴露规则

- `default_enabled_without_skill = true`；目标工具面中属普通助手对话「基础 4 个」之一。
- 例外：`ContextMode::ExplicitReferences` 且检索范围不受限时被 `constrain_for_run_context` 隐藏（允许清单不含本工具）。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/root.rs](../../src-tauri/src/ai_runtime/tool_catalog/root.rs)（`Dispatchable`、`ReadProfile`、`max_results = Some(1)`、`runtime`／`small_snapshot`／`runtime_fact`）。
- 派发分支与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/runtime.rs](../../src-tauri/src/ai_runtime/tool_dispatch/runtime.rs)。
- 快照构造与选区丢弃：[runtime_context.rs](../../src-tauri/src/ai_runtime/runtime_context.rs) 的 `app_context_snapshot()`、`note_title()`。
- 上下文输入结构（含选区字段的对照）：[tool_dispatch/context.rs](../../src-tauri/src/ai_runtime/tool_dispatch/context.rs)。
- 权限与能力分类：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)、[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)。
- 现有测试：`tool_dispatch/runtime.rs` 内的派发测试（断言 `note_path`、`attachment_count`、`vault_path`）。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，处理器与派发分支在位）。`verification.state = none`。

已知不确定处（见上）：选区字段在快照中被固定为空，是否属于合同缺口**待核对**；`note_title` 查询失败与标题缺失不可区分**待核对**。未执行真实模型验收，不声明合同正确或用户任务可用。

## 相关合同

- `K01` 请求身份与引用绑定：本工具读取的事实由该合同确立其身份（会话、Vault、文档版本、选区）；本卡只引用，不复制定义。
- `K05` 授权范围与冻结确认：运行时不扩权。
- `K10` 工具面版本与派发观察。
- 相邻工具：`T01` `system_time_now`、`T03` `capabilities_read`（同属运行时事实组）；`T06` `read_note`（正文读取的独立边界）。
- 需求依据：`N01`（基础对话可靠）要求模型不靠关键词猜测当前上下文。

<!-- iris:end T02 -->
