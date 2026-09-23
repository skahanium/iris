# vault_create_note

<!-- iris:object T35 kind=rules file=true -->

`vault_create_note` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T35 -->

<!-- iris:object T35 kind=tool name=vault_create_note owner=M02 -->

## 用途与语义归属

确认目标与内容后新建。语义归属「工具（`M2` / 文档服务）」，责任模块 `M02`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.3。

它是**新建**动作，不是改写动作：授权与确认判定归 `M02`（`C04`／`C05`），实际落盘由 `C06` 边界调用笔记写入原语完成（落点 [tool_dispatch/vault.rs](../../src-tauri/src/ai_runtime/tool_dispatch/vault.rs)、[storage/note_operations.rs](../../src-tauri/src/storage/note_operations.rs) 的 `create_note`）。目标路径已存在时本工具不会覆盖，也不改写成改写动作——覆盖既有笔记需要其他受控路径。

相邻工具：`T36` `vault_rename_move`（重命名／移动）、`T37` `vault_delete_to_trash`（移入回收站）、`T38` `vault_asset_write`（写资源）。

## 参数与消费

| 参数          | 类型   | 必填 | 消费事实                                                                           |
| ------------- | ------ | ---- | ---------------------------------------------------------------------------------- |
| `target_path` | string | 是   | Vault 相对 `.md` 路径；缺失或非字符串时拒绝 `missing target_path`                  |
| `content`     | string | 否   | 初始 Markdown 正文；非字符串时按空串处理（`as_str().unwrap_or("")`），**不是拒绝** |

处理器链路与逐项前置条件：

1. 在公共 Vault 移动锁内执行，锁内先复核当前 Vault 未切换（否则 `note_vault_changed`）。
2. `is_user_note_path(path)` 必须成立，且路径以 `.md` 结尾，且正文长度不超过 20 × 1024 × 1024 字节（`content.len()`，按 UTF-8 字节计）；任一不满足返回 `invalid_note_create`。因此 `.iris/**` 与 `.classified/**` 都不能经本工具创建，涉密笔记不走这条路径。
3. 授权闭包在写盘前执行（见「授权」）。
4. 落盘走 `NoteWriteService::create_under_move_lock` → `reject_existing = true` → `atomic_create`（以硬链接发布，目标已存在即失败），**先写临时文件再发布**，不覆盖既有文件。
5. 写盘后让派生索引跟上：索引刷新失败只把回执标为降级并排队修复，不回滚已写文件。

返回 `{"type":"vault_create_note","path":…,"receipt":…}`，`receipt` 是 `FileWriteResult` 的 camelCase 序列化（`entry`、`contentHash`、`indexStatus`；`operation` 仅在影响额外文件时出现）。**新建不产生版本快照**：本路径不调用恢复快照原语。

目录定义中 `content` 未列入 `required`，因此「创建空笔记」是合法调用。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["vault.manage"]`；`access_level = WriteMarkdown` 是展示元数据，不得扩大 Run 授权。
- 权限原子（`C04`）：`VaultCreateNote`，Medium，`supported = true`；`reversible_by` 为「version history」。
- 目录声明 `requires_confirmation = true`：本工具是确认类动作，模型提议先冻结变更集，用户确认后才执行。冻结记录目标路径、原始参数（含 `content`）、回滚说明与身份信息，并以 `plan_json` 持久化；确认有效期 10 分钟。
- 目标边界：`ensure_note_write_allowed` 依次执行 Run 存活检查 → 写目标必须命中用户显式应用目标或已消费冻结目标（否则 `WriteTargetViolation`）→ 文档策略的 `ApplyChange` 能力（拒绝时 `agent_run_document_policy_denied`）→ 活跃 Skill 范围（拒绝时为 `target path is outside the confirmed Skill scope`）。
- 内容与版本绑定：本工具没有 `base_content_hash` 参数，创建本无基线，因此冻结集的**基线哈希为空**，内容基线由「目标必须不存在」这一前置条件保证；`revalidate_frozen_hash_pairs` 对空列表不产生额外复验。
- 静态事实（**待核对**）：`vault.manage` 未出现在 [run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs) 的能力构造中，授权快照输入 `RunPolicyRequest.requested_capabilities` 固定为空向量。据此静态阅读，当前普通 Run 路径上没有产生该能力的代码；本工具是否可达未运行验证，也未登记为 `G*`／`Q*`。

## 副作用

在 Vault 内新建一个 `.md` 文件，并连带派生状态更新：

- 文件系统：新建目标文件（不覆盖、不改其他文件）；
- 索引：刷新该路径的派生索引行；失败时回执 `indexStatus = degraded` 并排队修复，Markdown 事实保留；
- 写守卫：标记该路径的内容哈希，供后续漂移比较；
- 不产生版本快照、不写回收站、不写长期记忆、不落证据账本；
- 工具审计只记录参数与结果的形状摘要（`shape=object, keys=N`），不记录 `content` 正文或路径取值；
- 已写入的文件属于已发生事实：失败后本工具只报告回执，不假装回滚。

## 预算

- 预算分类为 `ConfirmedChange`（确认类优先于访问级别回落），受 `max_confirmed_change_calls`（当前三类预设均为 6）与总 `max_tool_calls`（当前 24）约束；不占用 local／network／external-read／runtime 额度（`K06`）。
- 冻结前检查额度，额度不足以 `ToolLoopLimit` 结束；混有非确认类调用的整批被拒绝（`mixed_confirmation_batch`）；单次冻结最多 6 个操作、6 个目标。
- 不在 `is_discovery()` 名单内；派发包裹在 30 秒超时中；不在自动重试白名单。
- 相同参数重复超过 2 次会被循环以 `tool_call_repeated` 拒绝；已成功的同指纹调用以 `tool_call_already_succeeded` 拒绝。

## 幂等

**不是幂等动作，但重复调用会失败而不是产生副本**：第二次执行时目标已存在，`atomic_create` 的硬链接发布失败，整次调用报错，既有文件内容不被改动。要「改内容」必须走其他受控写入路径并重新确认。

## 取消

取消在两个时点生效：派发入口的 Run 存活检查，以及锁内授权闭包的首个检查（`ensure_note_write_allowed` → `ensure_run_active`）。因此取消后不开始新的持久写入。已经完成的创建只报告回执，不假装回滚。

## 失败反馈

| 情形                                            | 反馈                                                                                    |
| ----------------------------------------------- | --------------------------------------------------------------------------------------- |
| Run 已取消                                      | 返回 `Cancelled`，不写入                                                                |
| 未经确认                                        | 冻结变更集后以 `agent_run_confirmation_pending` 结束派发，不写入                        |
| 确认前目标或内容已变化／确认过期                | 冻结身份或有效期不再匹配时确认不可消费，要求重新确认                                    |
| 目标不在冻结范围                                | 拒绝 `WriteTargetViolation`                                                             |
| 路径不是用户笔记路径、不是 `.md`、或正文超 20MB | 拒绝 `invalid_note_create`                                                              |
| 目标已存在                                      | 底层发布失败（不覆盖、不改写既有文件）                                                  |
| 笔记被锁定                                      | 拒绝 `note_locked`                                                                      |
| Vault 在提交期间变化                            | 拒绝 `note_vault_changed`                                                               |
| 文档策略或 Skill 范围拒绝                       | `agent_run_document_policy_denied` / `target path is outside the confirmed Skill scope` |
| 参数类型不符                                    | `{"error":"tool_arguments_invalid"}`（非字段级说明）                                    |
| 超时                                            | `{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`                        |

以上可恢复错误按 `C14`／`N16` 处理：返回具体缺口让模型修正；用户侧不得因一次工具错误提示「模型能力降级」（`N17`）。

## 暴露规则

- 扩展能力目录项（`surface=extended`），位于目录的 `vault` 组；目录声明 `default_enabled_without_skill = false`，确认类动作不进入「无 Skill 即可用的只读基础面」（该字段当前只有测试消费者，写工具的排除由测试断言）。
- 运行期须在冻结能力中含 `vault.manage`；工具面装配时 `only_auto = true`（非 Durable 路径）会过滤掉确认类工具，因此本工具只在 Durable 效果路径上进入模型可见集合（`K10`）；进入工具面不等于获得授权，授权仍由 `C04` 判定。
- 在「显式引用且检索范围不受限」的上下文中被隐藏（`constrain_for_run_context` 的允许清单不含本工具）。
- 受限子任务排除本工具：子 Run 白名单只含只读词汇，持久副作用留在父级确认路径（`C15`）。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/vault.rs](../../src-tauri/src/ai_runtime/tool_catalog/vault.rs)（`vault_create_note`，`Dispatchable`、`WriteMarkdown`、`requires_confirmation = true`）。
- 派发：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/vault.rs](../../src-tauri/src/ai_runtime/tool_dispatch/vault.rs)（`vault_create_note_tool`）。
- 写入原语：[storage/note_operations.rs](../../src-tauri/src/storage/note_operations.rs)（`create_note`）、[storage/note_write.rs](../../src-tauri/src/storage/note_write.rs)（`create_under_move_lock`／`write_body`）、[storage/atomic_write.rs](../../src-tauri/src/storage/atomic_write.rs)（`atomic_create`、`with_vault_move_lock`）。
- 目标与范围校验：[tool_dispatch/context.rs](../../src-tauri/src/ai_runtime/tool_dispatch/context.rs)（`ensure_note_write_allowed`、`ensure_write_target_matches`）。
- 确认与冻结：[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)、[frozen_change_plan.rs](../../src-tauri/src/ai_runtime/frozen_change_plan.rs)。
- 能力与权限映射：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)、[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`、处理器、锁内前置条件与确认门槛在位）。`verification.state = none`。

不确定处（**待核对**）：`content` 非字符串时按空串处理是否符合 `C16`「所有暴露参数必须消费或明确拒绝」的目标要求；`vault.manage` 的产生位置与可及性；创建动作没有版本快照时如何满足用户可撤销的期望（本卡片不新设该承诺）。以上未登记为 `G*`／`Q*`。未执行真实模型验收，不声明合同正确或用户任务可用。

## 相关合同

- `K05` 授权范围与冻结确认：登记关系为 `consumes`；确认绑定目标、内容、范围、版本与有效期，写前复核路径身份（只引用，不复制其定义）。
- `K06` 统一预算账本；`K10` 工具面版本与派发观察。
- 相关对象：`M02`、`C04`、`C05`、`C06`、`C16`、`C17`、`C15`；相邻工具 `T36`、`T37`、`T38`。
- 需求依据：`N16`、`N17`；语义归属与目标合同见架构 §6.3；目录目标与现状差异见 `G01`。

<!-- iris:end T35 -->
