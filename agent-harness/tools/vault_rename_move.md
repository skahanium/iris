# vault_rename_move

<!-- iris:object T36 kind=rules file=true -->

`vault_rename_move` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T36 -->

<!-- iris:object T36 kind=tool name=vault_rename_move owner=M02 -->

## 用途与语义归属

确认后重命名或移动。语义归属「工具（`M2` / 文档服务）」，责任模块 `M02`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.3。

它是**路径变更**动作，并且会连带改写指向该笔记的 `[[wikilink]]`。目录描述写明它「返回 backlinks / wikilinks 影响摘要」，因此它同时是一个有**多文件影响面**的写入动作：目标笔记自身的移动与回链文件的改写属于同一次确认范围。授权判定归 `M02`（`C04`／`C05`），执行归 `C06` 边界（落点 [tool_dispatch/vault.rs](../../src-tauri/src/ai_runtime/tool_dispatch/vault.rs)、[storage/note_move.rs](../../src-tauri/src/storage/note_move.rs)）。

相邻工具：`T35` `vault_create_note`（新建）、`T37` `vault_delete_to_trash`（移入回收站）。

## 参数与消费

| 参数               | 类型               | 必填 | 消费事实                                                                                               |
| ------------------ | ------------------ | ---- | ------------------------------------------------------------------------------------------------------ |
| `path`             | string             | 是   | 源路径；缺失或非字符串时拒绝 `missing path`                                                            |
| `new_path`         | string             | 是   | 目标路径；缺失或非字符串时拒绝 `missing new_path`                                                      |
| `frozen_note_move` | 未在 schema 中声明 | 否   | 处理器读取该键：存在时按它反序列化 `NoteMovePlan` 并跳过重新计算预览；不存在时调用 `prepare_move` 现算 |

关于第三个参数（**待核对**）：`frozen_note_move` 既不在目录 `input_schema` 中，也未在本次核对范围（`src-tauri/src`、`src`）内发现任何生产写入点——只在处理器读取处命中。参数守卫不实现 `additionalProperties` 检查，因此未声明的多余字段不会被拒绝，而是被忽略或（对本工具）读取。

预览与执行的两段前置条件：

1. `prepare_move`：两路径都必须以 `.md` 结尾（否则 `note_move_requires_markdown_paths`）；两次 `validate_user_note_relative_path` 拒绝内部元数据路径与路径别名；`path == new_path` 或目标已存在返回 `file_already_exists`；源笔记被锁定返回 `note_locked`；随后扫描 Vault 内 `.md` 文件，逐行重写指向该笔记的 `[[wikilink]]`，跳过 frontmatter、围栏代码与行内代码/注释，产出一组 `BacklinkEdit { path, before, after }`。改写只匹配三种目标写法（完整路径、去掉 `.md` 的路径、标题词干）；任一**需要改写**的回链文件被锁定时该文件返回 `note_locked`，整次预览失败（不做「跳过锁定文件」的降级）。
2. `execute_move_locked`：在公共 Vault 移动锁内**重新**跑一次 `prepare_move` 并与传入计划做 JSON 等价比较，不一致返回 `note_move_preview_conflict`（预览与实际必须消费同一份事实）；确保源路径有稳定索引行并拒绝目标路径已有历史归属；为源与每个回链文件建立恢复快照（快照缺失或校验不符即整体失败，不写盘）；建立移动日志检查点后先移动文件系统，再提交身份迁移，最后发布回链改写。

返回 `{"type":"vault_rename_move","path":<新路径>,"previousPath":<原路径>,"receipt":…}`；`receipt` 含 `write`、`previousPath`、`appliedPaths`、`pendingPaths`、`recoveryVersions`、`recoveryWarnings`。**`pendingPaths` 非空表示移动已完成而部分回链改写未发布**——这是部分成功，不是整体失败。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["vault.manage"]`；`access_level = WriteMarkdown` 是展示元数据，不得扩大 Run 授权。
- 权限原子（`C04`）：`VaultRenameMove`，**High**，`supported = true`；`reversible_by` 为「version history」。
- 高风险门槛：允许写入的授权记录中，`Session` 作用域的「本会话允许」对 High／Critical 风险被显式拒绝（`agent_permission_session_grant_risk_denied`），因此本工具不能靠会话级授权长期免确认。
- 目录声明 `requires_confirmation = true`：模型提议先冻结变更集再等确认。冻结的 `relative_paths` 由 `path` 与 `new_path` 组成（该集合也是确认后派发时传入的 `confirmed_write_targets`），原始参数与回滚说明（「可重命名或移动回原位置」）随计划持久化，有效期 10 分钟。
- 处理器的授权闭包不只看一个路径：源路径、目标路径与**每个回链文件路径**都各自执行一次 `ensure_note_write_allowed`（Run 存活 → 写目标命中 → 文档策略 `ApplyChange` → 活跃 Skill 范围）。写目标校验以「该路径是否属于冻结目标集合」为判据。
- 静态阅读（**待核对**）：回链文件不在冻结目标集合内，而它们同样走写目标校验，因此当目标笔记存在需要改写的回链时，本次移动**是否会被 `WriteTargetViolation` 整体拒绝**，本卡片只做了静态阅读，未运行验证；该事实未登记为 `G*`／`Q*`。
- 内容基线绑定：本工具没有 `base_content_hash` 参数，冻结集的基线哈希只在笔记恰好出现在本轮上下文材料中时才非空；`revalidate_frozen_hash_pairs` 对空列表直接通过。版本一致性主要由执行期重新预览比较（`note_move_preview_conflict`）承担。
- 静态事实（**待核对**）：`vault.manage` 未出现在 [run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs) 的能力构造中，授权快照输入 `requested_capabilities` 固定为空向量；本工具在当前普通 Run 路径上的可及性未运行验证。

## 副作用

一次成功调用可能产生以下全部效果：

1. 文件系统移动：源路径移入目标路径（含同目录与其他目录两种情况）；
2. 回链改写：逐个改写回链 `.md` 的链接目标，只替换链接文本，不改其他字节；
3. 身份与索引迁移：`files` 行的路径迁移、目标路径的派生索引重建；索引失败只标降级并排队修复，不回滚移动；
4. 恢复快照：源与每个回链文件各一份，写在移动之前；
5. 移动日志：`.iris/operations/moves/<operationId>.json` 记录阶段、物理文件、目录、身份迁移、回链意图（含 before/after 哈希与恢复版本 id），供进程中断后的 `recover_pending` 使用；
6. 写守卫与时序保护：源路径标记移除、目标路径标记新哈希。

除 `.iris/operations/moves/` 下的移动日志外，不在 Vault 内新增其他类型的文件；不写长期记忆，不落证据账本；工具审计只记录参数与结果的形状摘要，不记录路径取值或链接正文。

## 预算

- 预算分类为 `ConfirmedChange`，受 `max_confirmed_change_calls`（当前三类预设均为 6）与总 `max_tool_calls`（当前 24）约束；一次调用只计一次，无论它改写了多少个回链文件——**回链数量不是预算维度**（`K06`）。
- 冻结前检查额度并按 `ToolLoopLimit` 结束；单次冻结最多 6 个操作、6 个目标——目标笔记与其新路径已占 2 个目标名额。
- 不在 `is_discovery()` 名单内；派发包裹在 30 秒超时中；不在自动重试白名单。回链扫描的规模由 Vault 文件数决定，可能触及超时边界。
- 相同参数重复超过 2 次会被循环以 `tool_call_repeated` 拒绝。

## 幂等

**不是幂等动作**，但重复执行不会造成重复移动：第二次调用时目标路径已存在，`prepare_move` 返回 `file_already_exists`，不产生新的文件系统效果。回链改写只替换链接目标文本，对同一改写重复发布是收敛的（`before == after` 的文件不进入改写集合）。

## 取消

取消在两个时点生效：派发入口检查，以及锁内授权闭包对源、目标与每个回链文件路径的检查。文件系统移动开始后（`checkpoint.move_filesystem()` 之后）没有中途取消检查点：此时取消不会回滚已完成的移动，回执与日志保留「已执行前缀」。回链发布失败时保留已发布的前缀并把其余标为 `pendingPaths`（`PendingConflict` 阶段），不假装整体成功。

## 失败反馈

| 情形                       | 反馈                                                                                                    |
| -------------------------- | ------------------------------------------------------------------------------------------------------- |
| Run 已取消                 | 返回 `Cancelled`，不开始新的副作用                                                                      |
| 未经确认                   | 冻结变更集后以 `agent_run_confirmation_pending` 结束派发                                                |
| 参数不是 `.md` 路径        | 拒绝 `note_move_requires_markdown_paths`                                                                |
| 源与目标相同，或目标已存在 | 拒绝 `file_already_exists`                                                                              |
| 预览与实际不一致           | 拒绝 `note_move_preview_conflict`（要求重新读取与重新确认）                                             |
| 目标路径已有历史归属       | 拒绝（身份迁移边界，发生在第一个文件系统副作用之前）                                                    |
| 源或回链文件被锁定         | 拒绝 `note_locked`                                                                                      |
| 恢复快照不可用或校验失败   | 整体失败，不写盘                                                                                        |
| 身份迁移失败               | 文件系统回滚后报 `note_move_identity_migration_failed`；回滚也失败时报 `…_and_disk_compensation_failed` |
| 回链部分发布               | 回执给出 `appliedPaths` 与 `pendingPaths`，阶段为 `PendingConflict`，不报整体成功                       |
| Vault 在提交期间变化       | 拒绝 `note_vault_changed`                                                                               |
| 路径不属于冻结目标         | 拒绝 `WriteTargetViolation`                                                                             |
| 超时                       | `{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`                                        |

以上可恢复错误按 `C14`／`N16` 处理；用户侧不得因一次工具错误提示「模型能力降级」（`N17`）。

## 暴露规则

- 扩展能力目录项（`surface=extended`），位于目录的 `vault` 组；目录声明 `default_enabled_without_skill = false`。
- 运行期须在冻结能力中含 `vault.manage`；工具面装配时 `only_auto = true`（非 Durable 路径）会过滤掉确认类工具，因此本工具只在 Durable 效果路径上进入模型可见集合（`K10`）。
- 在「显式引用且检索范围不受限」的上下文中被隐藏（`constrain_for_run_context` 的允许清单不含本工具）。
- 受限子任务排除本工具：子 Run 只读，回链改写这类多文件副作用不在子 Run 工具面内（`C15`）。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/vault.rs](../../src-tauri/src/ai_runtime/tool_catalog/vault.rs)（`vault_rename_move`，`Dispatchable`、`WriteMarkdown`、`requires_confirmation = true`）。
- 派发与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/vault.rs](../../src-tauri/src/ai_runtime/tool_dispatch/vault.rs)（`vault_rename_move_tool`，含 `frozen_note_move` 分支与逐路径授权）。
- 移动与回链：[storage/note_move.rs](../../src-tauri/src/storage/note_move.rs)（`prepare_move`、`execute_move_locked`、`rewrite_wikilinks`）。
- 检查点与恢复：[storage/move_journal.rs](../../src-tauri/src/storage/move_journal.rs)（`MoveCheckpoint`、`Phase`、`apply_backlinks`、`recover_pending`）。
- 恢复快照：[storage/note_operations.rs](../../src-tauri/src/storage/note_operations.rs)（`protect_snapshot`）。
- 确认与冻结：[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)、[frozen_change_plan.rs](../../src-tauri/src/ai_runtime/frozen_change_plan.rs)。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`；处理器、预览/执行一致性检查、检查点、回链改写与确认门槛都在位）。`verification.state = none`。

不确定处（**待核对**）：回链文件与冻结目标集合的关系（见「授权」）；`frozen_note_move` 的写入点与它在 schema 之外被读取的合规性；`vault.manage` 的产生位置；在高回链数量下 30 秒派发超时的余量。以上均未登记为 `G*`／`Q*`。未执行真实模型验收，本卡片不声明合同正确或用户任务可用。

## 相关合同

- `K05` 授权范围与冻结确认：登记关系为 `consumes`；确认绑定目标、内容、范围、版本与有效期；部分失败保留已执行前缀（只引用，不复制其定义）。
- `K06` 统一预算账本；`K10` 工具面版本与派发观察。
- 相关对象：`M02`、`C04`、`C05`、`C06`、`C16`、`C17`、`C15`；相邻工具 `T35`、`T37`。
- 需求依据：`N16`、`N17`；语义归属与「确认后重命名或移动」的目标合同见架构 §6.3；工具目录目标与现状差异见 `G01`。

<!-- iris:end T36 -->
