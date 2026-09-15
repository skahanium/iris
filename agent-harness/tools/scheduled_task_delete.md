# scheduled_task_delete

<!-- iris:object T34 kind=rules file=true -->

`scheduled_task_delete` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T34 -->

<!-- iris:object T34 kind=tool name=scheduled_task_delete owner=M02 -->

## 用途与语义归属

授权范围内删除登记。语义归属「工具（`M2` / 计划服务）」，责任模块 `M02`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.3。

它是确认类动作：授权判定归 `M02`（`C04`／`C05`），实际删除由 `C06` 边界提交（落点 [tool_dispatch/schedule.rs](../../src-tauri/src/ai_runtime/tool_dispatch/schedule.rs)），删除对象是应用状态层的 `scheduled_tasks` 行，不是笔记文件。

相邻工具：`T32` `scheduled_task_create`（登记）、`T33` `scheduled_task_list`（查询登记）。

## 参数与消费

| 参数 | 类型    | 必填 | 消费事实                                                                                     |
| ---- | ------- | ---- | -------------------------------------------------------------------------------------------- |
| `id` | integer | 是   | 作为 `DELETE FROM scheduled_tasks WHERE id = ?1` 的唯一条件；非整数或缺失时拒绝 `missing id` |

返回 `{ "ok": <deleted > 0>, "id": <id> }`。源码事实：

1. 删除是单条 SQL 语句，没有显式事务包裹，也没有删除前的存在性查询或影响范围确认。
2. `ok = false` 表示没有匹配行（id 不存在），此时处理器仍返回 `Ok`，因此在工具层 `success = true`——**「调用成功」与「确实删除了登记」是两件事**，消费者必须读 `ok`。
3. 目标身份只以 `id` 表达；确认摘要与冻结目标使用 `application://scheduled-tasks/{id}` 表达式（见「授权」）。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["schedule.manage"]`；`access_level = WriteSettings` 只是展示元数据，不得扩大 Run 授权。
- 权限原子（`C04`）：`AppStateWrite`，Medium，`supported = true`；`reversible_by` 回落为「permission settings」。
- 目录声明 `requires_confirmation = true`，因此本工具是**确认类**动作：模型提议不会直接执行。循环先冻结变更集并持久化待确认记录，再以 `agent_run_confirmation_pending` 结束本轮派发；用户在确认入口批准后，`execute_confirmed_frozen_change_set` 才按冻结顺序派发，且此时派发携带的 `confirmed_write_targets` 就是冻结目标集合。
- 冻结内容：[frozen_change_plan.rs](../../src-tauri/src/ai_runtime/frozen_change_plan.rs) 的 `freeze_set` 记录 `tool_call_id`、工具名、目标表达式、原始参数（`change`）、`rollback_summary`（本工具为「可通过应用设置撤销或更新」）、`confirmation_id`、Run／会话／请求身份、Vault 标识与有效期（10 分钟 TTL）；整个计划以 `plan_json` 持久化在 `agent_run_confirmations` 的待确认记录中。
- 静态事实（**待核对**）：处理器不消费 `ToolDispatchContext`，即不做写目标复核、文档策略复核或 Skill 范围复核，删除按 `id` 直接执行。「授权范围内删除登记」当前由**能力门槛 + 冻结确认**承担，运行时没有第二道「该 id 是否属于本轮已呈现登记」的校验；是否为预期设计未在本卡片范围内证实，也未登记为 `G*`／`Q*`。
- 权限摘要：`scope_summary` 只取 `target_path`／`path`／`note_path`，本工具的 `id` 不在其中，因此授权/审计摘要显示 `current request` 而不含目标 id（**待核对**：是否应显示被删除的登记身份）。
- 会话级授权与高风险工具的门槛：本工具风险级别为 Medium，允许写入会话级授权记录；但目录 `requires_confirmation = true` 的判定在循环中先于授权复用判定，因此本工具仍走冻结确认路径。

## 副作用

删除应用状态层的一行登记（`scheduled_tasks`）：

- 不改 `title`／`prompt`／`schedule`，不触发执行，不改其他登记；
- 不写 `.md`、不产生版本快照、不刷新笔记索引（本工具与笔记索引无关）；
- 不落证据账本；
- 工具审计只记录参数与结果的形状摘要（`shape=object, keys=N`），失败时记录错误文本（截断 200 字符），不记录被删除登记的内容；
- 已执行的删除属于已发生事实：本工具只报告回执（`ok`／`id`），不假装回滚。

## 预算

- 预算分类为 `ConfirmedChange`（目录 `requires_confirmation` 优先于访问级别回落），受 `max_confirmed_change_calls`（当前三类预设均为 6）与总 `max_tool_calls`（当前 24）约束；不占用 local／network／external-read／runtime 额度（`K06`）。
- 冻结前即检查额度：`tool_calls + requested > max_tool_calls` 或 `class_used + requested > limit(ConfirmedChange)` 时以 `ToolLoopLimit` 结束，不产生待确认记录。
- 一个模型响应混有确认类与非确认类调用时整批拒绝（`mixed_confirmation_batch`），不允许用批处理绕开确认；单次冻结最多 6 个操作与 6 个目标。
- 不在 `is_discovery()` 名单内；派发包裹在 30 秒超时中；不在自动重试白名单。
- 相同参数重复超过 2 次会被循环以 `tool_call_repeated` 拒绝。

## 幂等

删除按主键执行，重复删除不再改变状态，因此**效果幂等**；但第二次返回 `ok = false`，且工具层的 `success` 仍为 `true`。消费者不得用二次调用的 `success` 推断登记仍存在或被再次删除。

## 取消

派发入口检查 Run 是否已取消（`ensure_run_active`）。处理器内部没有第二个取消检查点：一旦进入 SQL 执行，删除要么完成要么报错，不存在部分删除状态。取消发生在确认之前时，冻结的确认不会被视为已执行；已批准并执行的删除属于已发生事实，只报告回执。

## 失败反馈

| 情形                                   | 反馈                                                                                                 |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Run 已取消                             | 返回 `Cancelled`，不执行删除                                                                         |
| 未经确认                               | 冻结变更集后以 `agent_run_confirmation_pending` 结束本轮派发，不执行删除                             |
| 确认已过期（超过有效期）或计划哈希不符 | 确认记录不可消费，要求重新确认                                                                       |
| 混批提议                               | 整批拒绝 `mixed_confirmation_batch`                                                                  |
| 能力不足                               | `C04` 判定 `Denied`（`tool is outside the immutable Run capability contract`），返回拒绝结果，不执行 |
| 参数类型不符                           | 守卫返回 `{"error":"tool_arguments_invalid"}`，非字段级说明                                          |
| `id` 缺失                              | 处理器拒绝 `missing id`                                                                              |
| `id` 不存在                            | `{ "ok": false, "id": … }`：不是执行错误，也不代表删除成功                                           |
| 数据库写入失败                         | `AppResult` 错误经派发层成为失败结果，不报告成功                                                     |
| 超时                                   | `{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`                                     |

以上可恢复错误按 `C14`／`N16` 处理：返回具体缺口让模型修正，单次失败不终止任务；用户侧不得因此提示「模型能力降级」（`N17`）。

## 暴露规则

- 扩展能力目录项（`surface=extended`），位于目录的 `root` 组；目录声明 `default_enabled_without_skill = false`，确认类动作不进入「无 Skill 即可用的只读基础面」。
- 运行期须在冻结能力中含 `schedule.manage`；工具面装配时 `only_auto = true`（非 Durable 路径）会把确认类工具整体过滤掉，因此本工具只在 Durable 效果路径上进入模型可见集合（`K10`）。
- 在「显式引用且检索范围不受限」的上下文中被隐藏（`constrain_for_run_context` 的允许清单不含本工具）。
- 受限子任务排除本工具：子 Run 白名单不含它，持久副作用始终留在父级确认路径（`C15`）。
- **待核对（静态边界）**：`schedule.manage` 未出现在 [run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs) 的能力构造中，授权快照输入 `RunPolicyRequest.requested_capabilities` 固定为空向量。据此静态阅读，当前普通 Run 路径上没有产生该能力的代码；本工具是否可达未运行验证，该事实未登记为 `G*`／`Q*`。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/root.rs](../../src-tauri/src/ai_runtime/tool_catalog/root.rs)（`scheduled_task_delete`，`Dispatchable`、`WriteSettings`、`requires_confirmation = true`）。
- 派发与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/schedule.rs](../../src-tauri/src/ai_runtime/tool_dispatch/schedule.rs)（`scheduled_task_delete_tool`）。
- 确认与冻结：[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)（`frozen_relative_paths` 的 `application://scheduled-tasks/{id}` 分支、`rollback_summary`、`request_change_confirmation`）、[frozen_change_plan.rs](../../src-tauri/src/ai_runtime/frozen_change_plan.rs)。
- 确认消费：[assistant_commands.rs](../../src-tauri/src/commands/assistant_commands.rs)（`spawn_confirmed_change_execution` → `execute_confirmed_frozen_change_set`，仅对 Durable＋Apply 上下文放行）。
- 能力与权限映射：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)、[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，处理器与确认门槛在位）。`verification.state = none`。

不确定处（**待核对**）：`schedule.manage` 的产生位置与可及性；`id` 是否应出现在确认/审计摘要；「授权范围内」是否还需要对登记身份的运行时校验。以上未登记为 `G*`／`Q*`。未执行真实模型验收，本卡片不声明合同正确或用户任务可用。

## 相关合同

- `K05` 授权范围与冻结确认：登记关系为 `consumes`；确认绑定目标、内容、范围、版本与有效期，确认后执行必须复核确认身份（只引用，不复制其定义）。
- `K06` 统一预算账本（ConfirmedChange 分类额度）；`K10` 工具面版本与派发观察。
- 相关对象：`M02`、`M06`、`C04`、`C05`、`C06`、`C16`、`C17`、`C15`；相邻工具 `T32` `scheduled_task_create`、`T33` `scheduled_task_list`。
- 需求依据：`N16`（可恢复错误先反馈纠偏）、`N17`（不得表达为模型能力降级）；语义归属与目标合同见架构 §6.3。

<!-- iris:end T34 -->
