# insert_text_at_cursor

<!-- iris:object T15 kind=rules file=true -->

`insert_text_at_cursor` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T15 -->

<!-- iris:object T15 kind=tool name=insert_text_at_cursor owner=M02 -->

## 用途与语义归属

绑定插入点与版本；候选与实际应用分离。语义归属「工具（`M2`／`M8`）」，责任模块 `M02`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.2。

它是**把文本落到文件**的动作，不是生成候选的动作：候选文本由 `C24` 产出并呈现，用户选择应用后本工具才提交写入。模型声明"已经写好了"不构成回执，真实写入以本工具的执行回执为准（讨论第十三节）。

按 `N03`，按目标文件格式与文风生成的内容**由用户插入**——本工具的调用前提是存在绑定了目标与版本的显式应用动作，不是模型自主落盘。

## 参数与消费

| 参数                | 类型                                             | 必填 | 消费事实                                                                                   |
| ------------------- | ------------------------------------------------ | ---- | ------------------------------------------------------------------------------------------ |
| `text`              | string                                           | 是   | 要插入的文本；非字符串或缺失时拒绝 `missing text`                                          |
| `target_path`       | string                                           | 是   | 已授权的 Vault 相对路径；参数缺失时回落到本轮的明确目标，仍缺失则拒绝                      |
| `base_content_hash` | string                                           | 是   | `read_note` 返回的整篇内容哈希；缺失或空串时拒绝                                           |
| `range`             | object，含 `start`、`end`（整数，`minimum = 0`） | 是   | UTF-8 字节位置；**`start` 必须等于 `end`**（插入点），范围非法时拒绝 `invalid patch range` |

目录声明的参数列表、必填性与 `additionalProperties` 约束以 [tool_catalog/write.rs](../../src-tauri/src/ai_runtime/tool_catalog/write.rs) 为准。

「`start` 必须等于 `end`」不只写在 schema 描述里，也由写入前置条件强制：提交路径要求 `current.get(range)` 与原文完全一致，而插入路径的原文为空串，因此非空范围会被判定为冲突（`note_original_conflict`）而不是被静默接受。

## 授权

- `C04` 是唯一授权决定者；运行期准入门槛是能力 `note.apply_patch`。
- 目录声明 `requires_confirmation = true`：写入前宿主冻结待确认的变更集（目标路径、`base_content_hash`、预期写入后哈希、回滚说明），确认后才进入确定性执行。
- 目录声明 `access_level = WriteMarkdown` 只是展示元数据，不得扩大 Run 授权。权限原子映射为 `VaultWritePatch`，风险级别 `Medium`。
- 写入目标不得被模型放宽：目标必须属于用户显式应用动作指定的路径或已消费冻结变更集的路径，否则拒绝 `WriteTargetViolation`。
- Run 效果为 `Draft` 时 `note.apply_patch` 被拒（只能提议补丁、不能落盘）；只有 `Apply` 类 Run 才能取得该能力。

## 副作用

写 `.md` 文件，并连带产生恢复快照与索引写入：

1. 先对当前内容生成恢复快照，快照缺失或校验不符则整体失败（不写入）；
2. 在 Vault 移动锁内按精确字节区间重建内容并写入；
3. 回执含 `before_hash`、`after_hash`、`version_id` 与索引状态；索引降级时返回警告而不是假装完全成功。

它只改目标 `.md` 的指定字节位置，不规范化无关 Markdown 字节、不改其他文件。记忆、计划登记等副作用不由本工具产生。

## 预算

按工具的单一预算分类 `ConfirmedChange` 计账（确认类优先于访问级别回落），受 `max_confirmed_change_calls` 约束（三类预设当前均为 6），并占用总工具调用上限（`max_tool_calls`，当前 24）；不占用本地、网络或运行时额度（`K06`）。

## 幂等

**不是幂等动作，但重复应用会失败而不是重复插入**：写入前置条件要求当前文件内容哈希等于 `base_content_hash`，第一次写入后哈希已变化，同一变更集再次提交返回 `note_content_conflict`。因此重试不会造成重复插入；要再次插入必须重新读取基线并重新确认。

## 取消

取消在两个时点生效：派发前（`ensure_run_active`）与提交前的授权复核（在 Vault 移动锁内再次检查取消与目标、文档能力、Skill 范围）。取消后不开始新的持久副作用；已经完成的写入只报告事实，不假装回滚。

## 失败反馈

| 情形                     | 反馈                                                                              |
| ------------------------ | --------------------------------------------------------------------------------- |
| Run 已取消               | 返回 `Cancelled`，不写入                                                          |
| 未确认                   | 请求确认并中断本轮派发（`CONFIRMATION_PENDING_ERROR`），不执行写入                |
| 确认前目标或内容变化     | 冻结的版本与基线不再匹配时拒绝（`note_content_conflict`），要求重新读取与重新确认 |
| 目标不在冻结范围内       | 拒绝 `WriteTargetViolation`                                                       |
| 范围非法或原文不一致     | 拒绝 `invalid patch range` / `note_original_conflict`，不模糊匹配、不猜测插入点   |
| 内容超限                 | 拒绝 `note_content_too_large`                                                     |
| 恢复快照不可用或校验失败 | 拒绝 `note_recovery_snapshot_missing` / `note_recovery_snapshot_invalid`，不写入  |
| Vault 在提交期间变化     | 拒绝 `note_vault_changed`                                                         |
| 索引降级                 | 写入回执带警告（"文档已写入，但索引待修复"），不把降级报告成完全成功              |

以上错误按可恢复错误处理：返回具体缺口让模型修正或重新读取基线，一次失败不终止任务（`C14`、`N16`）。

## 暴露规则

通用能力目录项（`surface=general`），位于目录的 `write` 组。目录声明 `default_enabled_without_skill = false`，且确认类动作不进入「无 Skill 即可用的只读基础面」。是否进入某一轮工具面由 `C16` 按明确任务、目标与权限决定并记录工具面版本（`K10`）——只加需要的写工具，不开放全部变更能力。

在受限子任务中，本工具被明确排除在子 Run 工具面之外（子 Run 只读）。

## 源码落点

- 目录定义：[tool_catalog/write.rs](../../src-tauri/src/ai_runtime/tool_catalog/write.rs)。
- 派发实现：[tool_dispatch/markdown.rs](../../src-tauri/src/ai_runtime/tool_dispatch/markdown.rs) 的 `markdown_write_patch_apply`（`insert_text_at_cursor` 与 `replace_selection` 共用入口，按工具名取 `text`）。
- 写入原语：[storage/note_operations.rs](../../src-tauri/src/storage/note_operations.rs) 的 `apply_edit`、`edited_content`、`protect_snapshot`。
- 确认与冻结：[frozen_change_plan.rs](../../src-tauri/src/ai_runtime/frozen_change_plan.rs)（两个编辑工具必须提供基线哈希与预期写入后哈希）、[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)（`frozen_base_content_hashes`、`expected_post_content_hashes`）。
- 审计脱敏：[tool_audit.rs](../../src-tauri/src/ai_runtime/tool_audit.rs)（只记录文本长度与哈希，不记录文本本身）。
- 契约测试：[tool_dispatch/write_contract_tests.rs](../../src-tauri/src/ai_runtime/tool_dispatch/write_contract_tests.rs)。

## 当前状态

- 文档成熟度 `draft`（见「维护规则」：[README.md](../README.md) §八）。
- 实现状态 `implementation.state = present`：目录项 `Dispatchable`，派发器有真实写入实现与确认门槛。这只描述静态源码事实，不代表合同正确或用户任务可用。
- 与目标合同的**待核对**项：
  1. 目标合同要求"候选与实际应用分离"，`C24` 的候选合同仍需补齐统一收口（架构 §10.1 把编辑候选交接列为复用现有绑定、补统一候选合同的项）；候选呈现与确认身份是否覆盖所有 UI 入口未在本卡片证据中确认。
  2. 确认身份与有效期由 `C05` 拥有，本卡片不复制其定义；冻结变更集的有效期数值属受控配置（运行中的变更确认有界，具体数值不在此新设）。
- 验证结果：本卡片不声明任何验证结论；本轮为基线 `2670739f` 的静态核对，未执行新实验、未运行付费实网评测。

## 相关合同

本工具消费 `K05`（授权范围与冻结确认）、`K14`（证据身份、来源与支持关系，用于写入回执与来源表达）。相关对象：`M02`（权限与受控副作用）、`C05`（变更冻结与确认）、`C06`（确定性副作用执行）、`M08`（验证与交付）、`C24`（编辑候选与差异）、`C23`（任务验证）、`C16`（工具目录、工具面与技能接入）、`C17`（调用派发与观察）、`K10`（工具面版本与派发观察）、`K16`（验证报告与可恢复错误反馈）、`N02`、`N03`。

<!-- iris:end T15 -->
