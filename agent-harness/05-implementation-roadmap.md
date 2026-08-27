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

| 阶段 | 状态       | 目标                                 | 同阶段必须删除或降级                         |
| ---- | ---------- | ------------------------------------ | -------------------------------------------- |
| HR-0 | 已验收     | 重置文档事实和路线                   | 旧现行文件名、旧完成结论、ROADMAP 冲突       |
| HR-1 | 基线已验收 | 建立能复现生产问题的通用行为基线     | ID 偶合、理想调用顺序和只验管线的假阳性      |
| HR-2 | 未开始     | Intake 去领域化并恢复渐进联网        | 所有非排除事实严格联网、领域前置阻断         |
| HR-3 | 未开始     | 统一自适应工具循环和分类预算         | Fresh research 平行预算、闭集 gap 与专用停机 |
| HR-4 | 未开始     | 自然澄清、普通最终化、错误和 UI 投影 | 新普通 Run 的 AwaitingInput、普通强制终局    |
| HR-5 | 未开始     | 有界冻结变更集与确认后只读验证       | 单 operation 限制、确认后零验证              |
| HR-6 | 未开始     | 领域 operation 退出核心并净删除      | classifier、默认表面、专用门禁和失效测试     |
| HR-7 | 未开始     | 通用质量评测与 Provider 能力校准     | 按单一模型/领域编排的成功假设                |

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

- `hr1_ordinary_external_questions_still_record_the_webpreferred_gap`、`hr1_no_progress_still_fails_before_the_reserved_final_synthesis`、`hr1_ordinary_missing_context_still_pauses_the_run_for_structured_input` 与 `hr1_ordinary_research_reply_still_requires_structured_finalization` 是受控 `#[should_panic]` 目标夹具；分别由 HR-2、HR-3、HR-4 反转为普通绿色回归，不能被解读为当前产品能力。
- `hr1_adaptive_search_accepts_a_refined_query_with_a_new_resource`、`hr1_local_multi_hop_reads_distinct_notes_without_web_access`、`hr1_repeated_failed_tool_call_stops_after_two_real_executions`、`hr1_same_session_runs_keep_w1_bound_to_their_own_evidence` 与恢复投影 Vitest 记录现有可保留的通用边界。
- `hr1_current_recommendation_quality_fixture_requires_status_scope_and_bound_sources` 仅证明确定性评测可以拒绝遗漏状态、范围或来源绑定的观察；它不等于真实 Provider 的回答质量。
- `frozen_confirmation_is_bound_to_its_run_hash_and_single_consumption` 继续限定 HR-5 前的单操作确认安全线，不提前实现多操作或写后验证。

退出条件（已满足）：每个生产问题都有先红后绿所需的独立测试夹具，且 fixture 不预编排唯一正确查询或答案正文。HR-1 的“已验收”只表示基线可复现；生产行为仍由后续 HR-2 至 HR-5 改造。

## 5. HR-2：Intake 去领域化

实现方向：

- 使用现有 `AgentIntent`、Effect、ContextMode、Freshness、Effort、RiskClass 和 CapabilityId 决定 envelope。
- 普通外部事实默认 WebPreferred；明确联网/URL/强时效/高风险才 WebRequired。
- FreshFactDomain 和 DomainOperation 不再决定权限、工具面、澄清或完成门禁。
- 领域旧字段保留反序列化和恢复读取；新 envelope 不再写入领域规划事实。

同步删除：所有“未命中排除项即 StrictExternalFact”的终局分支、领域 operation 可执行性前置阻断和相应关键词矩阵。

退出条件：Chat、AskNotes、Research、CitationCheck、Draft、Apply 的表驱动 intake 测试覆盖 Offline/WebPreferred/WebRequired，并证明 Web 开关不能被模型或分类器增权。

## 6. HR-3：统一自适应工具循环

实现方向：

- `RunBudgetPolicy` 升级为唯一总量与分类预算事实源，兼容读取旧 schema。
- catalog 现有 `cost_class` 扩展为 local、network、external_read、runtime 和 confirmed_change。
- 通用循环根据 resource ID、canonical URL、内容 hash 和 revision 计算进展。
- 连续两轮无进展或探索预算耗尽时关闭工具并保留最后一次综合。
- Web、本地和外部只读工具共享循环，但继续保持各自安全与内容边界。

同步删除：`FreshResearchPlan` 生产路由、闭集 `EvidenceGap`、Web evidence 专用停机、重复 deadline 和 search/fetch/repair 平行计数。

退出条件：差结果改写查询、本地多跳、混合工具、重复抑制、分类上限、取消、总预算和强制综合全部通过同一循环测试。

## 7. HR-4：回答、澄清、错误与投影

实现方向：

- 普通回答直接持久化自然正文和受控来源组，不暴露 `submit_final_answer`。
- CitationCheck、高风险当前事实和其他严格合同才启用结构化终局。
- 普通缺参自然追问并完成；`AwaitingInput` 只兼容读取旧 Run。
- 前端只由同 Run assistant 投影拥有正文、过程、来源和终态。
- 用户错误提示使用“证据不足、能力不可用、回答未完成”等产品语义；协议码只进诊断。

同步删除：新普通 Run 的 input transaction、领域补充卡特殊路由、普通回答的强制终局修复和重复来源解释器。

退出条件：自然追问、下一轮承接、正文不被终态覆盖、恢复不重放、迟到事件隔离和四类错误语义均有 Rust/Vitest 回归。

## 8. HR-5：冻结变更集

实现方向：

- 扩展现有 `FrozenChangePlan` 为最多 6 个操作、6 个文件的有序计划。
- 一次确认冻结完整参数、目标、base/expected hash、过期时间和 plan hash。
- Host 顺序执行；任何授权或 hash 漂移停止剩余操作。
- 成功后最多 2 次模型和 4 次目标限定的本地只读验证；不开放 Web、外部或新增写入。

同步删除：单 operation 假设、确认后固定零模型轮次和无法表达部分执行状态的测试。

退出条件：多文件成功、第二项前 hash 漂移、重启恢复、重复确认、部分执行报告、验证越权和再次写入拒绝均通过；不新增数据库表。

## 9. HR-6：领域核心退役

实施：

- 从 Intake、默认 tool surface、dispatcher 选择和 finalization 移除五类领域分支。
- 保留 migration 072、旧 envelope 和旧 provider mapping 的只读兼容。
- 有真实 Provider 的结构化能力通过统一 catalog/MCP snapshot/capability 接入，不创建领域状态机。
- 删除不再可达的 DTO renderer、fixture、helper、测试和文档；若某适配器仍有真实调用证据，单独记录保留理由。

退出条件：静态搜索证明核心不再依赖 FreshFactDomain/DomainOperation 做新 Run 决策；旧 Run 可安全读取或终态化；代码净减少且无第二路由。

## 10. HR-7：质量与 Provider 校准

- 建立普通对话、Web 自适应、本地多跳、混合材料、严格事实、无工具 Provider 和文档修改任务矩阵。
- 固定夹具验证行为和安全，不能冒充真实答案质量。
- 至少使用两个协议形态不同的 mock Gateway，禁止针对 MiniMax 调整核心。
- 真实 Provider 试点保持另行授权，记录模型、配置 hash、成本 checkpoint、p50/p95、token 和匿名 verdict，不保存正文、查询或 URL。

退出条件：质量、安全、性能和净简化门槛全部满足；真实试点缺失时只能声明 deterministic 能力，不得声明生产模型质量。

## 11. 阶段纪律

- 每阶段开始前创建独立、决策完整的测试先行实施计划。
- 每阶段提交必须列出保留、重构、删除和兼容读取项。
- 新抽象未替代并删除旧分支时只能标为部分实现。
- migration、公共 IPC 或持久化 JSON 变化必须提供兼容读取和回滚边界。
- 默认不新增依赖、数据库表、Provider 或模型专用分支。
