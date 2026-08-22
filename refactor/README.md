# Iris Agent Harness 可靠性重构

本目录定义 Iris 现有 Agent Harness 的可靠性边界、当前差距与增量施工。它不是第二份产品路线图，也不把 Iris 扩张成通用 Agent 平台。

- 版本排期与产品边界以 [`ROADMAP.md`](../ROADMAP.md) 为唯一来源。
- 已实现架构事实以 [`ARCHITECTURE.md`](../ARCHITECTURE.md) 和代码为准。
- 本目录记录可靠性约束、已核实差距、实施顺序和验收证据；完成状态以附录 A、B 与真实测试为准。
- 第一轮 Run、工具、证据展示、摘要与最小记忆收口已经形成代码基线；当前核心增量只收口尚未解决的时效事实可靠性与跨 Run 前端隔离。领域专用工具属于其后的能力增强，不阻塞核心缺陷验收。

## 当前状态

### 已建立且不得回退的基线

- `client_request_id` 是持久化 Run 的幂等键：同 ID、同请求指纹的重放返回原 Run 且不重复取得执行权；同 ID、不同指纹返回幂等冲突。`session_key` 只界定活动顶层 Run 的单航班范围。新的 ID 即使文本相同也代表新请求。
- 最终助手消息、Run 终态和前端完成投影遵守 durable finalization 顺序；sink 失败不改写已提交事实。
- 模型可见工具、`capabilities_read` 和执行门禁消费同一冻结工具表面。
- Web 来源诚实区分精确绑定与 `SourceGroupFallback`，来源组不冒充逐段引用。
- 会话摘要读取时重新验证；长期记忆只保留经确认的 global/vault 短偏好。

以上是长期回归基线，不是本轮尚待实现的目标。具体证据见附录 A、B。

### 本轮待完成目标

1. 让日期、天气、新闻、金融、影视与体育等时效事实由可信运行时或本 Run 新鲜证据支持；证据不足时失败关闭，不从模型记忆猜测。
2. 让简单问题使用有界研究预算：证据充分即停止，只有明确证据缺口时才继续搜索，不以无界思索替代回答。
3. 让严格当前事实通过结构化终局或确定性模板收口；`SourceGroupFallback` 不能单独满足成功条件。
4. 让每个助手回答只属于其 Run；新一轮处理期间不得投影上一轮正文、reveal 或迟到事件。
5. 用真实复现场景和问题—测试追踪证明用户可见问题已解决，而不是只证明内部流程执行过。

### 后续能力增强（不阻塞核心缺陷收口）

- 在通用时效链路稳定后，再建立时间、天气、新闻、金融、影视和体育的最小只读能力与低配置 provider 映射。
- 该增强有独立的 `CAP-001` 验收边界；未交付时可以表述为“领域能力未补齐”，但不能据此否定核心缺陷是否已经修复。
- 不得为了领域扩展预建万能数据平台、第二套 provider registry 或第二证据仓库。

## 文档语义

| 文档层         | 回答的问题                       | 允许的状态语气                           |
| -------------- | -------------------------------- | ---------------------------------------- |
| `01`、`03`     | 系统始终必须遵守什么             | 规范性契约，不表示已经实现               |
| `02`、附录 A   | 当前代码实际做到什么、缺什么     | 已交付/当前缺口/目标调整，必须有代码依据 |
| `04`、`plans/` | 接下来按什么顺序施工             | 计划中，不得写入架构事实                 |
| `05`、附录 B   | 什么证据允许改成 Resolved        | 已有实证与目标测试严格分开               |
| 附录 C         | 哪些能力明确不进入本轮           | Deferred                                 |
| 附录 D         | 六类能力应满足什么字段和失败边界 | 后续能力增强契约，实施前不表示可用       |

## 文档结构

### 核心文档

1. [`01-invariants-and-non-goals.md`](01-invariants-and-non-goals.md)：不可破坏的系统约束与明确非目标。
2. [`02-current-to-target-delta.md`](02-current-to-target-delta.md)：基于当前代码的最小目标形态。
3. [`03-reliability-contracts.md`](03-reliability-contracts.md)：Run、工具、授权、证据、时效、上下文和 UI 契约。
4. [`04-implementation-roadmap.md`](04-implementation-roadmap.md)：按风险排序的实施阶段与停线条件。
5. [`05-evaluation-and-acceptance.md`](05-evaluation-and-acceptance.md)：测试分层、场景矩阵和完成标准。

### 附录

- [`appendices/A-current-state-audit.md`](appendices/A-current-state-audit.md)：现状核对清单，只记录可由代码复核的事实。
- [`appendices/B-issue-test-traceability.md`](appendices/B-issue-test-traceability.md)：问题 ID 到测试用例和验收证据的追踪表。
- [`appendices/C-deferred-capabilities.md`](appendices/C-deferred-capabilities.md)：不进入本次主干的未来能力提案。
- [`appendices/D-current-fact-domain-matrix.md`](appendices/D-current-fact-domain-matrix.md)：六类当前事实的字段、时效、地域、来源与降级规则。

### 施工计划

- [`plans/01-turn-projection-isolation.md`](plans/01-turn-projection-isolation.md)：先修复跨 Run 旧答案串入新回答的问题。
- [`plans/02-freshness-routing-and-grounding.md`](plans/02-freshness-routing-and-grounding.md)：修正时效分类、有界联网研究和严格证据化收口。
- [`plans/03-common-domain-capabilities.md`](plans/03-common-domain-capabilities.md)：核心缺陷收口后，建设六类稳定能力与低配置服务商映射。

施工计划是可删除的阶段性执行清单；长期契约、当前事实和完成证据分别由上述对应文档维护。计划完成后不得把计划中的目标语气反向当成已实现架构事实。

## 使用方式

1. 先在附录 A 确认问题仍存在，并把状态标为 `Confirmed` 或 `Partial`。
2. 在附录 B 登记目标测试，但明确区分“计划名称”和“仓库中已存在且已运行的测试”。
3. 按实施路线图选择一个阶段，并使用对应施工计划逐项执行测试先行。
4. 实现时遵守可靠性契约；涉及阶段 8 的领域能力时再同时遵守附录 D。代码事实变化后优先更新附录 A、B。
5. 只有测试实际通过后，才可把问题状态改为 `Resolved`；不得用宽泛 E2E、来源组数量或模型自述代替证明。

## 核心缺陷收口完成定义

本次截图和对话暴露的时效链路与跨 Run 隔离缺陷，只有在以下条件同时成立时才算完成：

- `UI-003`、`ROUTE-003`、`WEB-001`、`EVID-005`、`EVAL-002` 均有修复前失败、修复后通过的聚焦测试；
- 第一轮已建立的 Run、工具授权、来源展示、摘要和最小记忆回归门禁继续通过；
- 本机日期问题不会误发 Web 请求；当前外部事实不会在缺少本 Run 可用证据时生成确定性结论；
- `SourceGroupFallback` 只表示本次检索来源组，不能作为严格当前事实已获支持的完成证据；
- 新 Run 从接受到首个本轮答案片段期间，助手正文为空且不会显示上一 Run 的 reveal、消息或迟到事件；
- 没有引入第二套 Run 状态机、工具目录、证据存储或会话真相源；
- 自由文本 LLM judge、跨会话语义记忆、浏览器操作和领域工具市场未被当作默认完成条件；
- 附录 A、B、真实测试结果与 `ROADMAP.md` 的边界一致，阶段 5–7 的核心 P0/P1 均为 `Resolved` 或有明确 Deferred 决定。

在阶段 5–7 的核心问题仍为 `Confirmed`、`Partial` 或 `Planned` 时，只能表述为“第一轮结构性基线已建立，核心缺陷补建未完成”。

## 领域能力增强完成定义

`CAP-001` 独立验收。只有六类当前事实遵守附录 D 的字段、时效、地域和来源规则，且无专用服务商时能够严格降级而不编造，才可宣称领域能力增强完成。该状态不得反向改变阶段 5–7 的验收结论。
