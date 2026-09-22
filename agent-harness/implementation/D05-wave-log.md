# D05 施工日记

本文件只登记 `D05` 各波执行事实与范围声明，**不是**工作包合同。合同正文见 [D05-document-tasks-and-delivery.md](./D05-document-tasks-and-delivery.md)。

读法：

- **本波范围外**：本波不做，后续波次可以做。
- **跨波锁定**：在本工作包关闭前不得做；改锁定须单独写明，不得用下一波「明确不做」更窄口径偷换。

<!-- iris:object FILE-D05-JOURNAL kind=rules file=true -->

## 第一波（2026-09-21，不关闭）

- **已落地**：`WebDecisionReason::LocalTransformation` 与 `is_local_transformation_request` 接到 `ExclusionClassifier::resolve`。`instruction：body` 剥离后仍用未剥离全文判定显式核实／高风险。`web.search` 仍只在 `web_enabled && freshness != Offline` 时进入信封。Durable Apply 派发前把 checkpoint 写成 `Dispatching`。
- **本波做**：纯润色／格式整理在联网开关打开时也不进入 `web.search`（Q14 触发面）。`Dispatching` 先核验磁盘哈希：已是 `expected_post` 则跳过写入、仍是 base 则派发一次、两者都不是则失败关闭且不重放后缀。机械 `V03`／`V04` 绑 `d05_document_task_tests.rs`。
- **本波范围外**：N04 正文／顺序／链接目标程序化核对器；把 `doc_normalize_markdown` 升格为内容保持；K15 任务结果字段；C24 独立候选类型／UI；N23 扩 scope。
- **跨波锁定**：不把 D05／Q14 标 closed 或 `verification.passed`；不回头做 D04／G02／G03／Q17；不做 D06、HR-7 live、Gemini 适配器、V05。
- **不能当作关闭证据**：斜杠命令 `useInlineAi.ts` 的 `webEnabled: false` 只是入口钉，不能单独关闭 Q14。

## 第二波（2026-09-21，不关闭）

- **已落地**：独立纯函数 `check_format_preservation`／`is_format_preservation_request`。生产入口在单工具确认与变更集冻结前拦截；错误码 `format_preservation_unproven`。T25 仅作空白对照，不是 N04 证明。
- **本波做**：格式整理候选写入确认前核对正文信息、块顺序、链接目标。未证明则把缺口回给模型，不冻结确认。机械 `V03`／`V04` 绑 `d05_content_preservation_tests.rs`。
- **本波范围外**：C24 独立候选类型／UI；K15 任务结果字段；把 T25 升格为内容保持；完整 CommonMark AST；N23。
- **跨波锁定**：不把 D05／Q14／N04 标 closed 或 `verification.passed`；不回头做 D04／G02／G03／Q17；不做 D06、HR-7 live、Gemini、V05。
- **不能当作关闭证据**：核对器通过空白样例不能推出 N04／D05 已验收；润色／翻译不走本门。

## 第三波（2026-09-22，不关闭）

- **已落地**：Host 侧 `EditCandidate`／`prepare_edit_candidate`。确认事件是 `summary` + `targets`，不含笔记正文。
- **本波做**：`replace_selection`／`insert_text_at_cursor` 进入冻结前成为可独立复验的 C24 候选。机械 `V03`／`V04` 绑 `d05_edit_candidate_tests.rs`。
- **本波范围外**：C24 独立候选 UI／DiffView；K15 任务结果字段；N23 撤回≠重规划。
- **跨波锁定**：不关闭 Q14；不把 D05／N04／C24 标 `verification.passed`；不回头做双路；不授予本波未要求的 `document.transform`；不做 V05／D04／D06。
- **不能当作关闭证据**：生成≠写盘不能推出 N04／Q14／D05 已验收；Draft 信封仍没有 `note.apply_patch`。

## 第四波（2026-09-22，不关闭）

- **已落地**：`classify_task_outcome` 派生 `completed`／`partial`／`blocked`，写入 `Completed` 事件 JSON 的可选 `taskOutcome`。历史事件缺字段反序列化为空。
- **本波做**：Host 限制说明保持 Run `Completed`、任务结果 `blocked`；部分写入为 `partial`。界面只对 `partial`／`blocked` 出 warning 条。机械 `V03`／`V04` 绑 `d05_delivery_outcome_tests.rs`。
- **本波范围外**：DiffView／新候选 UI。
- **跨波锁定**：不加 K15 独立 SQLite 列或 `agent_runs` 新列；不把 `capability_degraded` 映射为 `partial`；不关闭 Q14；不把 D05／C25／K15／N04 标 `verification.passed`；不做 V05／D04／D06。
- **不能当作关闭证据**：事件上有 `taskOutcome` 不能推出 N04／Q14／D05 已验收；确认卡片仍无 Markdown 差异。

## 修复复核（2026-09-22，不变更验收登记）

- 格式保持门按「参数／权限 → 整文 hash → 虚拟整文候选 → 整文保持 → 冻结」执行。
- 变更集拒绝使用带类型的 `Frozen`／`Rejected` 结果；全批在冻结前拒绝。
- `Resume` 后 worker 共用边界；当前前缀 hash 装配后只执行后缀。
- 选区仅精确映射已执行编辑。定向回归不替代 V05、语义验收或本工作包关闭证据。

<!-- iris:end FILE-D05-JOURNAL -->

<!-- iris:object FILE-D05-JOURNAL kind=rules -->

### FILE-D05-JOURNAL 正式定义

本对象是 `D05` 的施工日记容器：承接各波执行事实与范围外／跨波锁定声明，不承载工作包合同，也不构成验收。

<!-- iris:end FILE-D05-JOURNAL -->
