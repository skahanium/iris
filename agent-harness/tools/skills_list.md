# skills_list

<!-- iris:object T04 kind=rules file=true -->

`skills_list` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T04 -->

<!-- iris:object T04 kind=tool name=skills_list owner=M06 -->

## 用途与语义归属

发现技能方法：列出已确认的 prompt-only Iris Skills（全局作用域与当前 vault 作用域），并给出每条的校验状态、缺失依赖与可注入性。语义归属为工具（`M6`），责任模块 `M06`（`C16` 工具目录、工具面与技能接入）。

**关键边界（架构定义原文的取舍）**：发现技能方法，**不自动获得执行权限**。列出即列出：本工具不激活技能、不安装依赖、不改变授权；技能本身是 prompt 材料（`SKILL.md`），不是可执行代码，也不能新增工具或权限。技能方法与材料如何进入上下文由 `C07` 决定，不在本工具范围。

## 参数与消费

目录 schema 声明为空对象 `{"type":"object","properties":{}}`，**不声明任何参数**；`dispatch_skill_tool()` 显式忽略 `args`（形参 `_args`）。因此本工具没有「暴露但未消费」的参数。

输入来自应用状态而非模型：`state.vault_path()` 与 `state.cached_skills_for_vault(&vault)`——即**每 vault 的技能注册缓存**，不是每次调用重新扫描文件系统。源码注释明确说明理由：工具派发发生在 Run 内，不得扫描用户可控目录；缓存由 vault 激活与显式 UI 刷新／确认边界填充。缓存缺失时 `unwrap_or_default()` 产生**空列表**，与「确实没有任何技能」在模型侧不可区分（见「失败反馈」）。

返回 `Vec<SkillListEntry>`：内含 `SkillEntry` 的扁平字段，以及 `validation`（`valid`／`legacy`／`invalid`）、`missing_deps`、`kind`、`activation_ready`；`task_active`／`task_score` 等评分字段在本路径为 `None` 并被跳过序列化（本工具不评分、不决定是否注入）。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["skills.read"]`。
- 权限原子（`C04`）：`Atom::AppStateRead`，Low，`supported = true`，`requires_confirmation = false`。
- 读取的是已确认技能的元数据；`activation_ready` 以 `enabled`、校验状态、依赖齐备与 `confirmation_status == Confirmed` 共同判定，未确认技能不会因此获得注入资格。
- **技能不能增权**：本工具只读，读到的技能条目本身不携带权限；任何技能驱动的动作仍须走 `C04` 与 `K10` 的工具面规则。
- 子任务继承：只读、低风险，随父级冻结能力快照进入子运行；子运行读技能列表不改变其工具面。

## 副作用

**无文件写入、无数据库写入、不扫描文件系统。** 处理器只读一次进程内注册缓存并做纯函数映射（`skill_list_entries` 的注释明确「不触碰文件系统」）。输出策略与证据策略未声明专门元数据（`execution_metadata = None`）。

## 预算

- 无 `execution_metadata`，预算分类走回退规则：`access_level = ReadIndex` 且不需确认 → `ToolBudgetClass::Local`。
- 因此与 `T05`－`T08` 共用 Local 类别上限（当前预设 `max_local_tool_calls = 12`，`max_tool_calls = 24`；主循环分类额度，不是全局统一总数；数值权威在 `K06`）。
- 不进每回合 2 次发现调用上限（`is_discovery()` 不含本工具）。
- 派发包裹在 30 秒超时中；处理器为纯内存读，实际耗时远低于上限。

## 幂等

纯读取，重复调用不产生副作用。返回值随缓存刷新变化：用户增删技能、切换 vault 或显式刷新后结果不同；同一次缓存代内结果一致。消费者不得把列表当作执行许可，也不得假定「列表为空」等价于「技能不可用」。

## 取消

派发前执行 `ctx.ensure_run_active()`；已取消的 Run 不再读取缓存。处理器同步、无 `await` 点，无执行中途取消窗口。不留下部分结果。

## 失败反馈

- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", ...}`。
- vault 未就绪：`state.vault_path()` 返回错误（`AppResult`），经派发层成为失败的工具结果。
- 缓存缺失：**返回空数组而非错误**——空缓存与「无技能」不可区分。是否属于需要显式表达的缺口，本卡标为**待核对**：`G01` 只覆盖目录分类与 `request_tools` 参数消费，未登记该表达问题。
- 参数无效：本工具无参数；未声明字段当前被静默忽略，与 `G01` 的「无效参数必须明确拒绝」要求不一致。
- 用户侧：单次失败不得表达为「模型能力降级」（`N17`）。

## 暴露规则

- `default_enabled_without_skill = true`。注意字段名的含义是「不需要先有技能即可默认启用」——本工具是技能面的**入口**，它的存在不以技能已激活为前提。
- 目标工具面中属普通助手对话「基础 4 个」之一（时间、应用上下文、能力查询、技能列表）。
- 例外：`ContextMode::ExplicitReferences` 且检索范围不受限时被 `constrain_for_run_context` 隐藏；无 Range 的 @ 材料场景不应展示 vault 级技能清单。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/skills.rs](../../src-tauri/src/ai_runtime/tool_catalog/skills.rs)（`Dispatchable`、`ReadIndex`、`max_results = None`、`execution_metadata = None`）。
- 派发分支与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`skills_impl::is_skill_tool` 分支）、[tool_dispatch/skills.rs](../../src-tauri/src/ai_runtime/tool_dispatch/skills.rs)（`dispatch_skill_tool`、`skills_list_tool`，含「不得扫描用户目录」的注释）。
- 列表构造：[skills/scan.rs](../../src-tauri/src/ai_runtime/skills/scan.rs)（`skill_list_entries`，注释说明「不触碰文件系统」）。
- 缓存来源：[app.rs](../../src-tauri/src/app.rs)（`cached_skills_for_vault`、`cached_skills` 的注册表读取）；缓存跨文件系统变化存活直到显式刷新，见同文件测试 `cached_skills_survive_filesystem_changes_until_explicit_refresh`。
- 条目结构：[skills/model.rs](../../src-tauri/src/ai_runtime/skills/model.rs)（`SkillListEntry`、`validation_status`、`SkillConfirmationStatus`）。
- 权限与能力分类：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`AppStateRead`，Low，supported）、[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`skills.read`、`is_discovery()` 不含本工具）。
- 相关 IPC（非工具面）：`skills_list` 命令在 [commands/ai_commands.rs](../../src-tauri/src/commands/ai_commands.rs) 注册——与工具同名但入口不同，本卡不把 IPC 行为混入工具合同。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，派发分支与处理器在位，列表构造为纯函数）。`verification.state = none`。

不确定处（见上）：空缓存与无技能不可区分**待核对**；技能条目的字段是否足以支撑模型选择方法（`task_active`／`task_score` 在此路径恒为 `None`）**待核对**。未执行真实模型验收，不声明合同正确或用户任务可用。

## 相关合同

- `K10` 工具面版本与派发观察：本工具的暴露与技能面事实的唯一权威。
- 相邻工具：`T03` `capabilities_read`（工具面事实）；技能方法与上下文装配的分工属 `C16`／`C07`，不在本卡展开。
- 需求依据：`N01`（基础对话可靠）；技能不得增权的取舍来自架构定义 §3 M6 与 §6.2。

<!-- iris:end T04 -->
