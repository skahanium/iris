# fs_read_authorized_folder

<!-- iris:object T23 kind=rules file=true -->

`fs_read_authorized_folder` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T23 -->

<!-- iris:object T23 kind=tool name=fs_read_authorized_folder owner=M06 -->

## 用途与语义归属

读取**明确授权的外部目录**的**单层条目摘要**（名称、类型、文件字节数）。语义归属为工具（`M6` / 文件服务），责任模块 `M06`；目录归属 `boundary` 组（[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)），暴露面 `extended`。

架构定义给本项的目标合同与取舍原文是「**读取明确授权的外部目录**」（[架构定义](../../docs/agent-architecture.md) §6.3）。要点是「明确授权」：`authorized_root` 是必填参数，本工具不接受默认目录、不接受通配、不遍历 vault 之外的其他位置。

目录描述自带的措辞是「读取已由用户授权的外部目录**摘要**」——**摘要**二字是能力边界：本工具**不返回文件正文**，只返回目录项的元数据。要读外部文件正文需要另经导入路径（`T21`），本工具不提供正文读取参数。

本卡只引用上述措辞，不重复其他对象的正文。

## 参数与消费

| 参数                               | 类型    | 声明边界     | 处理器行为                                                                                                                                                                   |
| ---------------------------------- | ------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `authorized_root`                  | string  | **required** | 必填；`canonical_authorized_root` 要求路径**已存在且是目录**（`authorized_root must exist`／`authorized_root must be a directory`）；命中系统敏感前缀 → `不允许访问系统目录` |
| `max_entries`                      | integer | 默认 100     | 非 `u64` 或缺失按 100；**再 `clamp(1, 500)`**，声明未写出上下限                                                                                                              |
| 额外字段（如 `path`、`recursive`） | —       | —            | **被静默忽略、不报错**（schema 无 `additionalProperties: false`）                                                                                                            |

额外字段静默忽略与 `K10` 的「所有暴露参数必须被消费或明确拒绝」存在口径差，沿用 `G01` 的证据要求（工具卡逐项实现状态与目录分类的一致性核对）。

返回：`type`（`"fs_read_authorized_folder"`）、`root`（规范化后的绝对路径文本）、`entries`（数组，每项为 `{name, kind: "directory"|"file", bytes}`）、`count`（返回条目数，等于 `entries.len()`）。

三个必须如实写明的边界：

1. **单层**：使用 `std::fs::read_dir`，**不递归**；子目录只给出名称与 `kind = "directory"`，其内容不在本次返回中。
2. **静默截断**：`read_dir(...).take(max_entries)` 在遍历上限处停止，载荷**没有** `truncated` 字段或总条目数——`count = max_entries` 时消费者**无法区分**「目录正好这么多项」与「被上限截断」。这与 `read_note` 的显式 `truncated`／`nextStartByte` 形成对照（`T06`）。
3. **顺序未定义**：`read_dir` 的返回顺序不保证稳定，因此两次调用的 `entries` 顺序可以不同。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["external_fs.read"]`——与 `fs_pick_file`／`fs_pick_folder`（均为 Planned）共用同一能力标识。
- 权限原子（`C04`）：`Atom::FsReadAuthorizedFolder`，**`Risk::High`，`supported = true`**；作用域类别为 `PermissionScopeKind::Folder`。
- 确认语义：目录项 `requires_confirmation = true` → `preflight` 给出 `AllowOnce` → 判定结果为 `RequiresConfirmation`。**即使只读，本项仍落在确认路径上**：这是目录级外部访问与 vault 内读取（`read_note`，`requires_confirmation = false`）的显式差别。
- 高风险不允许写入会话级授权（`upsert_permission_grant` 对 `AllowForSession` + High／Critical 返回 `agent_permission_session_grants_risk_denied`）；「已有有效授权不反复索要」这条不变量在本次会话内的实际空间受该限制约束。
- 工具面：非 Durable Run 以 `only_auto = true` 构建，**过滤掉需要确认的工具，本项不在其中**；Durable 路径可见，执行前仍须确认。
- **处理器侧的授权判定范围（如实记录）**：处理器签名是 `fs_read_authorized_folder_tool(args)`，只接收参数，**没有 `ctx`**。因此卡内不做文档策略、检索范围、技能作用域的判定；`C04` 的能力与确认判定在派发边界生效。
- 权限作用域摘要取 `target_path`／`path`／`note_path` 之一，本项的参数名是 `authorized_root`：**三者都不匹配，摘要回落到「current request」**。因此授权摘要不携带实际目标目录，用户确认时看到的范围与真实读取目录的对应关系属**待核对**项；`PermissionScopeKind::Folder` 已声明了作用域形状，但摘要文本不含目录值。
- **处理器不校验 `authorized_root` 是否真的被用户授权过**：它只做存在性、目录性与系统敏感前缀检查。「明确授权」由 `C04` 的确认／授予提供，而不是由处理器比对一个授权目录清单——本卡不宣称目录许可清单已经存在。
- 子任务：**不在** `CHILD_SAFE_TOOLS` 白名单中；子运行不得读取父级未授权的目录（`Q07` 讨论的是子任务缺 `web_fetch`，与本项无关，不据此推断子任务可读外部目录）。

## 副作用

**无状态变更**：只做一次目录遍历，不写文件、不写数据库、不改索引、不改被读目录。

三类事实分开陈述：

1. **不写状态**：`read_dir` 是只读调用，`metadata()` 也只在 `entry` 上取元数据。
2. **读取面即信息面**：返回的条目名与字节数会让模型得知**vault 之外**的目录结构；其外发受 `C04` 授权与 `K05` 范围约束。本工具不返回文件正文，因此不构成「外部文件内容外发」的通道。
3. **L0 档位**：本工具的 `sandbox_profile_for_tool` 走默认分支，即 `fs_read_authorized_folder:l0_app_boundary`——应用级策略与权限边界，`limitations` 明写 _application-level controls only; not an OS sandbox_。它不是子进程档位，也不宣称 OS 级隔离。

## 预算

- **本项在目录表中有专属预算分类**：`budget_class()` 对 `fs_read_authorized_folder` 显式返回 `ToolBudgetClass::ExternalRead`（与 `doc_convert`／`doc_ocr`／`doc_extract_pdf`／`doc_extract_table` 同类），**优先于** `requires_confirmation` 推出 `ConfirmedChange` 的通用分支。`tool_catalog/tests.rs` 有对应断言。
- 当前生产预设（Standard／Delegated／DurableApply）`max_external_read_tool_calls = 6`、`max_tool_calls = 24`；`Direct` 预设置 0。这些是**主循环分类额度**，数值权威在 `K06`，本卡不新设数值。
- **不进发现配额**：`is_discovery()` 不含本项；它读取的是外部目录，不是 vault 内的候选发现。
- 单次条目上限 500（`clamp(1, 500)`）：这是**单次载荷上限**，不是调用次数额度。
- 派发包裹在 30 秒超时中；**不在**自动重试白名单（白名单只含 `web_search`／`web_fetch` 的超时与网络类错误）。

## 幂等

读取语义幂等：重复调用不产生变更，也不会「消费」目录内容。返回值随外部目录变化而变化——文件增删、大小变化都会改变 `entries`。

需要写明的两点限制：

- **顺序不稳定**：`read_dir` 顺序不保证，因此「同一目录两次调用返回相同数组」不可作为幂等断言；需要稳定比较时应按名称排序后再比。
- **静默截断**：`count` 达到 `max_entries` 时，消费者无法区分完整与截断；把它当作「该目录只有这些条目」的证据是不成立的。载荷缺 `truncated` 字段属实现事实（见「参数与消费」节）。

本工具没有版本或哈希参数，不提供「目录内容是否自上次读取后变化」的校验。

## 取消

- 派发前执行 `ctx.ensure_run_active()`（统一入口 `dispatch_tool_inner`），已取消的 Run 不进入处理器。
- 处理器为同步执行，无 `await` 检查点，也无卡内 `ctx` 复检：取消阻止后续调用，**不中断**已开始的遍历。
- 不留部分结果：失败即整体失败（任一条目 `metadata()` 失败都会向上返回错误），成功即完整（在 `max_entries` 上限内的完整条目对象）。

## 失败反馈

- 字段级：`missing authorized_root`。
- 路径问题（**不可通过重试同一参数恢复**）：`authorized_root must exist`、`authorized_root must be a directory`、`不允许访问系统目录`。
- 遍历失败：任一条目读取失败（权限不足、链接失效等）以 `AppError` 文本透传，**没有**「跳过失败项并保留其余」的部分成功表达——这与网页抓取族「部分失败保留成功页面」的处理方式明显不同（架构 §3 M5 的可恢复错误表针对抓取族，本项不在其列）。
- 空目录：返回 `count = 0` 与空数组，**不是错误**；但「空」与「截断到 0」在当前实现下不会同时出现（下限为 1）。
- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`。
- 越权与拒绝：能力不在冻结合同内即派发前拒绝；`deny` 不因重试或换参数转为 `allow`（`K05`）。
- 用户侧：单次失败不得升级为「模型能力降级」（`N17`）；恢复与归因轨迹归 `C14`／`C26`／`C27`（`K17`）。

## 暴露规则

- 目录状态 `Dispatchable`；`default_enabled_without_skill = false`。
- 需要 Run 冻结能力含 `external_fs.read`。
- 非 Durable Run 的工具面用 `only_auto = true` 构建，**本项（需确认）不在该工具面上**；Durable 路径可见但执行前仍须确认。
- `capability_affinity` 由 `access_level = ReadIndex` 推导为 `SearchNotes`。
- `ContextMode::ExplicitReferences` 且检索范围不受限时，`constrain_for_run_context` 的显式允许清单**不含本项**：即使授权已给，@-引用场景下外部目录读取也不可见。
- 子任务：不在 `CHILD_SAFE_TOOLS` 中。
- 与同族 Planned 项的关系：`fs_pick_file`／`fs_pick_folder`（请求用户选择文件或授权目录）仍是 **Planned 占位**，未实现、不暴露；因此当前**不存在**由 agent 主动发起「请用户现在授权这个目录」的对话内入口——授权动作由产品侧设置路径提供，本卡不据此推断该入口的完整形态。
- 无本项自身的 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)（`dispatchable`、`Access::ReadIndex`、`requires_confirmation = true`、`max_results = None`；描述为「读取已由用户授权的外部目录摘要」）。
- 能力与预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`external_fs.read`；`ExternalRead` 分支显式列出本工具）。
- 预算分类断言：[tool_catalog/tests.rs](../../src-tauri/src/ai_runtime/tool_catalog/tests.rs)（`catalog_owns_one_stable_budget_class_for_every_budgeted_tool_kind`）。
- 派发路由：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`"fs_read_authorized_folder" => boundary_impl::fs_read_authorized_folder_tool(args)`）。
- 处理器：[tool_dispatch/boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs)（`fs_read_authorized_folder_tool`、`canonical_authorized_root`、`is_sensitive_system_path`、`SENSITIVE_PREFIXES`）。
- 沙箱档位：[sandbox_profile.rs](../../src-tauri/src/ai_runtime/sandbox_profile.rs)（默认 `l0_app_boundary` 分支）。
- 权限原子与作用域类别：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`Atom::FsReadAuthorizedFolder`、`PermissionScopeKind::Folder`、会话级高风险管理）。
- 派发前判定：[tool_execution_pipeline.rs](../../src-tauri/src/ai_runtime/tool_execution_pipeline.rs)、[permission_decision.rs](../../src-tauri/src/ai_runtime/permission_decision.rs)、[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`preflight_tool_permission`）。
- 工具面过滤：[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs)、[normal_run_service.rs](../../src-tauri/src/ai_runtime/normal_run_service.rs)。
- 评测侧引用（**对照，不是本卡的验证证据**）：[agent_capacity_eval/test_support.rs](../../src-tauri/src/ai_runtime/agent_capacity_eval/test_support.rs) 在夹具中把本工具与 `read_note`、`web_search` 并列；[agent_tool_loop_tests.rs](../../src-tauri/src/ai_runtime/agent_tool_loop_tests.rs) 使用只读工具规格。这些**不构成**本工具行为的验收证据。
- **无本工具的独立处理器测试**：[tool_dispatch/tests.rs](../../src-tauri/src/ai_runtime/tool_dispatch/tests.rs) 不覆盖边界组。

## 当前状态

- 文档成熟度 `draft`。
- `implementation.state = present`（静态源码事实：`Dispatchable`、真实处理器、系统目录拒绝与条目上限钳制在位）。**不表示合同正确，也不表示用户任务可用。**
- `verification.state = none`；本卡不声明「已修复」「已验证通过」。
- 不确定处（**待核对**）：① 「明确授权」目前完全依赖 `C04` 的确认／授予，处理器不比对目录许可清单；② 授权作用域摘要回落为「current request」（参数名 `authorized_root` 不在摘要取值集合内），确认界面与真实目标的对应关系未核对；③ 静默截断缺 `truncated` 字段；④ `read_dir` 顺序不稳定未在返回中声明；⑤ 条目遍历中途失败时整体失败（无部分成功表达）是否符合目标可恢复性合同未确认；⑥ 只读动作被纳入需确认路径（High 风险）的产品理由未在架构定义中逐条说明，本卡只记录事实。

## 相关合同

- `K05` 授权范围与冻结确认：本项消费该合同；`Folder` 作用域类别与「明确授权」的关系归该合同。
- `K10` 工具面版本与派发观察：工具面组成、版本与参数处置要求的唯一权威。
- `K06` 统一预算账本：`ExternalRead` 类别额度与累计的唯一权威。
- `K17` 审计事件与诊断查询：拒绝原因与失败分类的可解释性依据。
- 相邻工具：`T21` `fs_import_to_vault`（外部读取之后产生 vault 写入的路径）、`T22` `fs_export` / `T24` `fs_write_authorized_export`（反向的外部写入）、`T06` `read_note`（vault 内正文读取，无需确认且带显式截断语义）。
- 需求依据：`N12`（严格场景保留限制、授权覆盖范围）、`N18`（可定位的失败事实）。

<!-- iris:end T23 -->
