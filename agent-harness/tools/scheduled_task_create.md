# scheduled_task_create

<!-- iris:object T32 kind=rules file=true -->

`scheduled_task_create` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T32 -->

<!-- iris:object T32 kind=tool name=scheduled_task_create owner=M02 -->

## 用途与语义归属

**沿已有登记语义登记一条计划任务**，不承诺后台自动执行。语义归属为工具／计划服务，责任模块 `M02`（权限与受控副作用）。

目标合同的两半都必须同时成立才算落地：

1. **登记**：把用户确认过的任务写入应用状态，返回它的标识；
2. **不承诺自动执行**：目标合同与工具返回都明确——「Iris 在没有调度器／自动化审批路径的情况下不会运行主动任务」。

当前实现满足这两半：写入一行登记记录并返回 `id`，返回体自带说明字段 `"note": "Task registered only; Iris does not run proactive tasks without a scheduler/automation approval path."`。**当前没有任何后台调度器消费这张表**：仓库内对 `scheduled_tasks` 的读写仅出现在本工具、`T33` `scheduled_task_list` 与 `T34` `scheduled_task_delete` 三个处理器中，没有其它调用方。因此「登记成功」不等于「将来会被执行」，用户可见表达必须保持这个区分。

## 参数与消费

目录 schema：`title`（string，**required**）、`prompt`（string，**required**）、`schedule`（string，**required**，描述为「自然语言或 cron 风格描述」）。

三个参数都被消费，且逐字段校验并归一化：

- 缺失：`missing title`／`missing prompt`／`missing schedule`；
- `trim()` 后为空（任一字段）：`scheduled_task_create requires non-empty fields`；
- 非字符串：`as_str()` 失败，走「缺失」分支。
- `schedule` **只按字符串存储**：当前实现不解析 cron 表达式、不校验语法、不计算下次执行时间。传入不可解析或语义含糊的描述会注册成功——模型与用户都不能据此认为「计划已生效」。
- **当前不存在声明未消费的参数**（与 `T30` 的 `paragraph` 情况不同）。
- 无长度上限声明；实际受 SQLite 与工具结果信封约束，**具体上限待核对**。

返回：`{ "ok": true, "id": <新行 id>, "note": "Task registered only; …" }`，HTTP 层无「部分成功」概念——要么插入成功，要么返回错误。

## 授权

- 能力合同（`C16`）：`["schedule.manage"]`（与 `T34` `scheduled_task_delete` 同能力；`T33` 查询用 `schedule.read`）。
- 权限原子（`C04`）：`Atom::AppStateWrite`，Risk = Medium，`supported = true`；与 `memory_write`、`saved genre template`、`update_user_rule` 共用同一原子。
- 确认要求：目录 `requires_confirmation = true` ⇒ `RequiresConfirmation`，必须先冻结为变更计划并经用户确认（`C05`、`K05`）。Medium 风险允许 Session 范围授权（High/Critical 才被拒绝）。
- 不涉及联网、不读 Vault 正文；不受联网开关或检索范围影响。
- 子任务：不在 `C15` 的 `child_tool_surface` 白名单内，子运行不获得它。
- 冻结目标：参数中没有路径字段，因此冻结计划把目标记为 `application://scheduled-tasks/new`（[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs) 的 `frozen_relative_paths`），回退说明为「可通过应用设置撤销或更新」（`rollback_summary`）。

## 副作用

**有数据库写入**：向 `scheduled_tasks` 表插入一行（`title`、`prompt`、`schedule`、`enabled = 1`、`created_at`／`updated_at` 为当前时间）。不改写用户笔记、不写文件、不建索引、不改配置。

绑定与复验的当前事实：

- 确认绑定的是**参数内容**（标题、提示词、计划描述）与一次执行身份，**没有版本或基准 hash 可复验**（该表没有内容哈希列，参数也不含 `base_content_hash`）；因此不存在类似笔记写入的「确认后内容漂移即拒绝」门槛。
- 应用状态写入仍需可持久化的变更回执：架构 §3 M9 规定「必要授权或变更回执无法持久化时，不执行新的持久副作用」。
- 表结构无唯一约束：`title`／`prompt`／`schedule` 都可重复，**登记去重不在存储层完成**（[migrations/021_skill_lifecycle_metadata.sql](../../src-tauri/migrations/021_skill_lifecycle_metadata.sql)）。
- **`rollback_summary` 写的是「可通过应用设置撤销或更新」，但当前仓库中只有本工具组（`T32`／`T33`／`T34`）读写该表**：`src/` 与 `src-tauri/src/commands/` 中只找到前端展示名映射，没有计划任务管理界面或独立 IPC 命令。也就是说，撤销路径实际仍需模型调用 `T34`。该表述与现状的对应关系**待核对**（本卡不据此断言设置界面存在或不存在，只记录本次核对范围）。

## 预算

- `requires_confirmation = true` ⇒ `ToolBudgetClass::ConfirmedChange`（[capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)）。确认批次要求**整批都是需确认调用**，混合批次以 `mixed_confirmation_batch` 拒绝。
- 当前预设 `max_confirmed_change_calls = 6`（三预设同值，[run_contract.rs](../../src-tauri/src/ai_runtime/run_contract.rs)）；这是主循环分类额度，不是全局统一总数，数值权威在 `K06`。
- 单次派发包裹在 30 秒超时内（`DEFAULT_TOOL_DISPATCH_TIMEOUT`）；SQLite 插入是本地同步操作，实际不会接近该上限。不在自动重试白名单内。
- 目录 `max_results = None`；`is_discovery()` 为 false，不占发现调用配额。
- 每次登记一行、无批量参数：不存在「一次调用登记多条」的放大路径。

## 幂等

**不幂等，且无去重键。** 同一 `title`／`prompt`／`schedule` 重复确认执行会插入**多行**——存储层没有唯一约束，处理器也不查询既有登记。

当前可依赖的机械保护（循环层而非本工具的幂等设计）：

- 同 Run 内已成功执行的同指纹调用返回 `tool_call_already_succeeded`；同回合内同指纹超过 `MAX_REPEAT_CALLS` 返回 `tool_call_repeated`；
- 确认计划一次性消费：执行以 checkpoint 推进（`Dispatching` → `Applied`），进程恢复只继续未执行后缀，不重放已应用的操作（[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)）。

跨 Run 的重复登记需要调用方（模型或用户）自查 `T33` `scheduled_task_list` 的结果——目标合同没有规定登记去重语义，本卡不代替它作承诺。

## 取消

取消在派发入口生效：`dispatch_tool_inner` 首先执行 `ctx.ensure_run_active()`；已取消的 Run 不进入处理器。

确认路径的取消语义：确认前取消不产生任何写入；**确认后执行中的取消没有中断检查点**（插入是单条同步语句），因此可能出现「用户已取消、登记已生成」的结果——必须按事实报告，不假装回滚（`scheduled_tasks` 没有回收站式恢复）。

取消不返还已消耗的 `ConfirmedChange` 额度。

## 失败反馈

- **可恢复（参数类）**：缺字段报 `missing title`／`missing prompt`／`missing schedule`；全空白字段报 `scheduled_task_create requires non-empty fields`；执行门对缺字段或类型不符返回 `{"error":"tool_arguments_invalid"}`（**不是字段级说明**，字段级反馈属 `G01` 目标）。
- **需用户操作**：未确认前不执行；已有有效授权不反复索要（`K05`）。
- **不可恢复（存储类）**：数据库写入失败（如回执无法持久化）返回错误并**不执行后续副作用**；不自动降级为「内存登记后稍后重试」。
- **语义缺口必须先说明而不是报成功**：`schedule` 不可解析时当前不报错，登记仍成功。按目标合同，工具**不承诺后台自动执行**，因此回复用户时必须说明「只登记、未执行」——把登记说成「已设定定时执行」属于超出合同与实现的宣称。
- **用户侧表达**：不得因一次工具错误提示「模型能力降级」（`N17`）；`Q15` 类「写入时机未确定」的问题同样适用于计划任务的自动化边界，本卡不代替它给出结论。
- 归因：工具结果经 `C17` 的统一派发审计（工具名、成功状态、耗时、安全摘要）；登记内容属于应用状态，不得进入日志或诊断正文。

## 暴露规则

- 目录 `default_enabled_without_skill = false`；不在核心无技能只读名单内。
- 目标工具面：扩展目录项，任务需要、能力启用且授权满足时进入（架构 §6.3）。它是「管理任务」类动作，属架构 §6.5 的「编辑应用或管理任务」行：只加需要的写工具，不开放全部变更能力。
- 实际门禁是 Run 冻结能力含 `schedule.manage`（`C16` 的 `is_authorized_by`）。
- **暴露缺口（本卡核对所得，尚未登记为 `G*`）**：基线 `2670739f` 中 `schedule.manage`（与 `schedule.read`）没有找到任何授予位置；`RunIntake` 加入的能力清单不含它们（[run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs)）。因此按当前路径，本工具虽为 `Dispatchable`，**可能无法进入任何 Run 的工具面**——与 `T33`／`T34` 同源。该结论限于静态核对，**待核对**是否存在其他授予入口。
- 子任务不暴露（见「授权」）。Planned 项不暴露；本工具不是 Planned。

## 源码落点

- 目录定义：[tool_catalog/root.rs](../../src-tauri/src/ai_runtime/tool_catalog/root.rs)（`scheduled_task_create`：`WriteSettings`、`requires_confirmation = true`、`max_results = None`、`default_enabled_without_skill = false`）。
- 处理器：[tool_dispatch/schedule.rs](../../src-tauri/src/ai_runtime/tool_dispatch/schedule.rs)（`scheduled_task_create_tool`：字段 trim 与空值校验、`INSERT INTO scheduled_tasks`、返回 `note`）。
- 表结构：[migrations/021_skill_lifecycle_metadata.sql](../../src-tauri/migrations/021_skill_lifecycle_metadata.sql)（`scheduled_tasks`，无唯一约束）。
- 能力与预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`schedule.manage`、`budget_class`）。
- 权限画像：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`AppStateWrite`，Risk::Medium）。
- 冻结与回退说明：[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)（`frozen_relative_paths` 的 `application://scheduled-tasks/new`、`rollback_summary`）。
- 派发分支与前端展示名：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[src/lib/tool-display-names.ts](../../src/lib/tool-display-names.ts)。
- 现有测试：本次核对未发现针对本工具处理器的专项测试；[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs) 的 `run_capability_contract_exposes_only_the_requested_patch_tools` 断言在仅 `note.apply_patch` 授权时本工具**不出现**在工具面（事实陈述，非缺陷定性）。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，处理器、表结构与确认路径在位）。`verification.state = none`。

已知缺口：`schedule.manage` 授予缺口（见「暴露规则」，未登记为 `G*`）；`schedule` 无语法校验与解析；登记去重缺失；`rollback_summary` 的撤销路径与仓库现状的对应关系待核对。未执行真实模型验收，不声明计划任务在实际使用中可被管理或被自动执行。

## 相关合同

- `K05` 授权范围与冻结确认：确认身份与冻结路径。
- `K06` 统一预算账本：`ConfirmedChange` 分类额度的唯一权威。
- 相邻工具：`T33` `scheduled_task_list`（登记查询，`schedule.read`）、`T34` `scheduled_task_delete`（登记删除，`schedule.manage`）、`T14` `memory_write`（同权限原子的应用状态写入）。
- 需求依据：本工具由架构定义 §6.3 的扩展目录定位给出（沿已有登记语义，不承诺后台自动执行）。
- 依据文档：[docs/agent-architecture.md](../../docs/agent-architecture.md) §3 M2、§6.3、§6.5；[ARCHITECTURE.md](../../ARCHITECTURE.md)（分层中 Scheduler 只调用内部 `sync_due_batch`，与计划任务登记不是同一套机制——本卡据此不把两者混为一谈）。

<!-- iris:end T32 -->
