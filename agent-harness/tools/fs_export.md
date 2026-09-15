# fs_export

<!-- iris:object T22 kind=rules file=true -->

`fs_export` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T22 -->

<!-- iris:object T22 kind=tool name=fs_export owner=M02 -->

## 用途与语义归属

把内容导出到用户确认的外部目标路径。语义归属为工具（`M2` / 导出服务），责任模块 `M02`；目录归属 `boundary` 组（[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)），暴露面 `extended`。

架构定义给本项的目标合同与取舍原文是「**保留显式导出接口，沿统一变更边界**」（[架构定义](../../docs/agent-architecture.md) §6.3）。两句取舍分别落到：

- **保留显式导出接口**：本次架构整理不动导入／导出／笔记管理这些既有产品功能；导出仍然是一个显式动作，而不是被折叠进某个通用写入工具。
- **沿统一变更边界**：Agent 引起的副作用只有 `C06` 这一条提交边界，导出也属于副作用（`C06` 的组件合同明确写「记忆、计划登记和导出等副作用由本组件授权执行，具体存储逻辑仍由对应业务组件完成」，见 [m02-permission-and-side-effects.md](../modules/m02-permission-and-side-effects.md)）；本工具是导出族中执行原语的一个入口，与 `T24` 共用同一实现。

本卡只引用上述措辞，不重复其他对象的正文。

## 参数与消费

| 参数                         | 类型    | 声明边界     | 处理器行为                                                                                                                                                                                                                                          |
| ---------------------------- | ------- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `dest_path`                  | string  | **required** | 必填；相对路径按 `authorized_root` 解析，绝对路径按原样使用；含 `..` → `Path traversal is not allowed`；命中系统敏感前缀 → `不允许导出到系统目录`；解析后的父目录规范化后必须仍在 `authorized_root` 内，否则 `dest_path is outside authorized_root` |
| `authorized_root`            | string  | **required** | 必填；解析前会 `create_dir_all(root)`，随后必须存在且是目录（`authorized_root must exist`／`must be a directory`），系统敏感前缀被拒                                                                                                                |
| `content`                    | string  | **required** | 必填；写入前检查 `content.len() > 20 MiB` → `content exceeds 20MB limit`                                                                                                                                                                            |
| `overwrite`                  | boolean | 默认 false   | 非布尔或缺失按 false；`false` 且目标已存在 → `Target already exists`                                                                                                                                                                                |
| 额外字段（如 `target_path`） | —       | —            | **被静默忽略**；`T24` 用的是 `target_path`，两者参数名不同（见「相关合同」）                                                                                                                                                                        |

参数与消费一致（每个声明参数都有真实行为），但 schema 无 `additionalProperties: false`，额外字段被静默忽略；这与 `K10` 的「所有暴露参数必须被消费或明确拒绝」存在口径差，沿用 `G01` 的证据要求（工具卡逐项实现状态与目录分类的一致性核对）。

写入方式：同目录临时文件 + `rename` 原子替换（`write_text_atomic`）；目标父目录会按需创建。返回：`type`（`"fs_export"`）、`destPath`（规范化后的绝对路径文本）、`bytes`（正文字节数）。

**本工具不读回显**：返回载荷只有目标路径与字节数，**没有**内容哈希、版本号或索引状态——外部导出目录不属于 vault 索引范围，因此也不存在「派生索引是否同步」这一维度。需要版本事实时应另建机制，本卡不宣称它已存在。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["external_fs.export"]`，与 `T24` 共用一个能力标识（对应「与 `fs_export` 共用执行原语」的取舍）。
- 权限原子（`C04`）：`Atom::FsExport`，**`Risk::High`，`supported = true`**；高风险不允许写入会话级授权。
- 确认语义：`requires_confirmation = true` → `preflight` 为 `AllowOnce` → 判定结果为 `RequiresConfirmation`，**必须经 `C05` 冻结确认后才提交写入**。
- 工具面：非 Durable Run 用 `only_auto = true` 过滤，**本项在这类 Run 的工具面上不出现**；Durable 路径可提议，但提议之后仍须确认。
- **处理器侧的授权判定范围（如实记录）**：处理器签名是 `fs_export_tool(args)`，只接收参数，**没有 `ctx`**。因此它不像 `T21` 那样在卡内调用 `ensure_note_write_allowed`——文档策略、检索范围、写入目标一致性、活跃技能作用域这些判定**不在本工具内**；进入派发前的能力判定与确认仍在，但导出**不经过 vault 写入族的卡内复检**。本卡只陈述这一结构事实，并把「导出是否应纳入同族卡内复检」留作待核对项。
- 权限作用域摘要取 `target_path`／`path`／`note_path` 之一，而本项的参数名是 `dest_path`：**三者都不匹配，摘要回落到「current request」**。因此授权摘要不携带实际目标路径，用户确认时看到的目标范围与真实写入目标的对应关系属**待核对**项。
- `authorized_root` 本身不进入作用域摘要；「这个目录是否被授权」由用户确认动作承担。
- 子任务：**不在** `CHILD_SAFE_TOOLS` 白名单中；子运行不得产生外部写入。

## 副作用

**有真实副作用：在 vault 之外创建或覆盖一个文件。**

- 目标父目录会被创建（`create_dir_all`），即使最终写入失败也可能留下空目录。
- 写入为原子替换：先写 `<目标名>.tmp` 再 `rename`；`rename` 失败时清理临时文件并返回错误。
- 导出**不触碰 vault 内容**：不写笔记、不建版本快照、不改索引。因此它不属于「可用版本历史回滚」的那一类动作——`reversible_by` 对导出族给出的是「permission settings」（见 `agent_permissions.rs`），即不可由笔记版本历史撤销。
- `content` 由模型提供，其来源与正确性不由本工具校验：本工具不回答「导出的内容是否就是用户要的那份」；内容保持与来源约束分别在 `C23`／`K14` 的边界内表达。
- 覆盖既有外部文件会**不可逆地替换**其内容（`overwrite = true` 且目标存在时）；没有备份、没有恢复版本，这与 vault 内写入（`T21` 会保护恢复快照）形成明显差别，使用者需按此判断风险。

## 预算

- `budget_class()` 归入 `ToolBudgetClass::ConfirmedChange`（无 `execution_metadata`；`requires_confirmation = true` 优先于 `access_level` 的分支）。
- 与 `T21`、`T24` 及写侧工具共用 `ConfirmedChange` 类别：当前生产预设（Standard／Delegated／DurableApply）`max_confirmed_change_calls = 6`、`max_tool_calls = 24`；`Direct` 置 0。数值权威在 `K06`。
- **不进发现配额**：`is_discovery()` 不含本项。
- 单次载荷上限 20 MiB（`MAX_EXTERNAL_TEXT_BYTES`）；这是体量上限，不是调用次数额度。
- 派发包裹在 30 秒超时中；**不在**自动重试白名单——重试是一次新的外部写入，直接触碰「结果未知的非幂等副作用不重复执行」这条约束。

## 幂等

- `overwrite = false` 时**不覆盖已存在目标**（`Target already exists`），因此「同一目标的重复导出」不会静默替换既有文件：第一次成功、第二次失败。
- `overwrite = true` 时**没有版本校验参数**：与 `T21` 的覆盖路径（要求 `base_content_hash` 并复核版本）不同，本工具的覆盖是**无条件整篇替换**——同一路径两次调用，后一次成功即前一次内容不再存在。这是本卡必须写明的取舍差异，而不是可忽略的实现细节。
- 目标路径解析包含规范化后的父目录越界检查，但**不阻止符号链接导致的实际落点差异**：判断基于 `parent().canonicalize()` 后的前缀比较，父目录内部再深一层的情况未逐项核对（**待核对**）。
- 回执未知时（超时、进程中断）不得凭重试假定「没写成」：应先核实目标状态再决定是否重试（`C06`、架构 §8.6）。

## 取消

- 派发前执行 `ctx.ensure_run_active()`（统一入口 `dispatch_tool_inner`），已取消的 Run 不进入处理器。
- 处理器为同步执行，无 `await` 检查点，也**没有**基于 `ctx` 的卡内复检：取消阻止后续调用，不中断已开始的写入。
- 不留部分结果：写入是「临时文件 + 原子替换」，失败即不落目标文件；但**父目录创建可能已经发生**（见「副作用」）。

## 失败反馈

- 字段级：`missing dest_path`／`missing authorized_root`／`missing content`。
- 路径问题（**不可通过重试同一参数恢复**）：`Path traversal is not allowed`、`不允许导出到系统目录`、`dest_path is outside authorized_root`、`authorized_root must exist`、`authorized_root must be a directory`、`Invalid output path`。
- 覆盖冲突：`Target already exists`（可恢复：由用户或模型决定是否显式 `overwrite = true`，而提高强度本身可能需要新的确认）。
- 体量：`content exceeds 20MB limit`。
- 系统调用失败：以 `AppError` 文本透传（IO 错误）。
- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`。
- 越权与拒绝：能力不在冻结合同内即派发前拒绝；`deny` 不因重试或换参数转为 `allow`（`K05`）。
- 用户侧：单次失败不得升级为「模型能力降级」（`N17`）；恢复与归因轨迹归 `C14`／`C26`／`C27`（`K17`）。

## 暴露规则

- 目录状态 `Dispatchable`；`default_enabled_without_skill = false`。
- 需要 Run 冻结能力含 `external_fs.export`。
- 非 Durable Run 的工具面用 `only_auto = true` 构建，**过滤掉需要确认的工具，本项不在其中**；Durable 路径可见但执行前仍须确认。
- `capability_affinity` 由 `access_level = WriteMarkdown` 推导为 `[WriteNotes, PatchDocument]`。
- `ContextMode::ExplicitReferences` 且检索范围不受限时，`constrain_for_run_context` 的显式允许清单**不含本项**。
- 子任务：不在 `CHILD_SAFE_TOOLS` 中。
- 与 `T24` 的暴露关系：两者共用能力标识与执行原语，但**参数名不同**（`dest_path` 与 `target_path`），且都是独立可暴露的目录项；架构 §6.3 把 `T24` 记为「保留兼容接口」，本卡不替它定义正文。
- 无本项自身的 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)（`dispatchable`、`Access::WriteMarkdown`、`requires_confirmation = true`、`max_results = None`）。
- 能力分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`"fs_export" | "fs_write_authorized_export" => &["external_fs.export"]`）。
- 派发路由：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`"fs_export" => boundary_impl::fs_export_tool(args)`）。
- 处理器与写入原语：[tool_dispatch/boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs)（`fs_export_tool`、`resolve_external_output`、`canonical_authorized_root`、`is_sensitive_system_path`、`write_text_atomic`、`MAX_EXTERNAL_TEXT_BYTES`）。
- 权限原子与 `reversible_by`：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)。
- 作用域摘要取值：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`scope_summary`：`target_path`／`path`／`note_path`）。
- 派发前判定：[tool_execution_pipeline.rs](../../src-tauri/src/ai_runtime/tool_execution_pipeline.rs)、[permission_decision.rs](../../src-tauri/src/ai_runtime/permission_decision.rs)。
- 工具面过滤：[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs)（`tools_for_authorized_capabilities(..., only_auto)` 的过滤测试明确断言 `note.apply_patch` 面不含 `fs_export`）、[normal_run_service.rs](../../src-tauri/src/ai_runtime/normal_run_service.rs)、[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)。
- 组件的统一变更边界：[m02-permission-and-side-effects.md](../modules/m02-permission-and-side-effects.md)（`C06`，含「`fs_write_authorized_export` 保留兼容接口并与 `fs_export` 共用执行原语」）。
- **无本工具的独立处理器测试**：[tool_dispatch/tests.rs](../../src-tauri/src/ai_runtime/tool_dispatch/tests.rs) 不覆盖边界组；[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs) 的测试覆盖的是「工具面是否暴露」而不是写入行为。

## 当前状态

- 文档成熟度 `draft`。
- `implementation.state = present`（静态源码事实：`Dispatchable`、真实处理器与原子写入原语在位）。**不表示合同正确，也不表示用户任务可用。**
- `verification.state = none`；本卡不声明「已修复」「已验证通过」。
- 不确定处（**待核对**）：① 处理器不接收 `ctx`，导出未经过 vault 写入族的卡内策略／作用域复检——「导出是否应纳入同族复检」需由 `M02`／`M06` 核对，本卡不判定缺口归属；② 授权作用域摘要回落为「current request」（参数名 `dest_path` 不在摘要取值集合内），确认界面呈现的目标范围与真实目标的对应关系未核对；③ `overwrite = true` 无版本校验，属能力差异还是待修正项未在证据中确认；④ 符号链接绕过路径前缀判断的可能性未逐项核对；⑤ 父目录创建发生在写入失败前的副作用未核对是否已在交付表达中体现。

## 相关合同

- `K05` 授权范围与冻结确认：本项消费该合同；导出属需确认的外部副作用，本卡只引用。
- `K10` 工具面版本与派发观察：工具面组成、版本与参数处置要求的唯一权威。
- `K06` 统一预算账本：`ConfirmedChange` 类别额度与累计的唯一权威。
- `K14` 证据身份、来源与支持关系：导出内容的来源表达不由本工具承担。
- `K17` 审计事件与诊断查询：写入失败分类的可解释性依据。
- 相邻工具：`T24` `fs_write_authorized_export`（共用原语的兼容接口）、`T21` `fs_import_to_vault`（反向动作，带版本校验与恢复快照）、`T23` `fs_read_authorized_folder`（同族外部目录读取）。
- 需求依据：`N12`（严格场景保留限制、授权覆盖写入）、`N18`（可定位的失败事实）。

<!-- iris:end T22 -->
