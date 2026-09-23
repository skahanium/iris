# git_read_status

<!-- iris:object T17 kind=rules file=true -->

`git_read_status` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T17 -->

<!-- iris:object T17 kind=tool name=git_read_status owner=M06 -->

## 用途与语义归属

读取当前 vault 工作树的 Git 状态摘要（分支与变更清单），**不返回文件正文**。语义归属为工具（`M6` / Git 服务），责任模块 `M06`；目录归属 `boundary` 组（[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)），暴露面 `extended`。

架构定义给本项的目标合同与取舍是「**仅在 Git 任务中暴露**」（[架构定义](../../docs/agent-architecture.md) §6.3）。本卡只引用该措辞，不重复定义其他对象的正文。

目录描述自带的边界措辞是「读取当前 vault 的 git status 摘要，不返回文件正文」：本工具报告的是变更清单与分支名，**正文内容**不在返回载荷内——但**变更文件名**本身仍属于 vault 结构信息，是否向模型外发受 `C04` 的文档策略与作用域约束（`K05`）。

## 参数与消费

| 参数             | 类型    | 声明边界   | 处理器行为                                                                                           |
| ---------------- | ------- | ---------- | ---------------------------------------------------------------------------------------------------- |
| `max_chars`      | integer | 默认 12000 | `max_chars(args, 12_000)`：非 `u64` 或缺失按 12000；**实际钳制区间为 100–60000**，声明未写出该上下限 |
| `path`（未声明） | —       | —          | 若模型额外传入，**被静默忽略、不报错**（本项 schema 无 `additionalProperties: false`）               |

`path` 一行不是建议参数，而是**待核对的口径差异**：`K10` 要求「所有暴露参数必须被消费或明确拒绝，不得存在静态忽略的参数」，而本项 schema 未声明该字段、处理器也不读它。相同形状的问题已由 `G01` 覆盖（工具卡逐项实现状态与目录分类的一致性核对是其所需证据之一）。

返回：`type`（`"git_read_status"`）、`scope`（固定 `"vault"`）、`status`（截断后的文本）、`sandbox_profile`（`git_read_status:l1_subprocess`）。

实际执行的命令是 `git status --short --branch`，固定 `current_dir = state.vault_path()`，并清除出站环境后只设 `LANG=C`。

**处理器不先校验 vault 是否是一个 Git 仓库**：非仓库目录下由 `git status` 自身失败，错误并入同一条 `git command failed: …` 文本。本卡不据此声称「Git 任务识别」已实现。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["git.read"]`。这是**精确按工具名映射**的能力标识，`access_level` 只是展示元数据，不得用来放宽 Run 授权。
- 派发前的能力判定（`C17`）：`decide_tool_permission` 在 `is_authorized_by(authorized_capabilities)` 不成立时直接返回 `Denied`，原因文本为「tool is outside the immutable Run capability contract」——**能力不在冻结合同内时本工具在派发边界即被拒绝**，与是否被列入某轮工具面无关。
- 权限原子（`C04`）：`Atom::GitReadStatus`，`Risk::Low`，`supported = true`。
- 确认语义：目录项 `requires_confirmation = false`，因此 `preflight_tool_permission` 给出 `PermissionDecision::Allow`，判定结果为 `AutoAllowed`。**本项不进入 `C05` 的冻结确认路径**（写侧的 `git_write_commit` 才需要确认，见 `T27`）。
- 已持久化的有效授权（`Allow`／`AllowForSession`，且 effect 全覆盖）会先于上述默认判定被采用；高／严重风险不允许写入会话级授权（`agent_permissions.rs` 的 `upsert_permission_grant`），`GitReadStatus` 属低风险，不受该限制。
- 子任务：本工具在 `subagent_coordinator.rs` 的 `CHILD_SAFE_TOOLS` 只读白名单中，父级工具面已含本项时子运行可继承，授权只继承或收窄。

## 副作用

**无状态变更。** 处理器只以只读方式执行一次 `git status`，不写文件、不写数据库、不改索引、不落证据。

需要分清三类不同事实：① **不写状态**；② **进程外副作用面**——执行子进程本身是真实动作，L1 profile 的约束是「应用级」，实现文件的原文即写明 _not an OS sandbox: no seccomp, namespace, chroot, or container isolation_；③ 命令注入的当前缓解是**固定参数数组、无用户可控参数进入命令、`env_clear()`、固定 `cwd`、禁用 hooks 与 LFS filter**，而非沙箱隔离。

返回载荷中的 `sandbox_profile` 是**能力声明**，用于让 UI 与审计不把 L1 误报成 L2（`SandboxLevel::L2OsBoundary` 的 `support = Unsupported`）。

## 预算

- `budget_class()` 归入 `ToolBudgetClass::Local`（无 `execution_metadata`，`access_level = ReadIndex`、`requires_confirmation = false`，故不落入 `ConfirmedChange`）。
- 当前三个生产预设（Standard／Delegated／DurableApply）的 `max_local_tool_calls = 12`、`max_tool_calls = 24`；`Direct` 预设把各分类额度置 0。这些是**主循环分类额度**，不是全局统一总数——数值权威在 `K06`，本卡不新设也不调整任何数值。
- **不进发现调用配额**：`is_discovery()` 只含 `search_hybrid`、`search_semantic`、`search_keyword`、`list_vault`、`web_search`（每模型回合上限 2 个发现动作）——本项不受该限制，但仍逐次计入 Local 额度与总工具调用数。
- 派发包裹在 30 秒超时中（`DEFAULT_TOOL_DISPATCH_TIMEOUT`）；`git_read_status` **不在** `is_retryable_tool_error` 的自动重试白名单内（该白名单只含 `web_search`／`web_fetch` 的超时与网络类错误）。
- `max_chars` 只截断返回文本，不构成调用次数或时长上的预算。

## 幂等

读取语义上是幂等的：同一 vault 状态下重复调用不产生重复变更。但**返回值随时间与工作树状态变化**，同一 Run 内两次调用可以给出不同的变更清单；消费者不得把一次状态输出当作可缓存的稳定事实，需要一致基准时应复用同一次观察。

本工具没有版本或哈希参数，也就**没有** `read_note` 那种「显式版本校验后拒绝漂移」的读取侧对应物（见 `T06`）：它报告的是「当前是什么样」，不是「我上次读到的还是不是同一个东西」。

## 取消

- 派发前执行 `ctx.ensure_run_active()`（位于 `dispatch_tool_inner` 的统一入口），已请求取消的 Run **不会启动 git 子进程**。
- 处理器是同步 `Command::output()`，无执行中途检查点：取消阻止后续调用，**不中断已启动的 git 子进程**。
- 部分结果：命名管道式输出在进程结束后一次性读取，不留下「半份状态」——失败即整体失败，成功即完整（在 `max_chars` 截断范围内的完整输出）。

## 失败反馈

- 缺少必填参数：本项无必填参数，不会出现 `missing path` 一类字段级错误。
- 已声明字段的类型错误：由派发前的参数校验（`guardrails::verify_tool_args`，按目录 schema 递归校验对象、必填字段与基础类型）**阻断**，不会进入处理器；错误文本形如 `invalid arguments for tool 'git_read_status': …`。
- 缺失的可选字段：按默认值处理，**不报错**（`max_chars` 缺失即 12000）。
- Git 侧失败（非仓库、git 不可执行、索引异常等）：统一返回 `git command failed: <stderr 前 400 字符>`，**不区分失败种类**，也不声明 vault 是否为仓库。
- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`，与 Git 失败属不同通道。
- 越权：能力不在冻结合同内时由派发边界拒绝，原因见「授权」节；`deny` 不因重试或换参数转为 `allow`（`K05`）。
- 用户侧表达：单次本工具错误不得被升级为「模型出错／模型能力降级」（`N17`）；恢复轨迹归 `C14`／`C26`／`C27`，诊断定位按「未知工具 → `C11` → `C16` → `C17`」的边界口径处理（架构定义 §8.5）。

## 暴露规则

- 目录状态 `Dispatchable`；`default_enabled_without_skill = false`，即**不因「无技能」而默认启用**。
- 进入某轮工具面的必要条件是 Run 冻结能力含 `git.read`（`git.read` 不在则工具面与派发两级都不可用）。
- 目标合同是「仅在 Git 任务中暴露」（架构定义 §6.3）。**当前源码的可机械核对事实是：按能力标识过滤，`capability_affinity` 由 `access_level = ReadIndex` 统一推导为 `SearchNotes`**，并未按任务类型再收窄。因此本卡不宣称该目标已落地，也不宣称「本工具出现在某轮工具面即等于观察到了 Git 任务识别」。
- 在 `ContextMode::ExplicitReferences` 且检索范围不受限时，`constrain_for_run_context` 收敛到一份显式允许清单（`read_note`、`get_outline`、`get_context_packets`、`get_backlinks`、`web_search`、`web_fetch`、`spawn_subagent`）——**本工具不在其中**，该场景下不可见。
- 子任务：`CHILD_SAFE_TOOLS` 只读白名单成员，父级工具面含本项时可继承。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)（`Dispatchable`、`Access::ReadIndex`、`requires_confirmation = false`、`max_results = None`、无 `execution_metadata`）；组装入口 [tool_catalog/groups.rs](../../src-tauri/src/ai_runtime/tool_catalog/groups.rs)。
- 能力与预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`required_capability_ids`、`is_authorized_by`、`budget_class`、`is_discovery`）。
- 派发路由与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)（`DISPATCHABLE_TOOL_NAMES`、`is_exposable_tool`、`dispatch_tool_inner` 的 `ensure_run_active`）、[tool_dispatch/boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs)（`git_read_status_tool`、`run_git`、`max_chars`、`truncate_chars`）。
- 沙箱档位：[sandbox_profile.rs](../../src-tauri/src/ai_runtime/sandbox_profile.rs)（`sandbox_profile_for_tool`）。
- 权限原子：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`permission_profile_for_tool`）。
- 派发前判定与审计：[tool_execution_pipeline.rs](../../src-tauri/src/ai_runtime/tool_execution_pipeline.rs)、[permission_decision.rs](../../src-tauri/src/ai_runtime/permission_decision.rs)。
- 工具面过滤：[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs)（`for_run`、`tools_for_authorized_capabilities`、`constrain_for_run_context`）、[normal_run_service.rs](../../src-tauri/src/ai_runtime/normal_run_service.rs)。
- 子任务白名单：[subagent_coordinator.rs](../../src-tauri/src/ai_runtime/subagent_coordinator.rs)（`child_tool_surface`）。
- 目录契约测试：[tool_catalog/tests.rs](../../src-tauri/src/ai_runtime/tool_catalog/tests.rs)。**本工具无独立处理器测试模块**：[tool_dispatch/tests.rs](../../src-tauri/src/ai_runtime/tool_dispatch/tests.rs) 只覆盖另一组工具的路径与版本语义，边界组处理器不在其中。

## 当前状态

- 文档成熟度 `draft`（登记见 [catalog.mjs](../catalog.mjs)；正文变化需显式接受并分类，见 [README.md](../README.md) §八）。
- `implementation.state = present`（静态源码事实：目录项 `Dispatchable`，派发器有真实处理器，沙箱档位与能力／权限映射在位）。**这只描述源码事实，不表示合同正确，也不表示用户任务可用。**
- `verification.state = none`：本轮未执行任何实验。本卡不声明「已修复」「已验证通过」。
- 不确定处（**待核对**）：① `max_chars` 声明未写出 100–60000 的实际钳制区间；② 未声明「忽略额外参数」与 `K10`「参数必须消费或明确拒绝」的口径差异（不单独立项，沿用 `G01` 的证据要求）；③ 「仅在 Git 任务中暴露」与当前按能力过滤的实现之间缺少任务类型级收窄，本卡只记录事实、不判定缺口归属；④ 非 Git 仓库下 `git status` 的具体错误文本未逐条核对。

## 相关合同

- `K05` 授权范围与冻结确认：本项消费该合同；本卡只引用，不复制其正文。
- `K10` 工具面版本与派发观察：工具面组成、版本与「参数已消费或明确拒绝」要求的唯一权威。
- `K06` 统一预算账本：分类额度与累计的唯一权威；本卡只说明本项归入哪个类别。
- `K17` 审计事件与诊断查询：拒绝原因与派发状态的可解释性依据。
- 相邻工具：`T18` `git_read_diff`（看差异）、`T19` `git_read_log`（看历史）、`T27` `git_write_commit`（写侧，需确认）。
- 需求依据：`N18`（内部错误审计覆盖工具与模型行为，提供可定位的失败事实）；暴露面与目录差异见 `G01`。

<!-- iris:end T17 -->
