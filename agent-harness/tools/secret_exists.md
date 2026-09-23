# secret_exists

<!-- iris:object T20 kind=rules file=true -->

`secret_exists` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T20 -->

<!-- iris:object T20 kind=tool name=secret_exists owner=M06 -->

## 用途与语义归属

查询某个具名凭据**是否存在**，返回值只有布尔事实，**不读取明文值**。语义归属为工具（`M6` / 凭据服务），责任模块 `M06`；目录归属 `boundary` 组（[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)），暴露面 `extended`。

架构定义给本项的目标合同与取舍原文是「**仅查询存在性，不读明文，不向普通任务默认暴露**」（[架构定义](../../docs/agent-architecture.md) §6.3）。三句取舍各自对应一条事实边界：

1. **仅存在性**：返回 `exists: bool`；凭据内容是事实属主（凭据存储），不是本工具的输出。
2. **不读明文**：架构定义把 `secret_read_plaintext` 列为 **Planned 占位但明确禁止实现与暴露**的能力，不是等待开发的计划（§6.4）；`M06` 的不变量第 8 条同此。本工具是这条禁止的正面形态——agent 侧只允许知道「有没有」。
3. **不向普通任务默认暴露**：见「暴露规则」节。

本卡只引用上述措辞，不复制其他对象的正文。

## 参数与消费

| 参数                                | 类型   | 声明边界                                                  | 处理器行为                                                                                                                                                                                   |
| ----------------------------------- | ------ | --------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `service`                           | string | **required**，声明「仅允许 `iris.llm.*` 或 `iris.mcp.*`」 | 必填；缺失即 `service is required`。取值经 `validate_credential_service` 校验，**拒绝前缀之外的服务名**（如 `evil.llm.deepseek`、`iris.llm.`、含空格或斜杠的写法），错误文本同时说明违规原因 |
| 额外字段（如 `value`、`plaintext`） | —      | —                                                         | **被静默忽略**（schema 无 `additionalProperties: false`）                                                                                                                                    |

`service` 的取值规则（源码事实）：必须以 `iris.llm.` 或 `iris.mcp.` 开头；后缀非空、不以点开头或结尾、不含 `..`；后缀字符限于小写字母、数字、`.`、`_`、`-`。测试中 `iris.llm.deepseek`、`iris.llm.custom_2`、`iris.mcp.anysearch` 为合法样例，`iris/llm/deepseek`、`evil.llm.deepseek` 为拒绝样例。

返回：`type`（`"secret_exists"`）、`service`（回显入参）、`exists`（布尔）。载荷**不含**任何凭据值、长度、指纹或更新时间：返回面本身不构成侧信道式的明文泄露路径。

本工具**没有声明「存在性查询范围」之外的参数**（没有 provider 列表、没有批量查询），因此一次调用只回答一个服务名。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["secret.metadata.read"]`。这是**元数据读取**能力，与写侧的 `secret.manage`、明文侧的 `secret.plaintext.read` 分属不同标识。
- 派发前能力判定（`C17`）：能力不在冻结 Run 合同时返回 `Denied`，原因「tool is outside the immutable Run capability contract」。
- 权限原子（`C04`）：`Atom::SecretExists`，`Risk::Low`，`supported = true`；作用域类别为 `PermissionScopeKind::Global`（不是文件或目录范围）。
- 确认语义：目录项 `requires_confirmation = false` → `PermissionDecision::Allow` → 预先允许（`AutoAllowed`）。**本项本身不进入 `C05` 冻结确认路径**；但「不默认暴露」与「被列入工具面」是两件事，见「暴露规则」。
- 已持久化的有效授权会被优先采用；低风险允许写入会话级授权（高风险／严重风险被 `upsert_permission_grant` 拒绝写入会话级授权，本项不属于该限制）。
- 子任务：本工具**不在** `subagent_coordinator.rs` 的 `CHILD_SAFE_TOOLS` 白名单中，子运行不能凭父级工具面继承本项。

## 副作用

**无状态变更、无凭据写入。** 处理器只做两次纯查询式调用：服务名校验（`validate_credential_service`）与存在性查询（`credential_available`）；不写文件、不写数据库、不修改凭据存储、不落审计正文。

按项目安全红线（[AGENTS.md](../../AGENTS.md) §1.4），凭据不得写入明文文件、日志、数据库或环境变量，也不得在日志、调试输出、错误消息中输出；本工具的存在性答案属元数据，**明文值从不进入返回值**——因此它不构成上述红线的例外，也不因「只查存在性」而获得读取明文的任何授权。

需要分开的两类事实：① 本工具不产生副作用；② 存在性答案会让模型得知「某服务的 Key 是否已配置」，这属于配置状态信息而不是密钥内容；其外发仍受 `C04` 授权与 Run 冻结范围约束。

## 预算

- `budget_class()` 归入 `ToolBudgetClass::Local`（无 `execution_metadata`；`access_level = ReadIndex` 且 `requires_confirmation = false`，故不落入 `ConfirmedChange`，也不落入 `Runtime`）。
- 与本地读取类工具共用 Local 类别额度：当前生产预设（Standard／Delegated／DurableApply）`max_local_tool_calls = 12`、`max_tool_calls = 24`；`Direct` 置 0。数值权威在 `K06`，本卡不新设数值。
- **不进发现配额**：`is_discovery()` 不含本项，不受「每模型回合最多 2 个发现动作」限制。
- 派发包裹在 30 秒超时中；**不在**自动重试白名单（白名单只含 `web_search`／`web_fetch` 的超时与网络类错误）。
- 出于「不默认暴露」的取舍，本项的**实际调用额度不是主要约束**：能否进入工具面先由授权与暴露规则决定。

## 幂等

查询语义幂等：同一服务名重复查询不产生变更。返回值可能随**用户配置行为**变化——用户新增或删除 Key 会改变 `exists`，因此它不是可长期缓存的稳定事实。

本工具没有版本或时间参数，无法回答「这个凭据是什么时候配置的」或「它是否仍然有效」；`exists = true` **只表示存储中存在条目**，不证明该凭据可用、未被撤销或与当前 provider 匹配。

## 取消

- 派发前执行 `ctx.ensure_run_active()`（统一入口 `dispatch_tool_inner`），已取消的 Run 不执行查询。
- 处理器为同步、无 `await` 点的调用，因此**不存在执行中途取消窗口**；超时与取消是两个不同通道。
- 不留部分结果：布尔答案无法部分完成。

## 失败反馈

- 缺少必填参数：`service is required`（字段级说明）。
- 非法服务名：`不允许的凭据服务名: <service>（…）`，同时给出违规原因（后缀为空／含非法字符等）。这类错误**不可通过重试同一参数恢复**，应改服务名或告知用户，而不是重复调用。
- 凭据后端失败（本地存储不可用等）：以 `AppError` 文本返回，不降级为 `exists: false`——**「查不到」不能冒充「不存在」**。
- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`。
- 越权：能力不在冻结合同内即派发前拒绝；`deny` 不因重试或换参数转为 `allow`（`K05`）。
- 用户侧：单次失败不得升级为「模型能力降级」（`N17`）；恢复与归因轨迹归 `C14`／`C26`／`C27`（`K17`）。

## 暴露规则

- 目录状态 `Dispatchable`；`default_enabled_without_skill = false`。
- 进入工具面与派发均需冻结能力含 `secret.metadata.read`。
- 架构 §6.3 的目标取舍是「**不向普通任务默认暴露**」。当前可机械核对的实现事实是：目录项 `default_enabled_without_skill = false`，且能力标识是一个专属的元数据读取能力，因此它**只有在 Run 冻结能力显式包含该标识时才可能出现**。本卡只记录这两条事实，**不宣称**存在按「普通任务／凭据任务」区分的额外收窄逻辑。
- `capability_affinity` 由 `access_level = ReadIndex` 统一推导为 `SearchNotes`。
- `ContextMode::ExplicitReferences` 且检索范围不受限时，`constrain_for_run_context` 的显式允许清单**不含本项**，该场景下不可见。
- 子任务：**不在** `CHILD_SAFE_TOOLS` 白名单中，子运行不继承。
- 相邻但禁止的对象：`secret_create_update` 与 `secret_read_plaintext` 都是 **Planned 占位**，且 `secret_read_plaintext` 属「明确禁止实现与暴露」；本工具的存在**不**为它们提供实现路径或名称别名。
- 无本项自身的 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/boundary.rs](../../src-tauri/src/ai_runtime/tool_catalog/boundary.rs)（`Dispatchable`、`Access::ReadIndex`、`requires_confirmation = false`、`max_results = None`、schema `required: ["service"]`）。
- 能力与预算分类：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)（`"secret_exists" => &["secret.metadata.read"]`）。
- 派发路由：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)；处理器：[tool_dispatch/boundary.rs](../../src-tauri/src/ai_runtime/tool_dispatch/boundary.rs)（`secret_exists_tool`）。
- 服务名校验：[security/ipc_policy.rs](../../src-tauri/src/security/ipc_policy.rs)（`validate_credential_service`，含同文件测试）。
- 存在性查询：[credentials.rs](../../src-tauri/src/credentials.rs)（`credential_available`、`credential_available_with_backend`）。
- 权限原子与作用域：[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)（`Atom::SecretExists`、`PermissionScopeKind::Global`）。
- 派发前判定：[tool_execution_pipeline.rs](../../src-tauri/src/ai_runtime/tool_execution_pipeline.rs)、[permission_decision.rs](../../src-tauri/src/ai_runtime/permission_decision.rs)。
- 工具面过滤：[tool_executor.rs](../../src-tauri/src/ai_runtime/tool_executor.rs)、[normal_run_service.rs](../../src-tauri/src/ai_runtime/normal_run_service.rs)。
- 子任务白名单（对照：本项不在名单内）：[subagent_coordinator.rs](../../src-tauri/src/ai_runtime/subagent_coordinator.rs)。
- **无本工具的独立处理器测试**：[tool_dispatch/tests.rs](../../src-tauri/src/ai_runtime/tool_dispatch/tests.rs) 不覆盖边界组；服务名校验的测试位于 [security/ipc_policy.rs](../../src-tauri/src/security/ipc_policy.rs) 的测试模块内。

## 当前状态

- 文档成熟度 `draft`。
- `implementation.state = present`（静态源码事实：`Dispatchable`、真实处理器、服务名白名单校验与权限映射在位）。**不表示合同正确，也不表示用户任务可用。**
- `verification.state = none`；本卡不声明「已修复」「已验证通过」。
- 不确定处（**待核对**）：① 「不向普通任务默认暴露」目前只体现为专属能力标识与 `default_enabled_without_skill = false`，是否存在任务类型级的额外收窄未在源码中核对到；② 额外字段静默忽略与 `K10` 的参数处置要求不一致（沿用 `G01`）；③ 凭据存储后端不可用时返回的具体错误文本类别未逐条核对，本卡只记录「不得降级为 `exists: false`」这一原则；④ 本工具对计费或路由选择的实际影响（例如模型据 `exists` 结果改换 provider）未在本轮核对。

## 相关合同

- `K05` 授权范围与冻结确认：本项消费该合同；作用域类别为 Global，属请求级判定。
- `K10` 工具面版本与派发观察：工具面组成、版本与参数处置要求的唯一权威。
- `K06` 统一预算账本：分类额度权威。
- `K17` 审计事件与诊断查询：拒绝原因的可解释性依据；审计不记录密钥内容。
- 相邻工具：`T03` `capabilities_read`（能力事实查询，与本项的凭据配置事实查询分属不同事实来源）。
- 需求依据：`N19`（开箱即用工具与用户自行付费的优质服务并存：需要能判断某服务是否已配置）、`N18`（可定位的失败事实）。

<!-- iris:end T20 -->
