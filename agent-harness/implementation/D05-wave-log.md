# D05 施工日记

本文件只登记 `D05` 各波执行事实与范围声明，**不是**工作包合同。合同正文见 [D05-document-tasks-and-delivery.md](./D05-document-tasks-and-delivery.md)。

读法：

- **本波范围外**：本波不做，后续波次可以做。
- **跨波锁定**：在本工作包关闭前不得做；改锁定须单独写明，不得用下一波「明确不做」更窄口径偷换。
- 各波条目是压缩执行事实，**不是**合同原文逐字搬迁。细节以对应提交为准：第一波 `9ca06b8c`，第二波 `3a2feb76`，第三波 `8f0f0c5c`，第四波 `59960549`；两栏口径与本摘要说明见 `1f2ae2cf` 及之后提交。「不把 `capability_degraded` 映射为 `partial`」在第四波**跨波锁定**，不是遗漏。

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

## 第五波（2026-09-22，不关闭）

- **已落地**：确认卡片差异预览走按需瞬时 IPC `assistant_run_confirmation_diff`（`session + runId + confirmationId + planHash`，只读、无副作用）。宿主侧复放冻结候选（original vs candidate 全文行 diff）并以 `similar` 计算有界统一 diff（上下文 2 行、≤50 hunks、≤20k 字符、`truncated` 标志）；磁盘漂移、不可读或非编辑操作的目标准确降级为 `previewable: false`。前端 `AssistantConfirmationDiff` 默认折叠、展开懒加载、统一内联 diff，失败静默回退不阻断批准／拒绝。新增稳定错误码 `agent_run_confirmation_plan_hash_mismatch`／`agent_run_confirmation_diff_unavailable`。持久化事件与工具结果保持只投影 `summary` + `targets`；瞬时预览响应不持久化、不进事件、工具结果、审计或日志。输入输出语义变更登记为 `R14`。
- **本波做**：C24 候选差异呈现（收口第四波缺口「确认卡片仍无 Markdown 差异」）。机械 `V03`／`V04` 绑 `d05_diff_preview_tests.rs`，前端交互绑 `V07` `tests/assistant-run-confirmation-diff.test.tsx`。
- **本波范围外**：N23 撤回≠重规划；差异行的 Markdown 渲染（本波为纯文本行）；`taskOutcome` warning 条的交互升格；并排双栏差异。
- **跨波锁定**：不关闭 Q14／N04／D05／C24；不把任何对象标 `verification.passed`；不回头做 D04／G02／G03／Q17；不做 D06、HR-7 live、V05；不加 K15 独立 SQLite 列或 `agent_runs` 新列；不授予 `document.transform`／`note.apply_patch`；涉密域不提供差异预览、不降级安全边界。
- **不能当作关闭证据**：差异可展示不能推出 N04 内容保持已验收或 D05／Q14 已关闭；机械 `V03`／`V04`／`V07` 不替代 V05／V06 语义验收；`R14` 的 `needs-reverification` 复核记录只是待复核登记，不是独立复核完成。

## 十任务战役（2026-09-23，不关闭）

- **已落地**：具名 T01–T10 生产入口战役 `src-tauri/src/ai_runtime/d05_ten_task_campaign_tests.rs`；报告 [docs/eval/results/2026-09-23-d05-ten-task-campaign.md](../../docs/eval/results/2026-09-23-d05-ten-task-campaign.md)。证据层级 `headless_deterministic`（V03／V04）+ 既有确认卡片 V07。T09 在 Approved 后对漂移磁盘执行已确认变更，错误码 `frozen_change_base_hash_drift`，磁盘保持漂移正文。
- **本战役做**：用固定题面把五波机械测试收成任务级复验表。T08 UI 只引用 `tests/assistant-run-confirmation-diff.test.tsx`，不重写前端测。
- **本战役范围外**：不跑 V05 live；不扩 52 题问答矩阵；不写 `product-gate.json`；不把本战役写成 D05 第六波产品施工。
- **跨波锁定**：不关 Q14／N04／D05／D04／C24／C25；不解封 CHG-83／CHG-84；不绑 V02／D05#file；不改 D05 合同正文；不加 SQLite 列、LangChain、品牌 `format:check` 文件。
- **不能当作关闭证据**：十题全绿不能推出 D05／N04／Q14 已验收；`headless_deterministic` 不替代 V05／V06。

## 关条钉死与 2A 前置（2026-09-23，不关闭）

- **已落地**：D05 合同 §三写入关闭清单；K16 窄改 `unknown`／`failed` 分列（`failed` 全批零冻结，`unknown` 进冻结并交付差异，未批准零写入）；`format_gate` 对表／HTML／零宽 `unknown` 进入确认卡；确认卡差异可见前禁止批准、拒绝始终可点。
- **本波做**：钉死关条文面；2A 两处产品门闩；1A 书面批准落到 V05 窗口并本机跑 MiniMax-M3／DeepSeek-Flash 三条 `#[ignore]` live，写 `docs/eval/results/<日期>-v05-*-live.md`。
- **V05 执行**：`live_minimax_m3_adapter_returns_https_citations` 通过。DeepSeek-Flash 两条先跳过后补跑：`live_deepseek_flash_native_search_protocol_probe` 与 `live_deepseek_flash_adapter_returns_https_citations` 均通过。不关 `G02`／`G03`／`Q17`／`D04`。
- **本波范围外**：`N23` 撤回≠重规划；Host `approve_change` IPC 防绕过；Markdown 显式保存改写 `.md`；D06、HR-7 `agent:eval:live`、扩 13 厂商矩阵。
- **跨波锁定**：不关 `Q14`／`N04`／`D05`／`D04`／`C24`／`C25`；不把任何对象标 `verification.passed`；不解封 CHG-83／CHG-84；不绑 V02／D05#file；不回头做 G02／G03／Q17；不做 D06。
- **不能当作关闭证据**：关闭清单本身、`unknown` 可冻结、确认卡门闩、两条适配器 live、Key 缺失跳过。窄窗口 live 不是 V05 全矩阵，也不把 `npm run agent:eval:live` 当 V05。

## N23 证据波（2026-09-23，不关闭）

- **已落地**：`N23` 撤回≠重规划的机械钉三枚，撤回路径行为未改。仓储正例 `n23_retract_deletes_only_the_suffix_and_leaves_the_run_ledger_untouched`：对已完成 Run 的已发布助手消息撤回，只删 `session_messages` 后缀、清摘要、退休 evidence；`agent_runs` 行数、状态、信封与事件不变，无新 accepted／running Run。命令探测 `n23_assistant_session_retract_returns_the_delete_count_and_never_starts_a_run`：`assistant_session_retract` 走 IPC 返回删除计数，`agent_runs`／`agent_run_events`／`session_messages` 无 `RunIntake::start`／`spawn_normal_direct_run` 痕迹。薄前端钉（`tests/use-assistant-conversation.test.tsx`）：`handleRetract` 只 invoke `assistant_session_retract`，不 invoke `assistant_run_start`。负例复用 `session_lifecycle_rejects_delete_and_retract_until_all_runs_are_terminal`。evidence.md 补 `N23` 不变量行后，V02 current 证据按新指纹重绑；治理复核的 V02／D05#file 绑定保持 stale，不改写旧指纹。
- **本波做**：范围仅 `N23` `V03` 证据绑定（`registry.json.verify`）；**明确不关 `D05`**。
- **本波范围外**：`N23` 语义验收（`V05`／`V06`）；新撤回 UI；Host `approve_change` IPC 门闩；Markdown 显式保存改写 `.md`；把撤回改成走 `assistant_run_control`；扩 DiffView；改 SQLite schema。
- **跨波锁定**：不关闭 Q14／N04／D05／D04／C24／C25；不把任何对象标 `verification.passed`；不解封 CHG-83／CHG-84，不绑 V02／D05 文件指纹；不做 D06、HR-7 live、V05 矩阵、`agent:eval:live`。
- **不能当作关闭证据**：撤回≠重规划的机械钉不能推出 `N23`／`D05`／`N04`／`Q14` 已验收；`V03` 与薄前端钉不替代 `V05`／`V06`；不是取消 Run、拒绝确认、同 Run 纠偏续轮或 `C08` 摘要更正语义。

<!-- iris:end FILE-D05-JOURNAL -->

<!-- iris:object FILE-D05-JOURNAL kind=rules -->

### FILE-D05-JOURNAL 正式定义

本对象是 `D05` 的施工日记容器：承接各波执行事实与范围外／跨波锁定声明，不承载工作包合同，也不构成验收。

<!-- iris:end FILE-D05-JOURNAL -->
