# vault_version_list

<!-- iris:object T39 kind=rules file=true -->

`vault_version_list` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T39 -->

<!-- iris:object T39 kind=tool name=vault_version_list owner=M06 -->

## 用途与语义归属

读取获准版本列表。语义归属「工具（`M6` / 文档服务）」，责任模块 `M06`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.3。

它是**版本元数据的只读入口**：返回某篇笔记的版本快照清单（编号、标签、内容哈希、字数、是否定稿、种类、创建时间），不返回版本正文，也不恢复、不删除、不修改任何版本。版本正文预览由另一条入口承担（`version_preview`），本工具不暴露它。

责任归属上，本卡片的读取路径确实落在 `M06`／文档服务：授权与范围判定由 `C04` 与文档策略给出，读取本身不产生副作用（落点 [tool_dispatch/vault.rs](../../src-tauri/src/ai_runtime/tool_dispatch/vault.rs)、[version/mod.rs](../../src-tauri/src/version/mod.rs)）。

## 参数与消费

| 参数   | 类型   | 必填 | 消费事实                                                       |
| ------ | ------ | ---- | -------------------------------------------------------------- |
| `path` | string | 是   | 目标笔记的 Vault 相对路径；缺失或非字符串时拒绝 `missing path` |

处理器顺序（都在公共 Vault 移动锁内）：

1. 复核当前 Vault 未切换（否则 `note_vault_changed`）；
2. `ensure_note_model_read_allowed`：对该路径依次检查文档策略的 `Discover`、`Read` 与 `SendToModel` 三项能力，任一被拒即 `agent_run_document_policy_denied`；
3. `ensure_retrieval_scope_allows_path`：不在本轮冻结检索范围内即 `agent_run_retrieval_scope_violation`；
4. `ensure_active_skill_scope_allows_path`：不在已确认 Skill 范围内即「target path is outside the confirmed Skill scope」；
5. `validate_user_note_relative_path`：拒绝内部元数据路径与路径别名；
6. 调用 `version_list`，并过滤掉 `is_legacy_unscoped` 的行。

返回 `{"type":"vault_version_list","path":…,"versions":[…],"count":<n>}`，每条含 `id`、`file_id`、`version_no`、`label`、`content_hash`、`word_count`、`is_finalized`、`kind`、`created_at`。源码事实：

1. 过滤后的结果中 `is_legacy_unscoped` 恒为 `false`——**归属不明的旧历史不进入本工具的结果**。「读取获准版本列表」中的「获准」不仅指路径获准，也指版本可以归属到当前 Vault；界面另有显式包含未分配历史的读取入口（`version_list_including_unassigned`）。
2. 查询条件为「`note_path` 匹配且（`vault_path` 为当前库或为空）且 `recycle_id IS NULL`」，`ORDER BY created_at DESC`；**没有 LIMIT**，处理器不读取目录声明的 `max_results`（`Some(50)`）。
3. 结果不含版本正文；`content_hash` 与 `word_count` 属于元数据。
4. 范围不足与「没有版本」可区分：前者是错误结果，后者是 `count = 0` 的正常结果。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["vault.read"]`；`access_level = ReadIndex` 是展示元数据，不得扩大 Run 授权。
- 权限原子（`C04`）：`VaultVersioning`，Low，`supported = true`；目录声明 `requires_confirmation = false` 且风险为 Low，因此预检决策为 `Allow` → `AutoAllowed`，不进入冻结确认路径。
- 版本读取的准入门槛是**两条**而不是一条：能力 `vault.read` 必须在冻结能力快照中，且目标路径必须同时通过文档策略、检索范围与 Skill 范围检查（见「参数与消费」）。任一不通过都返回错误，不返回空列表冒充「没有版本」。
- 锁内重新校验 Vault 身份，避免在路径解析与读取之间发生库切换。
- `vault.read` 由 [run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs) 在「本轮建立了隐式 Vault 依赖」时加入冻结能力集，因此本工具是本批 7 个工具中依赖的能力在当前授权路径上确有生产来源的一个（其余 6 个依赖 `vault.manage`／`schedule.read`／`schedule.manage`，见各卡片的待核对项）。

## 副作用

**无文件写入、无数据库写入、无版本写入。** 处理器只执行一次版本元数据查询与内存过滤。

- 不产生快照、不恢复版本、不清理版本、不改 `is_finalized`；
- 不刷新索引、不写长期记忆、不落证据账本（`register_local_tool_evidence` 的登记分支不含本工具）；
- 该查询在**主连接**（`with_conn`）上执行的是 SELECT，未观察到写语句（对照记录：已知的「读取路径写库」问题位于 MCP provider 列举，`F04`／`Q05`，不在本工具路径上）；
- 工具审计只记录参数与结果的形状摘要（`shape=object, keys=N`），不记录路径、标签或哈希取值；
- 读取持公共 Vault 移动锁，因此与普通保存／移动／回收串行，不产生并发写入。

## 预算

- 预算分类为 `Local`（`ReadIndex` 回落），受 `max_local_tool_calls`（当前三类预设均为 12）与总 `max_tool_calls`（当前 24）约束；不占用 network／external-read／runtime／confirmed-change 额度（`K06`）。
- 不在 `is_discovery()` 名单内，不占用每模型回合 2 个发现调用的配额。
- 派发包裹在 30 秒超时中；不在自动重试白名单。
- 返回体受普通工具结果 8 000 字符预算约束（`MAX_TOOL_RESULT_CHARS`）：版本很多的笔记可能被有界裁剪，而处理器没有分页参数可用。

## 幂等

纯读取，重复调用不产生副作用。结果随版本库变化而变（新建快照、移动笔记、回收、删除版本后同参数结果不同）。消费者不得把一次列表当作稳定快照，也不得把 `count = 0` 当作「该笔记不存在」。

## 取消

派发入口检查 Run 是否已取消；已取消的 Run 不执行查询。与写入类工具不同，读取路径的授权闭包**不含** Run 存活检查，锁内不再复核 Run 状态；查询在单次同步连接内完成，没有中途取消检查点，也不产生部分结果。

## 失败反馈

| 情形                       | 反馈                                                                             |
| -------------------------- | -------------------------------------------------------------------------------- |
| 参数类型不符               | `{"error":"tool_arguments_invalid"}`（非字段级说明）                             |
| `path` 缺失                | 拒绝 `missing path`                                                              |
| 路径为内部元数据或路径别名 | 路径校验拒绝                                                                     |
| 文档策略不允许读取或送模   | 拒绝 `agent_run_document_policy_denied`                                          |
| 不在检索范围               | 拒绝 `agent_run_retrieval_scope_violation`                                       |
| 不在已确认 Skill 范围      | 拒绝「target path is outside the confirmed Skill scope」                         |
| Vault 在读取期间变化       | 拒绝 `note_vault_changed`                                                        |
| 版本快照不可读             | 本工具只返回元数据，不读取正文，因此不触发快照解密错误；这类失败属于版本预览路径 |
| 超时                       | `{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`                 |

空结果是有效观察，不得当作传输故障重复同一请求。可恢复错误按 `C14`／`N16` 处理，返回具体缺口让模型改查；用户侧不得因一次工具错误提示「模型能力降级」（`N17`）。

## 暴露规则

- 扩展能力目录项（`surface=extended`），位于目录的 `vault` 组；目录声明 `default_enabled_without_skill = true`（本批 7 个工具中唯一如此），且不需要确认——但该字段当前只有测试消费者，生产暴露由能力过滤与上下文约束决定。
- 运行期须在冻结能力中含 `vault.read`；不需要确认，因此在 `only_auto = true` 与 `false` 两种装配下都不会被确认过滤掉（`K10`）。
- 在「显式引用且检索范围不受限」的上下文中被隐藏：`constrain_for_run_context` 的允许清单只含 `read_note`、`get_outline`、`get_context_packets`、`get_backlinks`、`web_search`、`web_fetch`、`spawn_subagent`，本工具不在其中。
- 受限子任务的工具白名单（`CHILD_SAFE_TOOLS`）**包含**本工具，因此只读委派任务可以看到版本元数据。
- 评测侧把本工具登记为「由 vault／context 能力准入的生产本地读取工具」之一（[agent_capacity_eval_tests.rs](../../src-tauri/src/ai_runtime/agent_capacity_eval_tests.rs) 的名称归一化断言）。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/vault.rs](../../src-tauri/src/ai_runtime/tool_catalog/vault.rs)（`vault_version_list`，`Dispatchable`、`ReadIndex`、`max_results = Some(50)`）。
- 派发与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/vault.rs](../../src-tauri/src/ai_runtime/tool_dispatch/vault.rs)（`vault_version_list_tool`）。
- 版本查询：[version/mod.rs](../../src-tauri/src/version/mod.rs)（`version_list`、`version_list_including_unassigned`、`VERSION_SELECT`、`VersionEntry`）。
- 范围与策略：[tool_dispatch/context.rs](../../src-tauri/src/ai_runtime/tool_dispatch/context.rs)、[tool_dispatch/note.rs](../../src-tauri/src/ai_runtime/tool_dispatch/note.rs)（`ensure_note_model_read_allowed`）、[storage/paths.rs](../../src-tauri/src/storage/paths.rs)（`validate_user_note_relative_path`）。
- 能力与权限映射：[tool_catalog/capability.rs](../../src-tauri/src/ai_runtime/tool_catalog/capability.rs)、[agent_permissions.rs](../../src-tauri/src/ai_runtime/agent_permissions.rs)。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`、处理器、三项范围检查与版本查询在位）。`verification.state = none`。

不确定处（**待核对**）：目录 `max_results = Some(50)` 在处理器中未被消费，版本数量多时依赖 8 000 字符结果预算裁剪；归属不明的旧历史既不进入本工具结果也没有在本卡片范围内找到面向模型的替代读取入口；读取路径不复核 Run 存活状态是否有意为之。以上未登记为 `G*`／`Q*`。未执行真实模型验收，本卡片不声明合同正确或用户任务可用。

## 相关合同

- `K05` 授权范围与冻结确认：登记关系为 `consumes`；本工具不进入确认路径，但读取仍以授权判定与范围为准（只引用，不复制其定义）。
- `K10` 工具面版本与派发观察；`K06` 统一预算账本（Local 分类额度）。
- 相关对象：`M06`、`M07`、`C04`、`C16`、`C17`、`C22`（版本元数据不构成笔记正文证据）；相邻工具 `T06` `read_note`、`T37` `vault_delete_to_trash`。
- 需求依据：`N17`；语义归属与「读取获准版本列表」的目标合同见架构 §6.3；工具目录目标与现状差异见 `G01`。

<!-- iris:end T39 -->
