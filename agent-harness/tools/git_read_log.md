# git_read_log

<!-- iris:object T19 kind=rules file=true -->

`git_read_log` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T19 -->

<!-- iris:object T19 kind=tool name=git_read_log owner=M06 -->

## 用途与语义归属

读取**获准仓库**的提交历史摘要（单行格式、含装饰）。语义归属为工具（`M6` / Git 服务），责任模块 `M06`；目录归属 `boundary` 组（[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)），暴露面 `extended`。

架构定义给本项的目标合同与取舍原文是「**读取获准仓库历史**」（[架构定义](../../docs/agent-architecture.md) §6.3）。这句取舍有两个要点：① 读取对象是**仓库历史**（提交元数据与单行标题），不是工作树正文；② 「获准」是前提——历史读取仍受授权范围约束。本卡只引用该措辞。

当前处理器的实际读取目标是**当前 vault**：命令固定 `current_dir = state.vault_path()`，不接受任意仓库路径参数。因此「哪些仓库算获准」在本工具上表现为「vault 是否可读、是否是一个 Git 仓库、Run 是否持有 `git.read`」，而不是一个独立的仓库白名单。

## 参数与消费

| 参数                            | 类型    | 声明边界                    | 处理器行为                                                                                                                            |
| ------------------------------- | ------- | --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `limit`                         | integer | 默认 20，声明 `maximum: 50` | 非 `u64` 或缺失按 20；**再 `clamp(1, 50)`**，声明未写出下限 1；实际传给 git 的 `-n` 取钳制后的值，并以字符串回显在载荷的 `limit` 字段 |
| `max_chars`                     | integer | 默认 12000                  | `max_chars(args, 12_000)`：非 `u64` 或缺失按 12000；**实际钳制区间 100–60000**，声明未写出该上下限                                    |
| 额外字段（如 `path`、`author`） | —       | —                           | **被静默忽略、不报错**（schema 无 `additionalProperties: false`）                                                                     |

「额外字段被静默忽略」与 `K10` 的「所有暴露参数必须被消费或明确拒绝」存在口径差，沿用 `G01` 的证据要求（工具卡逐项实现状态与目录分类的一致性核对）。

实际命令为 `git log --oneline --decorate -n <limit>`；`--oneline` 决定了本工具**只返回单行提交摘要**，不返回提交正文、作者邮箱或文件清单——需要更细的提交内容时没有对应参数。

返回：`type`（`"git_read_log"`）、`scope`（固定 `"vault"`）、`limit`（**字符串**形式的生效值）、`log`（截断后的文本）、`sandbox_profile`（`git_read_log:l1_subprocess`）。

`limit` 的回显类型是字符串而非数字，消费者按载荷解析时需按文本处理；本卡只记录该事实。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["git.read"]`（与 `T17`、`T18` 共用同一能力标识）。
- 派发前能力判定（`C17`）：能力不在冻结 Run 合同时返回 `Denied`，原因「tool is outside the immutable Run capability contract」。
- 权限原子（`C04`）：`Atom::GitReadLog`，`Risk::Low`，`supported = true`。
- 确认语义：`requires_confirmation = false` → `PermissionDecision::Allow` → 预先允许（`AutoAllowed`），**不进入** `C05` 冻结确认。
- **「获准仓库」的当前实现口径**：处理器只接受当前 vault 作为工作目录，且依赖 `state.vault_path()` 成功；它**不校验**该目录是否真是 Git 仓库（非仓库目录由 git 自身失败）。授权侧的收窄来自 `C04` 能力判定与 Run 冻结范围，**而不是**一个处理器侧的仓库许可清单——本卡不宣称仓库级白名单已经存在。
- 子任务：`CHILD_SAFE_TOOLS` 只读白名单成员，父级工具面含本项时可继承；子运行不能借此读到父级授权以外的仓库。

## 副作用

**无状态变更**：只读一次 `git log`，不写文件、不写数据库、不改索引。

三类事实分开陈述：① 不写状态；② 子进程是真实动作，L1 profile 仅为应用级约束，**不是** OS 沙箱（`L2OsBoundary` 当前 `support = Unsupported`）；③ 提交标题与分支／tag 装饰属于 vault **结构信息**，返回后进入模型上下文；其外发受 `C04` 授权与 `K05` 范围约束，本工具自身不做文档策略判定（入参中无作用域上下文）。

`max_chars` 截断在字符边界追加 `…`，载荷中**没有** `truncated` 字段：消费者无法区分「历史就这么长」与「被截断」。与 `read_note` 的显式截断语义形成对照（`T06`）。

## 预算

- `budget_class()` 归入 `ToolBudgetClass::Local`（无 `execution_metadata`；`access_level = ReadIndex` 且 `requires_confirmation = false`）。
- 与 `T17`、`T18` 共用同一类别额度：当前生产预设（Standard／Delegated／DurableApply）`max_local_tool_calls = 12`、`max_tool_calls = 24`；`Direct` 置 0。数值权威在 `K06`。
- **不进发现配额**：`is_discovery()` 不含本项。
- `limit` 只约束单次返回的提交条数（上限 50），**不是**调用次数额度；逐次调用仍计入 Local 类别。
- 派发包裹在 30 秒超时中；**不在**自动重试白名单（白名单只含 `web_search`／`web_fetch` 的超时与网络类错误）。

## 幂等

读取语义幂等：重复调用不产生变更。返回值随仓库状态变化——新提交、分支切换、tag 变化都会改变 `log`；`--decorate` 使分支／tag 引用位置一并出现在输出中，故同一提交区间在不同引用状态下可以给出不同文本。

本工具无版本或哈希参数，不提供「历史是否被改写」的校验（对照 `T06` 的 `content_hash` 语义）。

## 取消

- 派发前执行 `ctx.ensure_run_active()`（统一入口 `dispatch_tool_inner`），已取消的 Run 不启动子进程。
- 处理器同步执行 `Command::output()`，无中途检查点：取消阻止后续调用，不中断已启动的 `git log`。
- 不留部分结果：失败整体失败；成功路径的截断是**未声明的实现副作用**（见「副作用」节）。

## 失败反馈

- 参数无效：已声明字段的类型错误由派发前的参数校验（`guardrails::verify_tool_args`）**阻断**，错误文本形如 `invalid arguments for tool 'git_read_log': …`；缺失的可选字段按默认值处理而不报错。`limit` 超出声明上界（例如 1000）时**不被拒绝**，而是被静默钳制到 50，载荷回显钳制后的值。校验失败文本也不含「允许类型／必要字段」的逐项说明，与 `M06`／`G01` 对字段级校验的要求存在口径差，本卡只记录事实。
- Git 侧失败：统一 `git command failed: <stderr 前 400 字符>`；非仓库目录、无提交历史（空仓库）等在此通道内不作区分。
- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`。
- 越权：能力不在冻结合同内即派发前拒绝；`deny` 不因重试或换参数转为 `allow`（`K05`）。
- 用户侧：单次失败不得升级为「模型能力降级」（`N17`）；恢复与归因轨迹归 `C14`／`C26`／`C27`（`K17`）。

## 暴露规则

- 目录状态 `Dispatchable`；`default_enabled_without_skill = false`。
- 工具面与派发两级都需要冻结能力含 `git.read`。
- `capability_affinity` 由 `access_level = ReadIndex` 统一推导为 `SearchNotes`；Git 读取三件套的任务类型收敛条件见 `T17` 的记录，本卡不重复。
- `ContextMode::ExplicitReferences` 且检索范围不受限时，`constrain_for_run_context` 的显式允许清单**不含本项**，该场景下不可见。
- 子任务：`CHILD_SAFE_TOOLS` 成员。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)（`Dispatchable`、`Access::ReadIndex`、`requires_confirmation = false`、`max_results = None`，schema 声明 `limit` 默认 20、`maximum` 50）。
- 能力与预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)。
- 派发路由：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)；处理器与 `run_git`：[tool_dispatch/boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs)（`git_read_log_tool`：`clamp(1, 50)`、`--oneline --decorate`）。
- 沙箱档位：[sandbox_profile.rs](../../src-tauri/src/ai_runtime/sandbox_profile.rs)。
- 权限原子：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)。
- 派发前判定：[tool_execution_pipeline.rs](../../src-tauri/src/ai_runtime/tool_execution_pipeline.rs)、[permission_decision.rs](../../src-tauri/src/ai_runtime/permission_decision.rs)。
- 工具面过滤：[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs)、[normal_run_service.rs](../../src-tauri/src/ai_runtime/normal_run_service.rs)。
- 子任务白名单：[subagent_coordinator.rs](../../src-tauri/src/ai_runtime/subagent_coordinator.rs)。
- **无本工具的独立处理器测试**：[tool_dispatch/tests.rs](../../src-tauri/src/ai_runtime/tool_dispatch/tests.rs) 不覆盖边界组。

## 当前状态

- 文档成熟度 `draft`。
- `implementation.state = present`（静态源码事实：`Dispatchable`、真实处理器、沙箱档位与权限映射在位）。**不表示合同正确，也不表示用户任务可用。**
- `verification.state = none`；本卡不声明「已修复」「已验证通过」。
- 不确定处（**待核对**）：① 「获准仓库」目前等同于「当前 vault 可读 + 持有 `git.read`」，没有独立仓库许可清单与仓库级别的策略判定；② 载荷 `limit` 为字符串而非数字，前端／消费者解析口径未核对；③ `max_chars` 截断未声明（无 `truncated` 字段）；④ 额外参数静默忽略与 `K10` 参数处置要求不一致（沿用 `G01`）；⑤ 空仓库（无提交）下的具体错误文本未逐条核对。

## 相关合同

- `K05` 授权范围与冻结确认：本项消费该合同；本卡只引用。
- `K10` 工具面版本与派发观察：工具面组成、版本与参数处置要求的唯一权威。
- `K06` 统一预算账本：分类额度权威。
- `K17` 审计事件与诊断查询：派发状态与拒绝原因的可解释性依据。
- 相邻工具：`T17` `git_read_status`、`T18` `git_read_diff`、`T27` `git_write_commit`。
- 需求依据：`N12`（授权覆盖范围与严格场景限制）、`N18`（可定位的失败事实）。

<!-- iris:end T19 -->
