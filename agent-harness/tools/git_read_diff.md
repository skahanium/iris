# git_read_diff

<!-- iris:object T18 kind=rules file=true -->

`git_read_diff` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T18 -->

<!-- iris:object T18 kind=tool name=git_read_diff owner=M06 -->

## 用途与语义归属

读取当前 vault 工作树的 Git 差异摘要；**默认只返回 stat**，只有显式要求时才返回补丁正文。语义归属为工具（`M6` / Git 服务），责任模块 `M06`；目录归属 `boundary` 组（[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)），暴露面 `extended`。

架构定义给本项的目标合同与取舍是「**正文外发仍受文档策略约束**」（[架构定义](../../docs/agent-architecture.md) §6.3）。这条取舍的实质是：本工具能取到**文件正文级别的差异内容**，因此它不属于「只读元数据」那一档，必须按文档外发策略判定。本卡只引用该措辞，不重复定义其他对象的正文。

默认 `include_patch = false` 是一个**收敛外发面的默认值**：默认路径只给出 `--stat`（文件名与增删行计数），需要正文时由模型显式提高请求强度。

## 参数与消费

| 参数                          | 类型    | 声明边界   | 处理器行为                                                                                         |
| ----------------------------- | ------- | ---------- | -------------------------------------------------------------------------------------------------- |
| `include_patch`               | boolean | 默认 false | 非布尔或缺失按 false；`false → git diff --stat -- .`，`true → git diff -- .`（相对于 vault 根）    |
| `max_chars`                   | integer | 默认 12000 | `max_chars(args, 12_000)`：非 `u64` 或缺失按 12000；**实际钳制区间 100–60000**，声明未写出该上下限 |
| 额外字段（如 `path`、`refs`） | —       | —          | **被静默忽略、不报错**（schema 无 `additionalProperties: false`）                                  |

「额外字段被静默忽略」与 `K10` 要求的「所有暴露参数必须被消费或明确拒绝」构成口径差异；该差异沿用 `G01` 的证据要求（工具卡逐项实现状态与目录分类的一致性核对），本卡不另立缺口编号。

返回：`type`（`"git_read_diff"`）、`scope`（固定 `"vault"`）、`includePatch`（回显实际生效值）、`diff`（截断后的文本）、`sandbox_profile`（`git_read_diff:l1_subprocess`）。

命令固定为 `current_dir = state.vault_path()`，`env_clear()` 后只设 `LANG=C`，并禁用 hooks 与 LFS filter。**处理器不校验 vault 是否为 Git 仓库**，也不接受 `ref`／`range` 参数——它只读工作树相对索引的差异，不能读任意提交区间。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["git.read"]`（与 `T17`、`T19` 同一能力标识，**三者不因风险差异而分档**）。
- 派发前能力判定（`C17`）：能力不在冻结 Run 合同时返回 `Denied`，原因为「tool is outside the immutable Run capability contract」。
- 权限原子（`C04`）：`Atom::GitReadDiff`，`Risk::Low`，`supported = true`。
- 确认语义：目录项 `requires_confirmation = false`，属预先允许路径（`AutoAllowed`），**不进入** `C05` 冻结确认。
- **文档策略**：架构 §6.3 的目标取舍要求「正文外发仍受文档策略约束」。需要如实说明当前实现位置——处理器的入参里没有任何文档策略或作用域上下文（`git_read_diff_tool(state, args)` 只收 `state` 与 `args`），因此本项**没有** `read_note` 那种「文档策略 → 检索范围 → 技能作用域」的三道读取前判定。这不等于「外发不受约束」：`C04` 的能力判定与权限原子仍在派发边界生效；但在 `include_patch = true` 下取到的正文会被交给模型，其**外发**是否已被单独判定属本项待核对的差异，本卡不宣称目标已落地。
- 子任务：本工具在 `CHILD_SAFE_TOOLS` 只读白名单中，父级工具面含本项时可继承。

## 副作用

**无状态变更**：只读一次 `git diff`，不写文件、不写数据库、不改索引。

三类事实需分开：① 不写状态；② 子进程执行是真实动作，L1 profile 为应用级约束，**不是** OS 沙箱（`SandboxLevel::L2OsBoundary` 当前 `support = Unsupported`）；③ 差异正文一旦返回即进入模型上下文与后续证据链，其处理受 `K14`（证据身份）与 `K05`（外发范围）约束——**输出本身不是副作用，但它扩大了外发面的暴露**。

`max_chars` 截断在字符边界上追加 `…`，因此返回的补丁可能**不完整且未声明截断**（载荷中没有 `truncated` 字段可供消费者判断）。这一点与 `read_note` 的显式 `truncated`／`nextStartByte` 形成对照（`T06`），本卡不据此断言截断语义已被正确处理。

## 预算

- `budget_class()` 归入 `ToolBudgetClass::Local`（无 `execution_metadata`；`access_level = ReadIndex` 且 `requires_confirmation = false`）。
- 与 `T17`、`T19` **共用同一类别额度**：当前生产预设（Standard／Delegated／DurableApply）`max_local_tool_calls = 12`、`max_tool_calls = 24`；`Direct` 置 0。数值权威在 `K06`，本卡不新设数值。
- **不进发现配额**：`is_discovery()` 不含本项，不受「每模型回合最多 2 个发现动作」限制。
- 返回体量由 `max_chars` 控制（钳制上限 60000 字符），这**不是**额度承诺；`include_patch = true` 的载荷明显大于 stat 默认路径。
- 派发包裹在 30 秒超时中；**不在**自动重试白名单（白名单仅 `web_search`／`web_fetch` 的超时与网络类错误）。

## 幂等

读取语义上幂等：重复调用不产生重复变更。但**返回值随工作树变化**，且 `include_patch` 不同即不同请求（stat 与 patch 是两个不同强度的动作，不应互相冒充结果）。

本工具无版本或哈希参数，不提供「内容漂移后明确拒绝」的读取侧校验（对照 `T06` 的 `content_hash` 语义）：它报告「当前差异是什么」，不报告「我上次看到的差异是否还在」。

## 取消

- 派发前执行 `ctx.ensure_run_active()`（统一入口 `dispatch_tool_inner`），已请求取消的 Run 不启动子进程。
- 处理器为同步 `Command::output()`，无执行中途检查点：取消阻止后续调用，不中断已启动的 `git diff`。
- 不留下部分结果：失败即整体失败；成功路径的截断是**实现副作用而非声明的部分结果**（见「副作用」节的 `truncated` 缺位）。

## 失败反馈

- 参数无效：已声明字段的类型错误由派发前的参数校验（`guardrails::verify_tool_args`）**阻断**，错误文本形如 `invalid arguments for tool 'git_read_diff': …`；缺失的可选字段按默认值处理而不报错。**但本项没有字段级业务反馈**（例如 `include_patch` 传入非布尔值只得到统一的校验失败文本，不含「允许类型／必要字段」的逐项说明），这与 `G01`／`M06` 要求的「参数无效返回字段级校验问题、允许类型及必要字段」存在口径差，本卡只记录事实。
- Git 侧失败：统一 `git command failed: <stderr 前 400 字符>`；非仓库目录、索引损坏等在此通道内**不作区分**。
- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`。
- 越权与拒绝：能力不在冻结合同内即派发前拒绝；`deny` 不因换参数或重试转为 `allow`（`K05`）。
- 用户侧：不得因单次失败提示「模型能力降级」（`N17`）；诊断的可解释性归 `C26`／`C27`（`K17`）。

## 暴露规则

- 目录状态 `Dispatchable`；`default_enabled_without_skill = false`。
- 进入工具面与派发均需冻结能力含 `git.read`。
- `capability_affinity` 由 `access_level = ReadIndex` 统一推导为 `SearchNotes`；架构 §6.3 对 Git 读取三件套给出的收敛条件见 `T17` 对「仅在 Git 任务中暴露」的记录，本卡不重复。
- `ContextMode::ExplicitReferences` 且检索范围不受限时，`constrain_for_run_context` 的显式允许清单**不含本项**，该场景下不可见。
- 子任务：`CHILD_SAFE_TOOLS` 成员，父级工具面含本项时可继承。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)（`Dispatchable`、`Access::ReadIndex`、`requires_confirmation = false`、`max_results = None`）。
- 能力与预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)。
- 派发路由：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)；处理器与 `run_git`：[tool_dispatch/boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs)（`git_read_diff_tool`、`max_chars`、`truncate_chars`）。
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
- 不确定处（**待核对**）：① 「正文外发仍受文档策略约束」目前**没有处理器侧的策略／作用域判定**，只有派发边界的授权判定——目标与实现的差距归属需由 `M06`／`M02` 共同核对；② `max_chars` 截断未在载荷中声明（无 `truncated` 字段），消费者无法区分「差异就是这么短」与「被截断」；③ 额外参数静默忽略与 `K10` 的参数处置要求不一致（沿用 `G01`）；④ `include_patch = true` 的载荷上限与模型上下文的实际占用未测。

## 相关合同

- `K05` 授权范围与冻结确认：本项消费该合同；本卡只引用。
- `K10` 工具面版本与派发观察：工具面组成、版本与参数处置要求的唯一权威。
- `K14` 证据身份、来源与支持关系：差异正文进入结论时的来源与范围表达。
- `K06` 统一预算账本：分类额度权威。
- 相邻工具：`T17` `git_read_status`、`T19` `git_read_log`、`T27` `git_write_commit`。
- 需求依据：`N13`（外发资料边界冲突时先满足已授权的数据边界）、`N18`（可定位的失败事实）。

<!-- iris:end T18 -->
