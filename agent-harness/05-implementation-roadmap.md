# 05. Harness Recovery 实施路线图

> **文档状态**：现行
> **文档类型**：实施路线
> **事实基线**：2026-08-27，审计提交 `6c5dbd40`

本路线只规定 Harness 内部依赖、删除项和退出条件，不新增产品版本承诺。每个代码阶段开始前必须单独生成测试先行实施计划；未满足退出条件不得提前开始依赖它的阶段。

## 1. 历史状态重置

此前 AH-0 的跨平台 deterministic 基线和 AH-1 的单一文档入口仍有可复用价值，但此前 AH-2/AH-3 曾经写成已验证终局的判断已被生产缺陷推翻。新阶段不复用旧编号：

| 历史内容                   | 当前处置 | 说明                                     |
| -------------------------- | -------- | ---------------------------------------- |
| Run、跨平台 fixture 基础   | 保留     | 继续作为管线与安全测试基础               |
| 单一 `agent-harness/` 入口 | 保留     | 原位重写，不建立第二套文档               |
| Web 专用自适应研究骨架     | 重构     | 并入通用 AgentToolLoop                   |
| 领域 operation 核心路由    | 退出核心 | 只保留兼容读取和真实需求驱动的可选适配器 |
| 普通事实严格结构化终局     | 重构     | 仅严格任务使用                           |

## 2. 阶段总览

| 阶段 | 状态             | 目标                                 | 同阶段必须删除或降级                         |
| ---- | ---------------- | ------------------------------------ | -------------------------------------------- |
| HR-0 | 已验收           | 重置文档事实和路线                   | 旧现行文件名、旧完成结论、ROADMAP 冲突       |
| HR-1 | 基线已验收       | 建立能复现生产问题的通用行为基线     | ID 偶合、理想调用顺序和只验管线的假阳性      |
| HR-2 | 已验收           | Intake 去领域化并恢复渐进联网        | 所有非排除事实严格联网、新 Run 领域前置阻断  |
| HR-3 | 已验收           | 统一自适应工具循环和分类预算         | Fresh research 平行预算、闭集 gap 与专用停机 |
| HR-4 | 已验收           | 自然澄清、普通最终化、错误和 UI 投影 | 新普通 Run 的 AwaitingInput、普通强制终局    |
| HR-5 | 已验收           | 有界冻结变更集与确认后只读验证       | 单 operation 限制、确认后零验证              |
| HR-6 | 已验收           | 领域 operation 退出核心并净删除      | classifier、默认表面、专用门禁和失效测试     |
| HR-7 | 已验收（确定性） | 通用质量评测与 Provider 能力校准     | 按单一模型/领域编排的成功假设；真实试点结论  |

## 3. HR-0：文档事实重置（已验收）

实施：

- 原位重写唯一现行文档，统一状态头、基线日期和审计提交。
- 将研究合同改为通用 Agent 循环合同，将领域矩阵改为任务/能力/风险矩阵。
- 同步 `ROADMAP.md`、`docs/README.md` 和文档事实检查。
- 明确 `ARCHITECTURE.md` 仍只描述当前代码，不写入未实现目标。

退出条件：

- 新旧文件不并存，所有现行链接有效；
- 文档明确区分当前事实、目标合同和实施路线；
- `npm run docs:check`、`npm run format:check`、`git diff --check` 通过；
- 完整 diff 人工复核后才能把 HR-0 改为已验收。

2026-08-27 验收结果：现行文件已完成原位重写和两项受控重命名，ROADMAP 与 docs 索引已同步；文档事实检查会拒绝旧文件、平行根目录、缺少状态头、断链和被撤回方向重新成为现行结论。验证命令与最终提交见本轮 Git 记录。

## 4. HR-1：生产问题与通用行为基线

已先补失败回归，未改变生产路由：

- 普通事实与推荐不应自动变成严格 WebRequired；
- 首轮搜索结果差时模型应调整查询并取得新资源；
- 两轮无进展时应强制综合，而不是直接红色失败；
- 本地搜索后可多次读取相关笔记完成多跳回答；
- 普通正文存在时不得因非严格来源协议被改为失败；
- 普通缺参可自然追问；
- 高位 evidence ID、跨 Run `W1` 和不同 Provider tool-call 形态不发生偶合；
- 已确认写入仍保持单次、不可越权，作为 HR-5 前的安全基线。

2026-08-27 基线验收记录：

- HR-1 的普通 Web、普通缺参和普通最终化 `#[should_panic]` 目标夹具已分别在 HR-2/HR-4 替换为绿色回归；当前证据是 `ordinary_external_questions_use_webpreferred_without_strict_finalization`、`ordinary_clarification_completes_and_next_run_receives_conversation_context` 与 `hr1_ordinary_research_reply_uses_natural_source_group_finalization`。HR-3 的旧无进展夹具已由通用绿色回归替代并删除，避免继续固化被撤回的失败语义。
- `hr1_adaptive_search_accepts_a_refined_query_with_a_new_resource`、`hr1_local_multi_hop_reads_distinct_notes_without_web_access`、`hr1_repeated_failed_tool_call_stops_after_two_real_executions`、`hr1_same_session_runs_keep_w1_bound_to_their_own_evidence` 与恢复投影 Vitest 记录现有可保留的通用边界。
- `hr1_current_recommendation_quality_fixture_requires_status_scope_and_bound_sources` 仅证明确定性评测可以拒绝遗漏状态、范围或来源绑定的观察；它不等于真实 Provider 的回答质量。
- `frozen_confirmation_is_bound_to_its_run_hash_and_single_consumption` 保留为旧计划兼容与重复确认的安全线；多操作和写后验证由 HR-5 的独立回归覆盖。

退出条件（已满足）：每个生产问题都有先红后绿所需的独立测试夹具，且 fixture 不预编排唯一正确查询或答案正文。HR-1 的“已验收”只表示基线可复现；生产行为仍由后续 HR-2 至 HR-5 改造。

## 5. HR-2：Intake 去领域化

实现方向：

- 使用现有 `AgentIntent`、Effect、ContextMode、Freshness、Effort、RiskClass 和 CapabilityId 决定 envelope。
- 普通外部事实默认 WebPreferred；明确联网/URL/强时效/高风险才 WebRequired。
- FreshFactDomain 和 DomainOperation 不再决定权限、工具面、澄清或完成门禁。
- 领域旧字段保留反序列化和恢复读取；新 envelope 不再写入领域规划事实。

同步删除：所有“未命中排除项即 StrictExternalFact”的终局分支，以及新 Run 写入/冻结领域 operation、领域地点输入和 `web.domain.read` 的路径。旧领域执行分支只服务于已持久化的旧 envelope，HR-6 再净删除其实现与测试。

2026-08-28 验收结果：

- 新 Normal Run 统一写入 `FreshFactPolicy::default()`，不再写入 `FreshFactDomain`、`DomainOperation`、日期窗口、地点要求或 `web.domain.read`；旧 envelope 保持反序列化和执行恢复兼容。
- `ExclusionClassifier` 已收敛为任务/风险路由：普通外部请求为 `WebPreferred`，显式 Web/URL、强时效和高风险当前事实为 `WebRequired`；Web 关闭不会由分类器、模型、会话或历史记录重新授权。
- Chat、AskNotes、Research、CitationCheck、Draft、Apply 已有表驱动 Intake 回归；高位 ledger、通用 Web、无工具模型的严格失败和旧领域 envelope 恢复均有生产编排回归。
- 严格任务仍使用现有结构化最终化；普通 `WebPreferred` 的自然正文最终化明确留给 HR-4，不能以本阶段的工具连通性冒充回答可用性。

退出条件（已满足）：Chat、AskNotes、Research、CitationCheck、Draft、Apply 的表驱动 intake 测试覆盖 Offline/WebPreferred/WebRequired，并证明 Web 开关不能被模型或分类器增权。

## 6. HR-3：统一自适应工具循环（已验收）

实现方向：

- `RunBudgetPolicy` 升级为唯一总量与分类预算事实源，兼容读取旧 schema。
- catalog 现有 `cost_class` 扩展为 local、network、external_read、runtime 和 confirmed_change。
- 通用循环根据 resource ID、canonical URL、内容 hash 和 revision 计算进展。
- 连续两轮无进展、探索预算耗尽或只余最终模型回合时关闭工具并保留最后一次综合。
- Web、本地和外部只读工具共享循环，但继续保持各自安全与内容边界。

同步删除：`FreshResearchPlan` 生产路由、闭集 `EvidenceGap`、Web evidence 专用停机、重复 deadline 和 search/fetch/repair 平行计数。

2026-08-28 验收结果：

- `RunBudgetPolicy` 已在 HR-5 升级到 schema 3；新 Run 冻结 local 12、network 6、external_read 6、runtime 4、confirmed_change 6。历史 schema 只能按其当时的精确安全形状材料化，不能因升级得到确认后验证额度；篡改或未知 schema 仍失败关闭。
- `AgentToolLoop` 以 catalog 的单一工具类别计数，成功 fingerprint 不重复、相同失败至多两次；两次完整工具回合没有新增安全 identity、总工具额度耗尽，或只余最后一次模型回合时关闭业务工具面并保留最后一次综合。
- `FreshResearchPlan`、`EvidenceGap`、研究 checkpoint、search/fetch/repair 平行计数和 Web 专用循环 deadline 已从生产路径删除。Web、local、runtime 与外部读取均通过同一个 ToolLoop；单次 Web 请求仍保留自身超时、结果尺寸、URL 归属、权限和 evidence 安全边界。
- 新的 `WebRequired` 使用通用模型—工具循环；历史非空领域 envelope 只保留兼容读取，活跃旧 Run 会在 Provider 调度前安全终态化，不能成为新 Run 的第二规划器。

退出条件（已满足）：差结果改写查询、本地多跳、重复抑制、分类上限、取消、总预算和无进展强制综合均由同一 `AgentToolLoop` 回归覆盖；真实 Provider 表现仍留给 HR-7。

## 7. HR-4：回答、澄清、错误与投影

2026-08-30 验收结果：

- `requires_structured_finalization` 只为 `HighStakesCurrentFact` 暴露既有 `submit_final_answer`；普通 `WebRequired` 保持当前 Run evidence gate，并以自然正文与受控来源组完成。
- 新普通缺参在未调用工具时只能以一条短、无来源的自然问题完成；已有工具调用、正文、URL 或来源标记仍不能绕过 Web/external evidence 门禁。下一用户消息创建新 Run，由会话历史承接，不重放旧 Run。
- `useAssistantConversationProjection` 继续是唯一 Run-to-message writer：终态以同 Run durable 正文收敛，历史 user-only Run 可补建一条 assistant 行，无同 Run user 行的迟到事件仍被忽略。
- `FinalizationProtocolInvalid` 稳定错误码保留在事件诊断；用户只看到“本次回答未完成必要的来源校验，请重试”。证据不足、能力不可用、回答未完成和材料无效保持不同产品语义。

未新增 input transaction、领域补充卡、来源解释器、Provider 分支或新的 UI state store；旧 `AwaitingInput`/`submit_input` 仅兼容读取与恢复。

退出条件（已满足）：自然追问与下一轮承接由 `ordinary_clarification_completes_and_next_run_receives_conversation_context` 覆盖；普通严格联网自然来源组、高风险结构化边界、高 ledger ID、终态正文收敛和迟到隔离分别由 `hr1_ordinary_research_reply_uses_natural_source_group_finalization`、`high_stakes_current_fact_keeps_structured_finalization_tool`、`production_news_web_fallback_uses_natural_source_group_with_high_ledger_ids_and_recovers`、`tests/use-assistant-run-transcript.test.tsx` 覆盖。真实 Provider 质量仍留给 HR-7。

## 8. HR-5：冻结变更集

2026-08-30 验收结果：

- `FrozenChangePlan` 保留 legacy 计划读取，同时以 schema v2 冻结 1 至 6 个有序操作、至多 6 个路径；每项携带独立 tool call、参数、base/expected hash 与回滚摘要，重复路径必须形成 hash 链。
- 一次确认只消费整套计划。确认后的执行仅接受计划中的路径，逐项重验授权与 base hash；第二项漂移时已完成前缀被保留，后缀不调度，Run 以明确的部分执行报告完成。
- `DurableApplyCheckpoint` schema v2 记录无正文的操作游标；`Approved → Dispatching → Applied` 可逐项推进，重启时只恢复未执行后缀；已经处于整套 expected 状态的旧检查点安全终态化而不重放。
- 新 Durable Run 的确认后验证预算为最多 2 次模型和 4 次 `read_note`；工具面与实际调度同时限制为冻结目标，Web、external、runtime 和任何写入都不可用。旧 Run 物化为 `0/0`，不因升级增权；路由或模型不可用时保留 Host 的执行事实报告。

同步删除：新的确认路径不再把多个 confirmation call 拆成多个确认事务；单操作 persisted plan、schema 1 checkpoint 与旧预算只保留兼容读取，不能成为新执行语义。

退出条件（已满足）：`normal_executor_freezes_one_ordered_confirmation_batch_and_rejects_a_seventh_call` 覆盖真实执行器的一次确认与 6 项上限；`failed_confirmation_batch_closes_started_tool_lifecycle_without_pending_plan` 覆盖确认建立失败后的生命周期闭合；`confirmed_change_set_applies_two_ordered_operations_once_and_completes_its_cursor` 覆盖两目标顺序成功；`confirmed_change_set_reports_partial_completion_when_second_target_drifted` 覆盖第二项漂移和部分报告；`startup_recovery_resumes_only_the_unapplied_suffix_of_a_consumed_change_set` 覆盖重启前缀恢复；`frozen_confirmation_is_bound_to_its_run_hash_and_single_consumption` 覆盖重复确认；`post_confirmation_loop_allows_only_four_local_calls_and_reserves_the_second_turn` 与 `post_confirmation_verification_rejects_other_targets_and_non_read_tools` 分别覆盖 2/4 预算和验证越权。不新增数据库表、迁移、IPC 或 Provider。

## 9. HR-6：领域核心退役

实施：

- 从 Intake、默认 tool surface、dispatcher 选择和 finalization 移除五类领域分支。
- 保留 migration 072、旧 envelope 和旧 provider mapping 的只读兼容。
- 有真实 Provider 的结构化能力通过统一 catalog/MCP snapshot/capability 接入，不创建领域状态机。
- 删除不再可达的 DTO renderer、fixture、helper、测试和文档；若某适配器仍有真实调用证据，单独记录保留理由。

2026-08-31 验收结果：

- `fresh_domains/`、五个领域 catalog/dispatch 分支、专用地点与 provider renderer、`current_fact_finalization`、Host 预取和领域 fixture 已删除；新 Run 的 WebRequired 统一由模型先请求 `web_search` 再进入既有 `AgentToolLoop`。
- `FreshFactDomain`、`DomainOperation`、migration 072 和旧 MCP snapshot 只保留反序列化/读取兼容。含旧领域标记的活跃 Run 在 Provider 调度前明确终态化，绝不自动重放事实查询或副作用；设置层同样拒绝写入新的领域 binding。
- `retired_current_fact_domain_tools_are_not_agent_visible`、`legacy_current_fact_run_is_terminalized_without_provider_replay`、`new_domain_binding_input_is_rejected_before_it_can_recreate_retired_routing` 与静态扫描共同证明新核心没有第二领域路由。该阶段净删除约 6,600 行，不新增表、迁移、IPC 或 Provider。

退出条件（已满足）：新 Run 的权限、工具面、dispatch、finalization 和评测模拟均不以 `FreshFactDomain`/`DomainOperation` 决策；旧数据只能安全读取或终态化，不能回流为新写入。

## 10. HR-7：质量与 Provider 校准

- 建立普通对话、Web 自适应、本地多跳、混合材料、严格事实、无工具 Provider 和文档修改任务矩阵。
- 确定性 WebRequired fixture 必须和生产一样由模型先请求 `web_search`，再基于同 Run 的工具结果综合；不得把 Host 预取当作通用模型能力证据。
- 固定夹具验证行为和安全，不能冒充真实答案质量。
- 至少使用两个协议形态不同的 mock Gateway，禁止针对 MiniMax 调整核心。
- 真实 Provider 试点保持另行授权，记录模型、配置 hash、成本 checkpoint、p50/p95、token 和匿名 verdict，不保存正文、查询或 URL。

2026-08-31 确定性验收结果：

- 现有 `agent_capacity_eval` 被修正为与生产路径一致：WebRequired 场景先产生 `web_search` 工具调用、再由同一 Run 的工具结果触发综合，不再把 Host 预取误当成模型能力。
- 24 个通用场景的 deterministic matrix 已 24/24 完成、24/24 通过；其中覆盖普通回答、Web 自适应、本地多跳、混合材料、严格当前事实、无工具能力降级与有界文档修改。
- `tool_call_25_blocked` 改用 catalog 实际分类（12 本地 + 6 网络 + 6 外部只读）验证总预算：第 24 次可执行，第 25 次不执行且仍保留最终综合。八项硬边界均重复五次通过。
- OpenAI-compatible 与 Anthropic Messages 的 mock Gateway 都验证了真实工具调用和工具结果续接形态；chat-only 能力继续以显式降级而非伪造工具续接处理。核心未按 MiniMax 或模型名分叉。

退出条件（确定性部分已满足）：通用质量、安全、硬边界和多协议 mock 都有命名回归，且 `npm run agent:eval:smoke` 通过。真实 Provider 试点未经本轮授权，故不产生任何生产模型质量、延迟或成本结论；该限制不是待修复的代码缺口。

## 11. 阶段纪律

- 每阶段开始前创建独立、决策完整的测试先行实施计划。
- 每阶段提交必须列出保留、重构、删除和兼容读取项。
- 新抽象未替代并删除旧分支时只能标为部分实现。
- migration、公共 IPC 或持久化 JSON 变化必须提供兼容读取和回滚边界。
- 默认不新增依赖、数据库表、Provider 或模型专用分支。
