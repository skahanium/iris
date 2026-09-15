# vault_delete_to_trash

<!-- iris:object T37 kind=rules file=true -->

`vault_delete_to_trash` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T37 -->

<!-- iris:object T37 kind=tool name=vault_delete_to_trash owner=M02 -->

## 用途与语义归属

确认后移入回收站。语义归属「工具（`M2` / 文档服务）」，责任模块 `M02`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.3。

它是**可恢复删除**动作，不是永久删除：目录描述明确「将用户笔记移入 Iris 回收站，而不是永久删除」。授权与确认判定归 `M02`（`C04`／`C05`），实际移动与回收站元数据由 `C06` 边界调用笔记与回收站原语完成（落点 [tool_dispatch/vault.rs](../../src-tauri/src/ai_runtime/tool_dispatch/vault.rs)、[storage/note_operations.rs](../../src-tauri/src/storage/note_operations.rs)、[recycle/mod.rs](../../src-tauri/src/recycle/mod.rs)）。

相邻工具：`T35` `vault_create_note`、`T36` `vault_rename_move`、`T06` `read_note`（提供删除前基线哈希）。

## 参数与消费

| 参数                | 类型   | 必填 | 消费事实                                                                                                                  |
| ------------------- | ------ | ---- | ------------------------------------------------------------------------------------------------------------------------- |
| `path`              | string | 是   | 目标笔记路径；缺失或非字符串时拒绝 `missing path`                                                                         |
| `base_content_hash` | string | 是   | 目录描述为「`read_note` 返回的完整文档 hash；确认后发生变更会拒绝删除」；缺失或非字符串时拒绝 `missing base_content_hash` |

执行顺序（每一步都先于下一个副作用）：

1. 在公共 Vault 移动锁内：当前 Vault 未切换（否则 `note_vault_changed`）→ 授权闭包 → `validate_user_note_relative_path`（拒绝内部元数据路径与路径别名）→ 读取当前文件内容并重算哈希，与 `base_content_hash` 不等即返回 `note_content_conflict`。
2. 笔记写入服务确认该路径未被锁定（`note_locked`）。
3. 生成 `trashId`，把可读的版本快照复制进 `.iris/trash/<trashId>/versions/`；**不可读的快照不阻断删除**：保留原版本行与原始引用，仅在回收清单里标 `unreadable` 并记警告。
4. 写 `<bundle>/manifest.json`：原始路径、标题、删除时间、到期时间（删除时间 + 15 天）与版本清单。
5. `move_file_no_replace_locked` 把文档移入 `<bundle>/document.md`（同目录发布，不覆盖既有回收条目）。
6. 在 `BEGIN IMMEDIATE` 事务内归档版本行归属、为 `recycle_bin` 插入记录、删除该路径的派生文件索引行；任一步失败即回滚事务并把回执标为 `metadataPending = true`；版本归属不匹配时另记 `metadataError = recycled_version_ownership_mismatch`。

返回 `{"type":"vault_delete_to_trash","path":…,"trashId":…,"receipt":…}`，`receipt` 含 `path`、`trashId`、`contentHash`、`metadataPending`，以及仅在存在时出现的 `metadataError`。

**`metadataPending = true` 的含义是「文档已移入回收站，元数据未提交」**，不是「删除失败」，也不是「整体成功」：文件系统事实与回收站登记事实分别表达，消费者不得把二者压成一个布尔值。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["vault.manage"]`；`access_level = WriteMarkdown` 是展示元数据，不得扩大 Run 授权。
- 权限原子（`C04`）：`VaultDeleteToTrash`，**High**，`supported = true`；`reversible_by` 为「recycle bin restore」。
- 高风险门槛：`Session` 作用域的会话级允许对 High 风险被显式拒绝（`agent_permission_session_grant_risk_denied`），删除不能靠长期会话授权免确认。
- 目录声明 `requires_confirmation = true`：提议先冻结变更集再等确认。冻结记录目标路径、原始参数、回滚说明（「可从回收站恢复」）与身份信息，有效期 10 分钟。
- **内容基线进入冻结集**：`frozen_base_content_hashes` 会把 `base_content_hash` 参数作为该目标路径的基线哈希写入计划；确认后的派发前会重算当前文件哈希，漂移则本次操作以 `frozen_change_base_hash_drift` 失败且不执行删除。处理器内部还会再做一次同样的比较（`note_content_conflict`），两道检查的对象相同、失败码不同。
- 目标边界：`ensure_note_write_allowed` 的写目标校验、文档策略 `ApplyChange` 与活跃 Skill 范围都适用于目标路径（拒绝码见「失败反馈」）。
- 静态事实（**待核对**）：`vault.manage` 未出现在 [run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs) 的能力构造中，授权快照输入 `requested_capabilities` 固定为空向量；本工具在当前普通 Run 路径上的可及性未运行验证，也未登记为 `G*`／`Q*`。

## 副作用

- 文件系统：`<vault>/.iris/trash/<trashId>/document.md`、`versions/` 副本与 `manifest.json`；源路径不再存在；
- 数据库：版本行归档（归属到回收条目）、`recycle_bin` 插入、派生文件索引行删除（同一事务）；
- 写守卫：标记源路径已移除；
- 不产生新的版本快照（删除的是当前内容，恢复依据是既有历史与回收副本），不写长期记忆，不落证据账本；
- 工具审计只记录参数与结果的形状摘要，不记录路径或内容；
- 回收条目按 15 天保留期登记（到期后的清理属于回收站自身策略，不在本工具内执行）。

## 预算

- 预算分类为 `ConfirmedChange`，受 `max_confirmed_change_calls`（当前三类预设均为 6）与总 `max_tool_calls`（当前 24）约束；不占用 local／network／external-read／runtime 额度（`K06`）。
- 冻结前检查额度；混批提议整批拒绝；单次冻结最多 6 个操作与 6 个目标。
- 不在 `is_discovery()` 名单内；派发包裹在 30 秒超时中——版本快照复制规模较大时可能触及超时边界；不在自动重试白名单。

## 幂等

**不是幂等动作**：第一次调用后源路径已不存在，第二次调用在读取当前内容时失败（文件不存在），不会产生第二个回收条目，也不会重复归档。因此重试不会造成重复回收；要再删一次必须重新读取基线并重新确认。

## 取消

取消在两个时点生效：派发入口检查与锁内授权闭包的首个检查；此外确认后的派发前还有冻结基线复验。取消后不开始新的持久副作用。已经完成的移动与元数据提交属于已发生事实：本工具只报告回执（含 `metadataPending`），不假装回滚。

## 失败反馈

| 情形                       | 反馈                                                                                               |
| -------------------------- | -------------------------------------------------------------------------------------------------- |
| Run 已取消                 | 返回 `Cancelled`，不移动文件                                                                       |
| 未经确认                   | 冻结变更集后以 `agent_run_confirmation_pending` 结束派发                                           |
| 确认后内容已变化           | 派发前 `frozen_change_base_hash_drift`；处理器内 `note_content_conflict`——都要求重新读取与重新确认 |
| 目标不在冻结范围           | 拒绝 `WriteTargetViolation`                                                                        |
| 笔记被锁定                 | 拒绝 `note_locked`                                                                                 |
| Vault 在提交期间变化       | 拒绝 `note_vault_changed`                                                                          |
| 文档策略或 Skill 范围拒绝  | `agent_run_document_policy_denied` / `target path is outside the confirmed Skill scope`            |
| 路径为内部元数据或路径别名 | 路径校验拒绝（不访问 `.iris/**`）                                                                  |
| 元数据事务失败             | 回执 `metadataPending = true`，文件系统事实保留；归属冲突另带 `metadataError`                      |
| 某个版本快照不可读         | 不阻断删除，清单标 `unreadable` 并记警告                                                           |
| 参数类型不符               | `{"error":"tool_arguments_invalid"}`（非字段级说明）                                               |
| 超时                       | `{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`                                   |

可恢复错误按 `C14`／`N16` 处理；`metadataPending` 属「部分成功」，不得被投影成整体失败或整体成功（`K05` 的已执行前缀要求）；用户侧不得因一次工具错误提示「模型能力降级」（`N17`）。

## 暴露规则

- 扩展能力目录项（`surface=extended`），位于目录的 `vault` 组；目录声明 `default_enabled_without_skill = false`。
- 运行期须在冻结能力中含 `vault.manage`；工具面装配时 `only_auto = true`（非 Durable 路径）会过滤掉确认类工具，因此本工具只在 Durable 效果路径上进入模型可见集合（`K10`）。
- 在「显式引用且检索范围不受限」的上下文中被隐藏（`constrain_for_run_context` 的允许清单不含本工具）。
- 受限子任务排除本工具：子 Run 只读，删除类副作用留在父级确认路径（`C15`）。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/vault.rs](../../src-tauri/src/ai_runtime/tool_catalog/vault.rs)（`vault_delete_to_trash`，`Dispatchable`、`WriteMarkdown`、`requires_confirmation = true`）。
- 派发与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/vault.rs](../../src-tauri/src/ai_runtime/tool_dispatch/vault.rs)（`vault_delete_to_trash_tool`）。
- 笔记原语：[storage/note_operations.rs](../../src-tauri/src/storage/note_operations.rs)（`trash_note`）。
- 回收站：[recycle/mod.rs](../../src-tauri/src/recycle/mod.rs)（`trash_locked`、`TrashReceipt`、`RECYCLE_RETENTION_DAYS`、`archive_rows` 调用）。
- 确认与冻结：[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)（`frozen_relative_paths`／`frozen_base_content_hashes`／`revalidate_frozen_hash_pairs`／`rollback_summary`）、[frozen_change_plan.rs](../../src-tauri/src/ai_runtime/frozen_change_plan.rs)。
- 契约测试：[tool_catalog/tests.rs](../../src-tauri/src/ai_runtime/tool_catalog/tests.rs)（断言 `path` 与 `base_content_hash` 都在 `required` 中）。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`、处理器、基线哈希比较、回收站原语与确认门槛在位）。`verification.state = none`。

不确定处（**待核对**）：`vault.manage` 的产生位置与可及性；`metadataPending` 在用户侧的表达（本卡片只记录内部事实）；大版本历史下的 30 秒派发超时余量。以上未登记为 `G*`／`Q*`。未执行真实模型验收，本卡片不声明合同正确或用户任务可用。

## 相关合同

- `K05` 授权范围与冻结确认：登记关系为 `consumes`；部分失败保留已执行前缀，写入回执未知时先核实回执（只引用，不复制其定义）。
- `K06` 统一预算账本；`K10` 工具面版本与派发观察。
- 相关对象：`M02`、`C04`、`C05`、`C06`、`C16`、`C17`、`C15`；相邻工具 `T06` `read_note`、`T36`、`T39`。
- 需求依据：`N16`、`N17`；语义归属与「确认后移入回收站」的目标合同见架构 §6.3；工具目录目标与现状差异见 `G01`。

<!-- iris:end T37 -->
