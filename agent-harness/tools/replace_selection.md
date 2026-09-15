# replace_selection

<!-- iris:object T16 kind=rules file=true -->

`replace_selection` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T16 -->

<!-- iris:object T16 kind=tool name=replace_selection owner=M02 -->

## 用途与语义归属

绑定原文范围与版本；应用前确认。语义归属「工具（`M2`／`M8`）」，责任模块 `M02`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §6.2。

它是**把用户已选中的原文范围替换掉**的动作，不是生成候选的动作：候选文本由 `C24` 产出，用户选择应用后本工具才提交写入。与 `insert_text_at_cursor` 的分工是「范围替换」对「点插入」：前者必须给出范围内的原文，后者要求空范围。

按 `N02`，润色／扩写／缩写的对象可以是字词、单句或一段，因此替换范围以字节区间精确表达，不做模糊匹配。

## 参数与消费

| 参数                | 类型                                             | 必填 | 消费事实                                                                            |
| ------------------- | ------------------------------------------------ | ---- | ----------------------------------------------------------------------------------- |
| `replacement`       | string                                           | 是   | 替换文本；非字符串或缺失时拒绝 `missing replacement`                                |
| `target_path`       | string                                           | 是   | 已授权的 Vault 相对路径；参数缺失时回落到本轮的明确目标，仍缺失则拒绝               |
| `base_content_hash` | string                                           | 是   | `read_note` 返回的整篇内容哈希；缺失时拒绝                                          |
| `range`             | object，含 `start`、`end`（整数，`minimum = 0`） | 是   | UTF-8 字节范围，`end` 不包含在内；范围非法时拒绝 `invalid patch range`              |
| `original_text`     | string                                           | 是   | 字节范围内**完全一致**的原文；不一致时拒绝 `note_original_conflict`，不允许模糊替换 |

目录声明的参数列表、必填性与 `additionalProperties` 约束以 [tool_catalog/write.rs](../../src-tauri/src/ai_runtime/tool_catalog/write.rs) 为准。`original_text` 是必填项：写入前置条件要求 `current.get(range)` 与原文完全一致，这是「不允许模糊替换」的执行事实依据；`selection` 只是兼容性回落键，不是替代参数。

## 授权

- `C04` 是唯一授权决定者；运行期准入门槛是能力 `note.apply_patch`。
- 目录声明 `requires_confirmation = true`：写入前宿主冻结待确认的变更集（目标路径、`base_content_hash`、预期写入后哈希、回滚说明），确认绑定目标、内容、范围与版本，确认后才进入确定性执行。
- 目录声明 `access_level = WriteMarkdown` 只是展示元数据，不得扩大 Run 授权。权限原子映射为 `VaultWritePatch`，风险级别 `Medium`。
- 写入目标不得被模型放宽：目标必须属于用户显式应用动作指定的路径或已消费冻结变更集的路径，否则拒绝 `WriteTargetViolation`。
- Run 效果为 `Draft` 时 `note.apply_patch` 被拒；只有 `Apply` 类 Run 才能取得该能力。已有有效授权不反复索要确认（`K05`）。

## 副作用

写 `.md` 文件，并连带产生恢复快照与索引写入：

1. 先对当前内容生成恢复快照，快照缺失或校验不符则整体失败（不写入）；
2. 在 Vault 移动锁内按精确字节区间重建内容并写入；
3. 回执含 `before_hash`、`after_hash`、`version_id` 与索引状态；索引降级时返回警告。

它只替换目标范围内字节，不改范围之外的内容。语义层面"替换后正文信息与顺序是否保持"不由本工具判定，属于 `C23` 的验证合同。

## 预算

按工具的单一预算分类 `ConfirmedChange` 计账，受 `max_confirmed_change_calls` 约束（三类预设当前均为 6），并占用总工具调用上限（`max_tool_calls`，当前 24）；不占用本地、网络或运行时额度（`K06`）。一次模型响应里混有确认类与非确认类调用时整批被拒绝，不允许借批量规避额度或确认。

## 幂等

**不是幂等动作，但重复应用会失败而不是二次替换**：写入前置条件要求当前内容哈希等于 `base_content_hash`，且范围内的原文与 `original_text` 完全一致；第一次写入后两者都不再成立，同一变更集再次提交返回 `note_content_conflict` 或 `note_original_conflict`。因此重试不会造成重复替换，要再次替换必须重新读取基线并重新确认。

## 取消

取消在两个时点生效：派发前（`ensure_run_active`）与提交前的授权复核（在 Vault 移动锁内再次检查取消、目标、文档能力与 Skill 范围）。取消后不开始新的持久副作用；已经完成的写入只报告事实，不假装回滚。

## 失败反馈

| 情形                               | 反馈                                                                              |
| ---------------------------------- | --------------------------------------------------------------------------------- |
| Run 已取消                         | 返回 `Cancelled`，不写入                                                          |
| 未确认                             | 请求确认并中断本轮派发（`CONFIRMATION_PENDING_ERROR`），不执行写入                |
| 确认前目标或内容变化               | 冻结版本与当前状态不匹配时拒绝（`note_content_conflict`），要求重新读取与重新确认 |
| 目标不在冻结范围内                 | 拒绝 `WriteTargetViolation`                                                       |
| 原文与范围不一致                   | 拒绝 `note_original_conflict`，不模糊替换、不猜测范围                             |
| 范围非法（非字符边界、反向区间等） | 拒绝 `invalid patch range`                                                        |
| 内容超限                           | 拒绝 `note_content_too_large`                                                     |
| 恢复快照不可用或校验失败           | 拒绝 `note_recovery_snapshot_missing` / `note_recovery_snapshot_invalid`，不写入  |
| Vault 在提交期间变化               | 拒绝 `note_vault_changed`                                                         |
| 索引降级                           | 写入回执带警告，不把降级报告成完全成功                                            |

以上错误按可恢复错误处理：返回具体缺口让模型修正或重新读取基线，一次失败不终止任务（`C14`、`N16`）。

## 暴露规则

通用能力目录项（`surface=general`），位于目录的 `write` 组。目录声明 `default_enabled_without_skill = false`，确认类动作不进入「无 Skill 即可用的只读基础面」。是否进入某一轮工具面由 `C16` 按明确任务、目标与权限决定并记录工具面版本（`K10`）。

在受限子任务中，本工具被明确排除在子 Run 工具面之外（子 Run 只读）。

## 源码落点

- 目录定义：[tool_catalog/write.rs](../../src-tauri/src/ai_runtime/tool_catalog/write.rs)。
- 派发实现：[tool_dispatch/markdown.rs](../../src-tauri/src/ai_runtime/tool_dispatch/markdown.rs) 的 `markdown_write_patch_apply`（按工具名取 `replacement`）。
- 写入原语：[storage/note_operations.rs](../../src-tauri/src/storage/note_operations.rs) 的 `apply_edit`、`edited_content`、`protect_snapshot`。
- 确认与冻结：[frozen_change_plan.rs](../../src-tauri/src/ai_runtime/frozen_change_plan.rs)、[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs)。
- 审计脱敏：[tool_audit.rs](../../src-tauri/src/ai_runtime/tool_audit.rs)。
- 契约测试：[tool_dispatch/write_contract_tests.rs](../../src-tauri/src/ai_runtime/tool_dispatch/write_contract_tests.rs)（工具名与替换范围断言）。

## 当前状态

- 文档成熟度 `draft`（见「维护规则」：[README.md](../README.md) §八）。
- 实现状态 `implementation.state = present`：目录项 `Dispatchable`，派发器有真实写入实现与确认门槛。这只描述静态源码事实，不代表合同正确或用户任务可用。
- 与目标合同的**待核对**项：
  1. 「应用前确认」的确认身份由 `C05` 拥有；本卡片只声明确认是执行前提，不复制其定义。确认的有效期数值属受控配置。
  2. 目标合同要求内容保持验证（`C23`）与候选统一收口（`C24`）；本工具只按精确范围替换，不自行判定"正文信息与顺序保持不变"。纯正则空白整理器或写入回执都不等于内容保持已验证。
  3. `selection` 兼容回落键与必填 `original_text` 并存的口径已在参数节说明；是否有其他调用方仍依赖该回落键，未在本卡片证据中确认。
- 验证结果：本卡片不声明任何验证结论；本轮为基线 `2670739f` 的静态核对，未执行新实验、未运行付费实网评测。

## 相关合同

本工具消费 `K05`（授权范围与冻结确认）、`K14`（证据身份、来源与支持关系，用于写入回执与来源表达）。相关对象：`M02`（权限与受控副作用）、`C05`（变更冻结与确认）、`C06`（确定性副作用执行）、`M08`（验证与交付）、`C24`（编辑候选与差异）、`C23`（任务验证）、`C16`（工具目录、工具面与技能接入）、`C17`（调用派发与观察）、`K10`（工具面版本与派发观察）、`K16`（验证报告与可恢复错误反馈）、`N02`、`N04`。

<!-- iris:end T16 -->
