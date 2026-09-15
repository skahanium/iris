# memory_write

<!-- iris:object T14 kind=rules file=true -->

`memory_write` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T14 -->

<!-- iris:object T14 kind=tool name=memory_write owner=M02 -->

## 用途与语义归属

确认后新增、修改或删除长期条目。语义归属「工具（`M2` → `M3`）」，责任模块 `M02`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.2。

它是长期记忆的**唯一模型可提议写入口**：授权与确认判定归 `M02`（`C04`／`C05`），条目存储、来源与失效表达归 `M03`（`C09`，落点 [tool_dispatch/memory.rs](../../src-tauri/src/ai_runtime/tool_dispatch/memory.rs)）。默认不把任意聊天内容自动写成长期事实；写入必须沿用户确认合同（`K08`、`N14`、讨论第十二节）。

## 参数与消费

| 参数        | 类型                                                 | 必填   | 默认     | 消费事实                                                                         |
| ----------- | ---------------------------------------------------- | ------ | -------- | -------------------------------------------------------------------------------- |
| `operation` | string，枚举 `upsert` / `delete_key` / `clear_scope` | 否     | `upsert` | 决定执行分支；未知值被拒绝                                                       |
| `key`       | string                                               | 按分支 | 无       | `upsert` 与 `delete_key` 必填（空串视为缺失）；键长上限 200 字符                 |
| `content`   | string                                               | 按分支 | 无       | 仅 `upsert` 使用；内容长上限 2 000 字符                                          |
| `scope`     | string，枚举 `global` / `vault`                      | 否     | `global` | 决定写入哪个作用域；`vault` 解析为当前 Vault 的 `vault:<vault_id>`，其他值被拒绝 |

分支级参数约束（当前实现明确拒绝而不是静默忽略）：

- `delete_key` 携带 `content` → 拒绝 `memory_delete_key_rejects_content`；
- `clear_scope` 携带 `key` 或 `content` → 拒绝 `memory_clear_scope_rejects_key_content`；
- `key` 或 `content` 超长 → 拒绝 `memory_write_exceeds_budget`。

返回体为 `ok`、`operation`、`scope`（只回报 `global` 或 `vault`）与 `affectedCount`。

## 授权

- `C04` 是唯一授权决定者；运行期准入门槛是能力 `memory.write`。
- 目录声明 `requires_confirmation = true`：本工具是**确认类**动作，未经确认不得执行；写入前宿主冻结待确认的变更集（目标、内容哈希、回滚说明，见 [frozen_change_plan.rs](../../src-tauri/src/ai_runtime/frozen_change_plan.rs) 与 [run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs) 的 `scheduled_change_targets` / `rollback_summary`），确认后才派发。
- 目录声明的 `access_level = WriteSettings` 只是展示元数据，不得扩大 Run 授权。权限原子映射为 `AppStateWrite`，风险级别 `Medium`。
- 一次模型响应里若混有确认类与非确认类调用，整批被拒绝（`mixed_confirmation_batch`），不允许用批处理绕过确认。

## 副作用

写应用状态层存储（长期记忆条目），可能新增、覆盖或删除条目：

- `upsert` 按 `(scope, key)` 冲突更新内容与时间戳，`source` 记为 `user_confirmed`；
- `delete_key` 删除指定作用域下的单个键；
- `clear_scope` 清空指定作用域的全部条目，**不触及**其他作用域与 `global`。

它不写 `.md`、不改笔记、不参与笔记索引重建；条目属于应用运行时状态而非笔记知识来源（`AGENTS` §1.3）。记忆副作用与文件写入虽都属受控变更，但不共用同一目标校验路径（记忆目标以 `application://memory/...` 或 `application://memory-scope/...` 表达）。

## 预算

按工具的单一预算分类 `ConfirmedChange` 计账（确认类优先于访问级别回落），受 `max_confirmed_change_calls` 约束（三类预设当前均为 6），并同时占用总工具调用上限（`max_tool_calls`，当前 24）；不占用本地、网络或运行时额度（`K06`）。

## 幂等

- `upsert` 对相同 `(scope, key, content)` 重复执行结果收敛为同一条目，重复调用不产生副本。
- `delete_key` 与 `clear_scope` 是幂等删除：目标不存在时 `affectedCount` 为 0，不报错、不产生其他副作用。
- 幂等由键与作用域而不是由内容哈希保证：不同内容写入同一键是覆盖，不是新增。

## 取消

派发边界在写库前检查 Run 取消标志；取消后不开始新的持久写入，也不把已冻结但未确认的变更视为已执行。已执行的记忆变更属于已发生事实，本工具只报告回执，不假装回滚。

## 失败反馈

| 情形                                | 反馈                                                               |
| ----------------------------------- | ------------------------------------------------------------------ |
| Run 已取消                          | 返回 `Cancelled`，不写入                                           |
| 未获确认                            | 请求确认并中断本轮派发（`CONFIRMATION_PENDING_ERROR`），不执行写入 |
| 确认后目标或内容变化                | 冻结的变更集与当前状态不一致时拒绝执行，要求重新确认               |
| `operation` 非法                    | 拒绝 `memory_operation_invalid`                                    |
| `scope` 非法或 Vault 作用域不可确定 | 拒绝 `memory_scope_invalid` / `memory_vault_scope_unavailable`     |
| 缺少 `key` 或 `content`             | 拒绝 `missing key` / `missing content`                             |
| 分支参数冲突                        | 按分支拒绝（见「参数与消费」）                                     |
| 超长键或内容                        | 拒绝 `memory_write_exceeds_budget`                                 |
| 写库失败                            | 返回存储错误；不得报告成功，也不得把失败压成成功                   |

## 暴露规则

通用能力目录项（`surface=general`），位于目录的 `root` 组。目录声明 `default_enabled_without_skill = false`，且它与「无 Skill 即可用的只读基础面」互斥（确认类动作不进入该基础面）。是否进入某一轮工具面由 `C16` 按授权与上下文决定并记录工具面版本（`K10`）。

在受限子任务中，本工具被明确排除在子 Run 工具面之外：子 Run 从不获得变更类工具，深度不会超过一层，持久副作用始终留在父级确认路径上（`C15`）。

## 源码落点

- 目录定义：[tool_catalog/root.rs](../../src-tauri/src/ai_runtime/tool_catalog/root.rs)。
- 派发实现：[tool_dispatch/memory.rs](../../src-tauri/src/ai_runtime/tool_dispatch/memory.rs) 的 `memory_write_tool`（`clear_memory_scope`、`active_vault_scope`、`requested_scope`；上限常量 `MAX_MEMORY_KEY_CHARS = 200`、`MAX_MEMORY_CONTENT_CHARS = 2_000`）。
- 确认与冻结：[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)（记忆目标表达式与 `rollback_summary`）、[frozen_change_plan.rs](../../src-tauri/src/ai_runtime/frozen_change_plan.rs)。
- 目录契约测试：[tool_catalog/tests.rs](../../src-tauri/src/ai_runtime/tool_catalog/tests.rs)（`memory_write` 需要确认的断言）。

## 当前状态

- 文档成熟度 `draft`（见「维护规则」：[README.md](../README.md) §八）。
- 实现状态 `implementation.state = present`：目录项 `Dispatchable`，派发器有真实处理器与确认门槛。这只描述静态源码事实，不代表合同正确或用户任务可用。
- 产品行为尚未确定：**写入时机、有效期、冲突处理与用户管理**未确定（`Q15`）；当前只确定确认合同与作用域语义。因此本卡片不声明「何时应该写」「写到何时失效」「冲突如何合并」的行为。
- 目标合同要求补齐条目的来源与失效表达（`C09`）；当前存储已带 `source` 与 `updated_at`，但来源与失效的完整表达仍按 `G*`／`Q15` 的范围施工。
- 验证结果：本卡片不声明任何验证结论；本轮为基线 `2670739f` 的静态核对，未执行新实验、未运行付费实网评测。

## 相关合同

本工具消费 `K05`（授权范围与冻结确认）、`K08`（压缩覆盖与长期记忆条目）。相关对象：`M02`（权限与受控副作用）、`C04`（授权与外发策略）、`C05`（变更冻结与确认）、`C06`（确定性副作用执行）、`C09`（长期记忆）、`C16`（工具目录、工具面与技能接入）、`C17`（调用派发与观察）、`C15`（子任务协调）、`K06`（统一预算账本）、`K10`（工具面版本与派发观察）、`N14`、`Q15`。

<!-- iris:end T14 -->
