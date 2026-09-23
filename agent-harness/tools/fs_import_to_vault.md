# fs_import_to_vault

<!-- iris:object T21 kind=rules file=true -->

`fs_import_to_vault` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T21 -->

<!-- iris:object T21 kind=tool name=fs_import_to_vault owner=M02 -->

## 用途与语义归属

把用户授权的**外部 Markdown 文件**导入 vault 的确认目标路径；目标已存在时必须走覆盖路径，且覆盖要求提供已读正文的 `base_content_hash`。语义归属为工具（`M2` / 文档服务），责任模块 `M02`；目录归属 `boundary` 组（[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)），暴露面 `extended`。

架构定义给本项的目标合同与取舍原文是「**已授权外部来源导入；覆盖需版本校验**」（[架构定义](../../docs/agent-architecture.md) §6.3）。两句取舍分别落在：

- **已授权外部来源**：源文件必须位于 `authorized_root` 之内，路径经规范化与越界校验；`authorized_root` 为必填参数，不是可选提示。这属于「外部目录读取」这一族边界（与 `T23` 同族），但本项在读取之后**继续产生 vault 写入**。
- **覆盖需版本校验**：`overwrite = true` 时 `base_content_hash` 为必填，且写入经 `NoteEdit` 的版本复验；基准不符即拒绝，不静默覆盖。

本卡只引用上述措辞，不重复其他对象的正文；写入侧的统一提交边界是 `C06`（见 [m02-permission-and-side-effects.md](../modules/m02-permission-and-side-effects.md)），导出族的兼容关系同样写在该组件合同中。

## 参数与消费

| 参数                | 类型    | 声明边界                           | 处理器行为                                                                                                                                                                                                                             |
| ------------------- | ------- | ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `source_path`       | string  | **required**                       | 必填；经 `resolve_external_input` 规范化：不存在 → `source_path does not exist`，越出 `authorized_root` → `source_path is outside authorized_root`，不是文件 → `source_path must be a file`，含 `..` → `Path traversal is not allowed` |
| `authorized_root`   | string  | **required**                       | 必填；必须已存在且是目录（`authorized_root must exist`／`must be a directory`）；命中系统敏感前缀 → `不允许访问系统目录`                                                                                                               |
| `target_path`       | string  | **required**                       | 必填；必须是 vault 内合法用户笔记且以 `.md` 结尾，否则 `invalid_note_import_target`                                                                                                                                                    |
| `base_content_hash` | string  | 声明「仅 `overwrite=true` 时必填」 | 仅覆盖路径读取：`overwrite = true` 时缺失即 `missing base_content_hash`；非覆盖路径**传入也不消费**                                                                                                                                    |
| `overwrite`         | boolean | 默认 false                         | 非布尔或缺失按 false                                                                                                                                                                                                                   |

参数与消费的一致性总体成立（每个声明参数都有对应行为），但有两处**待核对**的口径细节：① 非覆盖路径下传入 `base_content_hash` 会被忽略而不报错（`K10` 要求「消费或明确拒绝」）；② schema 无 `additionalProperties: false`，额外字段被静默忽略。二者沿用 `G01` 的证据要求（工具卡逐项实现状态与目录分类的一致性核对）。

写入前的处理器侧顺序（源码事实）：

1. `target_path` 形状校验（用户笔记路径 + `.md`）；
2. `ctx.ensure_note_write_allowed(db, target_path)`（运行活跃性 → 写入目标与冻结确认一致 → 文档策略 `ApplyChange` 能力 → 活跃技能作用域）；
3. 解析并读取外部源文件（`String` 读取，**对源文件扩展名不作限制**）；
4. 覆盖路径：读取目标现有正文 → 构造整篇替换的 `NoteEdit`（`range = 0..original.len()`）→ `apply_edit`（内部再次复核授权与写入目标一致性）；非覆盖路径：`create_note`。

返回：`type`（`"fs_import_to_vault"`）、`path`（写入结果中的条目路径）、`bytes`（导入正文字节数）、`title`、`indexStatus`、`receipt`（覆盖路径为 `NoteEditReceipt` 序列化结果：`path`／`beforeHash`／`afterHash`／`versionId`／`write`；非覆盖路径为 `FileWriteResult`）。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["external_fs.import"]`——与本项「外部读 + vault 写」的复合效果对应的**单一专属能力**，不复用 `external_fs.read` 或 `external_fs.export`。
- 权限原子（`C04`）：`Atom::FsImportToVault`，**`Risk::High`，`supported = true`**。高风险意味着不能写入会话级授权（`upsert_permission_grant` 拒绝 `AllowForSession` 于 High／Critical）。
- 确认语义：目录项 `requires_confirmation = true` → `preflight` 给出 `PermissionDecision::AllowOnce` → 判定结果为 `RequiresConfirmation`。**本项必须经 `C05` 冻结确认后才能提交副作用**，模型不能凭一次工具提议完成导入。
- 工具面过滤的一个非平凡后果：非 Durable 的 Run 用 `only_auto = true` 构建工具面，**直接过滤掉需要确认的工具**；因此在这些 Run 中本项不出现在工具面上。Durable 路径使用 `only_auto = false`，本项可被提议，但提议之后仍必须完成确认才能执行。
- 文档策略与作用域：写入前 `ensure_note_write_allowed` 会重新判定运行活跃性、写入目标与确认一致、文档策略的 `ApplyChange` 能力、技能作用域——**授权判定发生在卡内，而不是只发生在工具面构建时**。
- 子任务：**不在** `CHILD_SAFE_TOOLS` 白名单中；子运行不得借助本项产生 vault 写入（一层委派只读）。
- 参数化授权差异（**待核对**）：权限作用域的摘要取 `target_path`／`path`／`note_path` 之一，本项覆盖 `target_path`；`authorized_root` 本身**不进入作用域摘要**，因此「这个目录是否被授权」由用户确认动作承担，而不是由一个目录级授予记录比对。

## 副作用

**有真实副作用：一次 vault 写入 + 一次可能的恢复版本快照。**

- 非覆盖路径：`create_note` → `NoteWriteService::create_under_move_lock`，带**存在性前置条件**（不覆盖已有笔记）；失败路径不写入。
- 覆盖路径：`apply_edit` → 保护一份恢复快照（`protect_snapshot`，快照缺失或校验失败即报 `note_recovery_snapshot_missing`／`note_recovery_snapshot_invalid`），再在同一把 vault move 锁内复核授权并写入。
- 索引：写入后更新派生索引。**索引刷新失败不使写入回滚**：结果以 `indexStatus = Degraded` 返回并调度修复，**工具仍然报告成功**。消费者必须把「正文已落盘」与「派生索引已同步」当作两个事实（`M06` 的目录分类一致性要求与 `G01` 同一方向）。
- 外部源文件只读，不改动、不删除、不移动。
- 覆盖路径的写入是整篇替换：`range = 0..original.len()`，`original` 为当前全文。这意味着**源文件内容会完整替换目标笔记正文**，包括目标原有的 frontmatter；本卡只陈述该实现事实，不为其正确性背书。

## 预算

- `budget_class()` 归入 `ToolBudgetClass::ConfirmedChange`（无 `execution_metadata`，但 `requires_confirmation = true` 优先于 `access_level` 的分支）。
- 当前生产预设（Standard／Delegated／DurableApply）`max_confirmed_change_calls = 6`、`max_tool_calls = 24`；`Direct` 预设置 0。这些是**主循环分类额度**，数值权威在 `K06`，本卡不新设数值。
- **不进发现配额**：`is_discovery()` 不含本项。
- 处理器内另有单次载荷上限：源文件与正文均按 `MAX_EXTERNAL_TEXT_BYTES = 20 MiB` 双检（读取时多读 1 字节用于判断超限）；`create` 路径在 `create_note` 内还有一次 20 MiB 校验。这是**单次载荷上限**，不是调用次数额度。
- 派发包裹在 30 秒超时中；**不在**自动重试白名单——对真实写入的重试会直接触碰「结果未知的非幂等副作用不重复执行」这条约束。

## 幂等

- 非覆盖路径**有目标级保护**：目标已存在即失败（`create_note` 的存在性前置条件），因此「同一目标的重复导入」不会悄悄产生第二份同路径笔记，也不会覆盖已有内容。
- 覆盖路径**绑定版本**：`base_content_hash` 与目标当前全文哈希不符即 `note_content_conflict`（`apply_edit` 内 `edited_content`）；原文范围不匹配即 `note_original_conflict`。这是「重试不得造成重复变更」在覆盖语义下的实现形态。
- 需要如实说明的边界：**保护是针对目标路径与版本的，不是跨路径去重**。同一份源文件导入到两个不同目标路径是两次不同的、都会成功的写入；本工具没有内容级去重，也不回答「这份资料是否已经在 vault 里」。
- 回执未知时（超时、进程中断）不得凭重试假定「没写成」：应先核实目标状态，再决定是否重试；这条约束来自 `C06` 与架构 §8.6 的恢复验收案例。

## 取消

- 派发前执行 `ctx.ensure_run_active()`（统一入口 `dispatch_tool_inner`）；`ensure_note_write_allowed` 内部**再次**检查运行活跃性，因此写入路径有两次取消检查，且第二次发生在实际落盘之前。
- 处理器为同步执行，无 `await` 检查点：取消阻止后续调用与本次写入的启动，**不中断**已经进入写入临界区的操作。
- 不留部分结果：`apply_edit`／`create_note` 都在同一把 vault move 锁内完成，异常路径不写正文（快照与写入是两个动作，快照成功而写入失败时快照仍在，属实现事实、本卡不声明其已被视为部分结果处理）。

## 失败反馈

- 字段级：`missing source_path`／`missing authorized_root`／`missing target_path`／`missing base_content_hash`；路径问题各自返回不同文本（见参数表）。这些属可恢复或不可恢复两类，需按原因区分：路径越界属**不可通过重试同一参数恢复**。
- 目标形状：`invalid_note_import_target`（非用户笔记路径或非 `.md`）。
- 文档策略／作用域拒绝：`ensure_note_write_allowed` 链上的运行错误码（运行不活跃、写入目标与确认不一致、文档策略拒绝 `ApplyChange`）。
- 版本冲突：`note_content_conflict`、`note_original_conflict` —— 明确且可恢复，模型应重新读取目标当前版本后重新确认，而不是原样重试。
- 大小与编码：`content exceeds 20MB limit`；非法 UTF-8 由 `read_to_string` 报错，**没有**「按替换字符读入」的降级路径（对照 [paths.rs](../../src-tauri/src/storage/paths.rs) 中的 lossy 读取，本工具不使用它）。
- 快照：`note_recovery_snapshot_missing`／`note_recovery_snapshot_invalid`。
- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`。
- 用户侧：单次失败不得升级为「模型能力降级」（`N17`）；每次失败的分类与可恢复性归 `C14`／`C26`／`C27`（`K17`）。

## 暴露规则

- 目录状态 `Dispatchable`；`default_enabled_without_skill = false`。
- 需要 Run 冻结能力含 `external_fs.import`。
- **需要确认的工具在非 Durable Run 的工具面上不出现**（`only_auto = true` 过滤）；Durable 路径保留在工具面上，确认仍在执行前发生。因此「工具可见」与「写入已获准」是两个不同事实。
- `capability_affinity` 由 `access_level = WriteMarkdown` 推导为 `[WriteNotes, PatchDocument]`（并按工具名前缀规则无附加项）。
- `ContextMode::ExplicitReferences` 且检索范围不受限时，`constrain_for_run_context` 的显式允许清单**不含本项**，该场景下不可见。
- 子任务：不在 `CHILD_SAFE_TOOLS` 中，子运行不可见。
- 与导入族的关系：`fs_pick_file`／`fs_pick_folder` 仍是 **Planned 占位**（未实现，不暴露）；本工具不替代它们，也不因为文档里存在这些名字而声明可用。
- 无本项自身的 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)（`dispatchable`、`Access::WriteMarkdown`、`requires_confirmation = true`、`max_results = None`；描述写明「覆盖时必须提供已读正文的 `base_content_hash`，并保留恢复版本」）。
- 能力分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`external_fs.import`）。
- 派发路由：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`"fs_import_to_vault" => boundary_impl::fs_import_to_vault_tool(state, ctx, args)`）。
- 处理器与外部路径解析：[tool_dispatch/boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs)（`fs_import_to_vault_tool`、`resolve_external_input`、`canonical_authorized_root`、`is_sensitive_system_path`、`MAX_EXTERNAL_TEXT_BYTES`）。
- 写入原语与回执：[storage/note_operations.rs](../../src-tauri/src/storage/note_operations.rs)（`apply_edit`、`edited_content`、`create_note`、`protect_snapshot`、`NoteEditReceipt`）、[storage/note_write.rs](../../src-tauri/src/storage/note_write.rs)（`FileWriteResult`、`index_status`）、[storage/atomic_write.rs](../../src-tauri/src/storage/atomic_write.rs)（`with_vault_move_lock`）。
- 卡内授权判定：[tool_dispatch/context.rs](../../src-tauri/src/ai_runtime/tool_dispatch/context.rs)（`ensure_note_write_allowed`、`ensure_write_target_matches`）。
- 权限原子与作用域：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)。
- 派发前判定：[tool_execution_pipeline.rs](../../src-tauri/src/ai_runtime/tool_execution_pipeline.rs)、[permission_decision.rs](../../src-tauri/src/ai_runtime/permission_decision.rs)。
- 工具面过滤：[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs)、[normal_run_service.rs](../../src-tauri/src/ai_runtime/normal_run_service.rs)、[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)（确认路径 `request_change_confirmation`、`CONFIRMATION_PENDING_ERROR`）。
- **无本工具的独立处理器测试**：[tool_dispatch/tests.rs](../../src-tauri/src/ai_runtime/tool_dispatch/tests.rs) 不覆盖边界组；写入原语的测试在 [storage/note_write_tests.rs](../../src-tauri/src/storage/note_write_tests.rs) 与 `note_operations` 测试模块内。

## 当前状态

- 文档成熟度 `draft`。
- `implementation.state = present`（静态源码事实：`Dispatchable`、真实处理器、外部路径校验、版本校验与恢复快照路径在位）。**不表示合同正确，也不表示用户任务可用。**
- `verification.state = none`；本卡不声明「已修复」「已验证通过」。
- 不确定处（**待核对**）：① `authorized_root` 不进入权限作用域摘要，目录级授权是否在别处持久化未核对到；② 非覆盖路径忽略传入的 `base_content_hash`、额外字段静默忽略，与 `K10` 的参数处置要求不一致（沿用 `G01`）；③ 覆盖路径对目标 frontmatter 的处理（整篇替换）是否符合产品预期未在证据中确认；④ 源文件非 UTF-8 时的失败文本与用户可见表达未逐条核对；⑤ `indexStatus = Degraded` 是否已被下游交付路径（`C25`）区分呈现未核对。

## 相关合同

- `K05` 授权范围与冻结确认：本项消费该合同；覆盖路径的版本复验是「确认绑定版本」在工具层的实现形态。本卡只引用。
- `K10` 工具面版本与派发观察：工具面组成、版本与参数处置要求的唯一权威。
- `K06` 统一预算账本：`ConfirmedChange` 类别额度与累计的唯一权威。
- `K14` 证据身份、来源与支持关系：导入来源与被导入目标的范围表达。
- `K17` 审计事件与诊断查询：写入回执与失败分类的可解释性依据。
- 相邻工具：`T23` `fs_read_authorized_folder`（同一外部目录族）、`T22` `fs_export` / `T24` `fs_write_authorized_export`（导出族）、`T06` `read_note`（提供覆盖所需的整篇哈希）。
- 需求依据：`N03`（按目标格式生成内容并由用户插入——导入属用户显式动作的一条路径）、`N12`（严格场景保留限制、授权覆盖写入）、`N18`（可定位的失败事实）。

<!-- iris:end T21 -->
