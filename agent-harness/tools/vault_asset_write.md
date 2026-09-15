# vault_asset_write

<!-- iris:object T38 kind=rules file=true -->

`vault_asset_write` 是 Iris 的内置业务工具之一（扩展能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T38 -->

<!-- iris:object T38 kind=tool name=vault_asset_write owner=M02 -->

## 用途与语义归属

确认目标后写入资源。语义归属「工具（`M2` / 文档服务）」，责任模块 `M02`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.3。

它写的是**二进制资源**（`assets/` 目录下的文件），不是 Markdown 笔记：授权与确认判定归 `M02`（`C04`／`C05`），实际落盘由 `C06` 边界调用资源原语完成（落点 [tool_dispatch/vault.rs](../../src-tauri/src/ai_runtime/tool_dispatch/vault.rs)、[storage/asset_operations.rs](../../src-tauri/src/storage/asset_operations.rs)）。资源原语的注释明确：版本快照、模型参数与索引都不属于该边界。

相邻工具：`T35` `vault_create_note`（新建笔记）。

## 参数与消费

| 参数          | 类型   | 必填 | 消费事实                                                                                       |
| ------------- | ------ | ---- | ---------------------------------------------------------------------------------------------- |
| `path`        | string | 是   | Vault 相对资源路径（目录描述给出示例 `assets/image.png`）；缺失或非字符串时拒绝 `missing path` |
| `data_base64` | string | 是   | 标准 base64 编码的二进制数据；缺失或非字符串时拒绝 `missing data_base64`                       |

解码与前置条件（顺序即拒绝顺序）：

1. `decode_asset` 先 `trim`，再按编码串长度上限拒绝（`20MB.div_ceil(3) × 4`）以免在解码前分配超大缓冲，然后做标准 base64 解码（失败为「无效的资源数据」），最后校验解码结果非空且不超过 20 × 1024 × 1024 字节（「资源数据为空」／「资源超过 20MB 限制」）。
2. `is_vault_asset_path`：路径必须以 `assets/` 开头、不含反斜杠、每段非空且不是 `.` 或 `..`，否则「资源路径必须位于 assets/ 下」。
3. 期望 Vault 先 `canonicalize`（失败即 `note_vault_changed`），再在公共 Vault 移动锁内复核当前 Vault 未切换。
4. 授权闭包 → `ensure_safe_file_parent`（父目录必须是安全的 vault 内目录；符号链接别名指向内部元数据目录时被拒绝）→ `resolve_vault_path` → `atomic_create`（硬链接发布，不覆盖既有文件，失败即整次调用失败）。

返回 `{"type":"vault_asset_write","path":…,"bytes":<解码后字节数>}`。与 `T35` 不同，**本工具不回传内容哈希、版本 id 或索引状态**：资源既不进笔记索引，也不产生版本快照。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["vault.manage"]`；`access_level = WriteMarkdown` 是展示元数据，且与实际写入对象（二进制资源）不一致；该字段只用于展示与预算回落，预算分类由 `requires_confirmation` 决定，因此不改变实际权限。
- 权限原子（`C04`）：`VaultAssetsWrite`，Medium，`supported = true`。
- 目录声明 `requires_confirmation = true`：本工具是确认类动作，提议先冻结变更集再等用户确认；冻结记录目标路径、原始参数、回滚说明与身份信息，有效期 10 分钟。
- 目标边界：`ensure_note_write_allowed` 对资源路径同样执行 Run 存活检查、写目标命中检查（冻结目标集合由 `path` 键组成）、文档策略 `ApplyChange` 与活跃 Skill 范围检查（拒绝码见「失败反馈」）。
- 内容绑定：本工具没有 `base_content_hash` 参数，冻结集不含内容基线；「不覆盖」由发布原语的原子创建语义保证。**原始参数会随冻结计划的 `change` 字段持久化**，其中包含 `data_base64` 编码后的资源内容；该持久化与其他数据边界的相容性不在本卡片核对范围（**待核对**）。
- 静态事实（**待核对**）：`vault.manage` 未出现在 [run_intake.rs](../../src-tauri/src/ai_runtime/run_intake.rs) 的能力构造中，授权快照输入 `requested_capabilities` 固定为空向量；本工具在当前普通 Run 路径上的可及性未运行验证。
- 同类核对：架构 §10.2 已就「目录权限分类与真实效果不符」登记了 `doc_normalize_markdown` 的例子（`G01`）；本工具的 `WriteMarkdown` 是否属于同一口径，本卡片不判定（**待核对**）。

## 副作用

- 文件系统：在 `assets/` 下创建资源文件（必要时创建安全父目录），不覆盖既有文件、不触碰其他文件；
- 不产生版本快照、不写回收站、不写笔记索引、不写长期记忆、不落证据账本；
- 资源原语不经过笔记写入服务，因此**不标记写守卫**（写守卫由笔记写入路径使用），本卡片只记录该事实（**待核对**：资源写入是否需要独立的漂移保护，未登记为 `G*`／`Q*`）；
- 工具审计只记录参数与结果的形状摘要（`shape=object, keys=N`），不记录资源内容、路径取值或字节数以外的细节；
- 已写入的资源属于已发生事实：失败后只报告回执，不假装回滚。

## 预算

- 预算分类为 `ConfirmedChange`（目录 `requires_confirmation` 优先于访问级别回落），受 `max_confirmed_change_calls`（当前三类预设均为 6）与总 `max_tool_calls`（当前 24）约束；不占用 local／network／external-read／runtime 额度（`K06`）。
- 单次调用的数据量由 20MB 上限与 base64 编码串长度上限共同约束，**但不能由预算维度表达**：额度按调用次数计，不按字节计（`K06` 的记账维度不含字节数）。
- 冻结前检查额度；混批提议整批拒绝；单次冻结最多 6 个操作与 6 个目标。
- 不在 `is_discovery()` 名单内；派发包裹在 30 秒超时中；不在自动重试白名单。

## 幂等

**不是幂等动作，但重复调用会失败而不是覆盖**：第二次执行时目标资源已存在，原子创建失败，整次调用报错且既有资源字节不变。相同参数重复超过 2 次会被循环以 `tool_call_repeated` 拒绝；已成功的同指纹调用以 `tool_call_already_succeeded` 拒绝。

## 取消

取消在两个时点生效：派发入口检查与锁内授权闭包的首个检查。取消后不创建目录、不发布文件。已经发布的资源只报告回执，不假装回滚；失败路径保证不留下半成品目录（原语测试覆盖「拒绝的范围与非法字节不产生目录」）。

## 失败反馈

| 情形                            | 反馈                                                                                    |
| ------------------------------- | --------------------------------------------------------------------------------------- |
| Run 已取消                      | 返回 `Cancelled`，不创建文件                                                            |
| 未经确认                        | 冻结变更集后以 `agent_run_confirmation_pending` 结束派发                                |
| 目标不在冻结范围                | 拒绝 `WriteTargetViolation`                                                             |
| 路径不在 `assets/` 下或含非法段 | 拒绝「资源路径必须位于 assets/ 下」                                                     |
| base64 非法                     | 拒绝「无效的资源数据」                                                                  |
| 解码结果为空或超过 20MB         | 拒绝「资源数据为空」／「资源超过 20MB 限制」                                            |
| 目标已存在                      | 原子创建失败（不覆盖、不改写既有资源）                                                  |
| 父目录不安全或指向内部元数据    | 路径校验拒绝，不创建目录                                                                |
| Vault 在提交期间变化            | 拒绝 `note_vault_changed`                                                               |
| 文档策略或 Skill 范围拒绝       | `agent_run_document_policy_denied` / `target path is outside the confirmed Skill scope` |
| 参数类型不符                    | `{"error":"tool_arguments_invalid"}`（非字段级说明）                                    |
| 超时                            | `{"error":"tool_dispatch_timeout","failure_class":"timeout", …}`                        |

可恢复错误按 `C14`／`N16` 处理：返回具体缺口让模型修正参数或目标，单次失败不终止任务；用户侧不得因此提示「模型能力降级」（`N17`）。

## 暴露规则

- 扩展能力目录项（`surface=extended`），位于目录的 `vault` 组；目录声明 `default_enabled_without_skill = false`，且测试断言写工具不进入无 Skill 的只读基础面。
- 运行期须在冻结能力中含 `vault.manage`；工具面装配时 `only_auto = true`（非 Durable 路径）会过滤掉确认类工具，因此本工具只在 Durable 效果路径上进入模型可见集合（`K10`）。
- 在「显式引用且检索范围不受限」的上下文中被隐藏（`constrain_for_run_context` 的允许清单不含本工具）。
- 受限子任务排除本工具：子 Run 白名单只含只读词汇，资源写入不在其中（`C15`）。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/vault.rs](../../src-tauri/src/ai_runtime/tool_catalog/vault.rs)（`vault_asset_write`，`Dispatchable`、`WriteMarkdown`、`requires_confirmation = true`）。
- 派发与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/vault.rs](../../src-tauri/src/ai_runtime/tool_dispatch/vault.rs)（`vault_asset_write_tool`）。
- 资源原语：[storage/asset_operations.rs](../../src-tauri/src/storage/asset_operations.rs)（`is_vault_asset_path`、`decode_asset`、`create_asset`、`MAX_ASSET_BYTES`）。
- 路径与原子发布：[storage/paths.rs](../../src-tauri/src/storage/paths.rs)（`ensure_safe_file_parent`、`resolve_vault_path`）、[storage/atomic_write.rs](../../src-tauri/src/storage/atomic_write.rs)（`atomic_create`）。
- 确认与冻结：[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)、[frozen_change_plan.rs](../../src-tauri/src/ai_runtime/frozen_change_plan.rs)。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`、处理器、解码上限、`assets/` 路径约束、原子创建与确认门槛在位）。`verification.state = none`。

不确定处（**待核对**）：`vault.manage` 的产生位置与可及性；`WriteMarkdown` 分类与二进制资源效果的对应关系；资源写入是否需要有别于笔记的漂移保护；冻结计划持久化 `data_base64` 的边界。以上未登记为 `G*`／`Q*`。未执行真实模型验收，本卡片不声明合同正确或用户任务可用。

## 相关合同

- `K05` 授权范围与冻结确认：登记关系为 `consumes`；确认绑定目标、内容、范围、版本与有效期（只引用，不复制其定义）。
- `K06` 统一预算账本（ConfirmedChange 分类额度，按调用次数而不按字节计）；`K10` 工具面版本与派发观察。
- 相关对象：`M02`、`C04`、`C05`、`C06`、`C16`、`C17`、`C15`；相邻工具 `T35`。
- 需求依据：`N16`、`N17`；语义归属与「确认目标后写入资源」的目标合同见架构 §6.3；目录分类一致性的核对要求见 `G01`。

<!-- iris:end T38 -->
