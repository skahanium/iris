# fs_write_authorized_export

<!-- iris:object T24 kind=rules file=true -->

`fs_write_authorized_export` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T24 -->

<!-- iris:object T24 kind=tool name=fs_write_authorized_export owner=M02 -->

## 用途与语义归属

把内容写入用户授权的导出目录。语义归属为工具（`M2` / 导出服务），责任模块 `M02`；目录归属 `boundary` 组（[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)），暴露面 `extended`。

架构定义给本项的目标合同与取舍原文是「**保留兼容接口，与 fs_export 共用执行原语**」（[架构定义](../../docs/agent-architecture.md) §6.3）。两句取舍分别落到：

- **保留兼容接口**：本项与 `T22` `fs_export` 是导出族的**两个并存入口**，不合并、不删除；`C06` 的组件合同同样写明「`fs_write_authorized_export` 保留兼容接口并与 `fs_export` 共用执行原语」（见 [m02-permission-and-side-effects.md](../modules/m02-permission-and-side-effects.md)）。
- **共用执行原语**：两个处理器在源码中调用**同一个** `write_text_atomic` 与同一套外部路径解析（`resolve_external_output`），并共用 `external_fs.export` 能力标识。本卡只写本入口自身的合同，不复制 `T22` 的正文。

命名差异是本项区别于 `T22` 的唯一入口级差异：**写入目标参数名为 `target_path`**（`T22` 用 `dest_path`），措辞强调「写入用户授权的导出目录」。

## 参数与消费

| 参数                       | 类型    | 声明边界     | 处理器行为                                                                                                                                                                                                                                                                                                 |
| -------------------------- | ------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `authorized_root`          | string  | **required** | 必填；解析前 `create_dir_all(root)`，随后必须存在且是目录（`authorized_root must exist`／`must be a directory`），系统敏感前缀被拒                                                                                                                                                                         |
| `target_path`              | string  | **required** | 必填；相对路径按 `authorized_root` 解析，绝对路径按原样使用；含 `..` → `Path traversal is not allowed`；命中系统敏感前缀 → `不允许导出到系统目录`；规范化后的父目录必须仍在 `authorized_root` 内，否则 `dest_path is outside authorized_root`（**错误文本沿用 `dest_path` 字样**，未按本入口的参数名改写） |
| `content`                  | string  | **required** | 必填；`content.len() > 20 MiB` → `content exceeds 20MB limit`                                                                                                                                                                                                                                              |
| `overwrite`                | boolean | 默认 false   | 非布尔或缺失按 false；`false` 且目标已存在 → `Target already exists`                                                                                                                                                                                                                                       |
| 额外字段（如 `dest_path`） | —       | —            | **被静默忽略**：本入口不读 `dest_path`，误用 `T22` 的参数名会得到 `missing target_path`                                                                                                                                                                                                                    |

参数与消费总体一致（`missing target_path` 属字段级反馈，方向正确），但有两点必须写明：① 越界错误文本使用 `dest_path`，与本入口的参数名不一致（**待核对**：属实现遗留还是刻意统一文案）；② schema 无 `additionalProperties: false`，额外字段被静默忽略，与 `K10` 的「所有暴露参数必须被消费或明确拒绝」存在口径差，沿用 `G01` 的证据要求。

写入方式与 `T22` 相同：同目录临时文件 + `rename` 原子替换，目标父目录按需创建。返回：`type`（`"fs_write_authorized_export"`）、`destPath`（规范化后的绝对路径文本）、`bytes`（正文字节数）——**返回字段名与 `T22` 完全一致**，两个入口在载荷层不可区分，只能靠 `type` 判别。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["external_fs.export"]`，与 `T22` **共用同一能力标识**。
- 权限原子（`C04`）：`Atom::FsWriteAuthorizedExport`，**`Risk::High`，`supported = true`**——注意原子与 `T22` 的 `Atom::FsExport` **不同**，即审计与授权记录中两个入口可区分，尽管能力标识相同。
- 确认语义：`requires_confirmation = true` → `preflight` 为 `AllowOnce` → 判定结果为 `RequiresConfirmation`，**必须经 `C05` 冻结确认后才提交写入**；高风险不允许写入会话级授权。
- 工具面：非 Durable Run 以 `only_auto = true` 构建，**本项不在该工具面上**；Durable 路径可见但执行前仍须确认。
- **作用域摘要**：取值集合为 `target_path`／`path`／`note_path`，本入口的参数名正好是 `target_path`，因此授权摘要**携带实际目标路径**（对比 `T22` 的 `dest_path` 回落到「current request」）。这是两个入口在授权呈现上的实际差别，本卡只陈述事实。
- **处理器侧的授权判定范围（如实记录）**：处理器签名是 `fs_write_authorized_export_tool(args)`，只接收参数，**没有 `ctx`**；卡内不做文档策略、检索范围或写入目标一致性复检，授权判定发生在派发边界与确认流程。
- `authorized_root` 本身不进入作用域摘要；目录级授权由 `C04` 的确认／授予承担，处理器只做存在性、目录性与系统敏感前缀检查——**不比对授权目录清单**。
- 子任务：**不在** `CHILD_SAFE_TOOLS` 白名单中；子运行不得产生外部写入。

## 副作用

**有真实副作用：在 vault 之外创建或覆盖一个文件。**

- 目标父目录会被 `create_dir_all` 创建，即使写入最终失败也可能留下空目录。
- 写入为原子替换：写 `<目标名>.tmp`，`rename` 到目标；`rename` 失败时删除临时文件并返回错误。
- **不触碰 vault 内容**：不写笔记、不建版本快照、不改索引；导出族在 `agent_permissions.rs` 的 `reversible_by` 里对应「permission settings」，即不可由笔记版本历史撤销。
- `overwrite = true` 且目标存在时是**不可逆替换**：无备份、无恢复版本。这与同族导入路径（`T21` 覆盖时保护恢复快照）形成对照。
- `content` 由模型提供，其来源与正确性不由本工具校验；本工具不回答「导出的内容是否就是用户要的那份」。内容保持与来源约束在 `C23`／`K14` 的边界内表达。

## 预算

- `budget_class()` 归入 `ToolBudgetClass::ConfirmedChange`（无 `execution_metadata`；`requires_confirmation = true` 优先于 `access_level` 分支）。
- 与 `T21`、`T22` 及写侧工具共用 `ConfirmedChange` 类别：当前生产预设（Standard／Delegated／DurableApply）`max_confirmed_change_calls = 6`、`max_tool_calls = 24`；`Direct` 置 0。数值权威在 `K06`。
- **两个入口共用同一类别、同一能力标识、同一执行原语**：因此同一次导出任务走 `T22` 还是 `T24`，对分类额度的消耗方式相同；把两者视为「额外的免费额度」是不成立的。
- **不进发现配额**：`is_discovery()` 不含本项。
- 单次载荷上限 20 MiB（`MAX_EXTERNAL_TEXT_BYTES`）；这是体量上限，不是次数额度。
- 派发包裹在 30 秒超时中；**不在**自动重试白名单——重试即一次新的外部写入，触碰「结果未知的非幂等副作用不重复执行」。

## 幂等

- `overwrite = false`：目标已存在即 `Target already exists`，因此同一目标的重复写入不会静默替换既有文件。
- `overwrite = true`：**无版本校验参数**，属无条件整篇替换（与 `T21` 覆盖路径要求 `base_content_hash` 不同）；同一路径两次成功调用后，前一次内容不再存在。
- 目标父目录创建是前置动作，重复调用不因目录已存在而失败。
- 路径安全基于 `parent().canonicalize()` 后的前缀比较；符号链接导致的实际落点差异未逐项核对（**待核对**）。
- 回执未知时（超时、进程中断）不得凭重试假定「没写成」：先核实目标状态，再决定是否重试（`C06`、架构 §8.6）。

## 取消

- 派发前执行 `ctx.ensure_run_active()`（统一入口 `dispatch_tool_inner`），已取消的 Run 不进入处理器。
- 处理器为同步执行，无 `await` 检查点，也无卡内 `ctx` 复检：取消阻止后续调用，**不中断**已开始的写入。
- 不留部分结果：临时文件 + 原子替换保证失败不落目标文件；但**父目录创建可能已经发生**。

## 失败反馈

- 字段级：`missing target_path`／`missing authorized_root`／`missing content`。误用 `dest_path` 会得到 `missing target_path`——这是本入口与 `T22` 参数名差异的直接后果。
- 路径问题（**不可通过重试同一参数恢复**）：`Path traversal is not allowed`、`不允许导出到系统目录`、`dest_path is outside authorized_root`（文本沿用 `dest_path` 字样）、`authorized_root must exist`、`authorized_root must be a directory`、`Invalid output path`。
- 覆盖冲突：`Target already exists`（可恢复，但提高强度到 `overwrite = true` 本身可能需要新的确认）。
- 体量：`content exceeds 20MB limit`。
- 系统调用失败：以 `AppError` 文本透传。
- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`。
- 越权与拒绝：能力不在冻结合同内即派发前拒绝；`deny` 不因重试或换参数转为 `allow`（`K05`）。
- 用户侧：单次失败不得升级为「模型能力降级」（`N17`）；恢复与归因轨迹归 `C14`／`C26`／`C27`（`K17`）。

## 暴露规则

- 目录状态 `Dispatchable`；`default_enabled_without_skill = false`。
- 需要 Run 冻结能力含 `external_fs.export`（与 `T22` 同）。
- 非 Durable Run 的工具面用 `only_auto = true` 构建，**本项（需确认）不在其中**；Durable 路径可见但执行前仍须确认。
- `capability_affinity` 由 `access_level = WriteMarkdown` 推导为 `[WriteNotes, PatchDocument]`。
- `ContextMode::ExplicitReferences` 且检索范围不受限时，`constrain_for_run_context` 的显式允许清单**不含本项**。
- 子任务：不在 `CHILD_SAFE_TOOLS` 中。
- **与 `T22` 的暴露关系**：两者是同一能力下的两个可选入口。本卡不规定「哪个应优先进入工具面」——该选择属装配与迁移决定；本卡只记录「保留兼容接口、共用执行原语」这一架构取舍，以及二者在参数名、权限原子、作用域摘要上的实际差异。
- 无本项自身的 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)（`dispatchable`、`Access::WriteMarkdown`、`requires_confirmation = true`、`max_results = None`、`required: ["authorized_root", "target_path", "content"]`）。
- 能力分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`"fs_export" | "fs_write_authorized_export" => &["external_fs.export"]`）。
- 派发路由：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`"fs_write_authorized_export" => boundary_impl::fs_write_authorized_export_tool(args)`，该名称同时在 `DISPATCHABLE_TOOL_NAMES` 中）。
- 处理器与共用原语：[tool_dispatch/boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs)（`fs_write_authorized_export_tool` 与 `fs_export_tool` 都调用 `resolve_external_output` 与 `write_text_atomic`）。
- 权限原子：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`Atom::FsWriteAuthorizedExport`、`Risk::High`）、作用域摘要取值（`scope_summary`）。
- 派发前判定：[tool_execution_pipeline.rs](../../src-tauri/src/ai_runtime/tool_execution_pipeline.rs)、[permission_decision.rs](../../src-tauri/src/ai_runtime/permission_decision.rs)。
- 工具面过滤：[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs)、[normal_run_service.rs](../../src-tauri/src/ai_runtime/normal_run_service.rs)、[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)。
- 组件的兼容声明：[m02-permission-and-side-effects.md](../modules/m02-permission-and-side-effects.md)（`C06` 兼容节）。
- **无本工具的独立处理器测试**：[tool_dispatch/tests.rs](../../src-tauri/src/ai_runtime/tool_dispatch/tests.rs) 不覆盖边界组；也没有针对「`T22`／`T24` 载荷不可区分」这一事实的对照测试。

## 当前状态

- 文档成熟度 `draft`。
- `implementation.state = present`（静态源码事实：`Dispatchable`、真实处理器、与 `T22` 共用的路径解析与原子写入原语在位）。**不表示合同正确，也不表示用户任务可用。**
- `verification.state = none`；本卡不声明「已修复」「已验证通过」。
- 不确定处（**待核对**）：① 越界错误文本用 `dest_path` 与本入口参数名不一致；② 两个入口的载荷除 `type` 外完全相同，消费者区分入口的唯一依据是 `type` 字段，该口径是否被下游（`C25` 交付投影）使用未核对；③ `overwrite = true` 无版本校验（与 `T21` 的覆盖差异）；④ 符号链接绕过前缀判断的可能性未逐项核对；⑤ 工具面装配时两个入口的取舍规则（保留兼容期间的默认可见性）未在源码中核对到明确策略。

## 相关合同

- `K05` 授权范围与冻结确认：本项消费该合同；导出属需确认的外部副作用，本卡只引用。
- `K10` 工具面版本与派发观察：工具面组成、版本与参数处置要求的唯一权威。
- `K06` 统一预算账本：`ConfirmedChange` 类别额度与累计的唯一权威。
- `K14` 证据身份、来源与支持关系：导出内容的来源表达不由本工具承担。
- `K17` 审计事件与诊断查询：写入失败分类与入口身份的可解释性依据。
- 相邻工具：`T22` `fs_export`（同一执行原语的另一入口）、`T21` `fs_import_to_vault`（反向动作，带版本校验）、`T23` `fs_read_authorized_folder`（同族外部目录读取）。
- 需求依据：`N12`（严格场景保留限制、授权覆盖写入）、`N18`（可定位的失败事实）。

<!-- iris:end T24 -->
