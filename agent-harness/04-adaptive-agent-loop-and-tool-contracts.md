# 04. 自适应 Agent 循环与工具合同

> **文档状态**：现行
> **文档类型**：目标合同
> **事实基线**：2026-09-03，审计起点 `e30f47d1`

## 1. 唯一多轮循环

`AgentToolLoop` 是 Direct 之外唯一的模型—工具编排器。它接收已冻结的消息、工具表面和 `RunBudgetPolicy`，持续到模型给出答案、进入确认、被取消或触发 Host 停止条件。

本地检索、Web、runtime 和外部只读工具不再拥有各自的 planner、deadline 或无进展状态机。工具实现仍可拥有单次调用的超时、结果大小和安全策略，但不能建立第二循环。

## 2. 冻结预算

初始预算复用现有 profile，不继续保留 Quick/Standard/Deep Web 专用档位：

| Profile      | 模型轮次 | 工具总数 | 用途                            |
| ------------ | -------: | -------: | ------------------------------- |
| Direct       |        1 |        0 | 无工具普通回答                  |
| Standard     |        8 |       24 | 普通本地、Web 和外部只读任务    |
| Delegated    |        8 |       24 | 现有有界 ChildRun；不在本轮扩张 |
| DurableApply |        8 |       24 | 确认前读取与变更计划            |

Standard、Delegated 和 DurableApply 同时受以下分类上限约束：

| 工具类别          | 上限 | 说明                                     |
| ----------------- | ---: | ---------------------------------------- |
| `local`           |   12 | 搜索、读取、提纲、反向链接等             |
| `network`         |    6 | Web 搜索和当前 Run 已登记 URL 的受控抓取 |
| `external_read`   |    6 | 用户明确授权的外部只读工具               |
| `runtime`         |    4 | 时间、应用状态等可信小快照               |
| confirmed changes |    6 | 一个冻结变更集内的有序写操作             |

分类上限不是额外额度，所有调用仍受 24 次总上限。预算在 Run 接受时冻结并持久化；旧 schema 按保守默认值读取，新 Run 只写当前 schema。

## 3. 模型自适应原则

Prompt 只提供通用研究行为，不提供电影、天气等领域脚本：

1. 识别用户问题中的时间、地域、对象、范围和输出要求。
2. 优先调用最直接、成本最低且已授权的工具。
3. 阅读结果后判断相关性、覆盖、时效、权威性和冲突。
4. 结果不佳时调整关键词、语言、时间范围、地域或来源方向。
5. 需要原文时读取当前 Run 已发现的资源，不猜测未登记 URL 或路径。
6. 已有材料足够时立即停止工具调用并回答。
7. 预算不足时区分已确认内容、分析和未核实内容。

模型可以产生新的自然语言搜索方向；Host 不再要求闭集 `EvidenceGap` 枚举，也不解析模型的内部推理。

每个模型轮次都会看到同一种简洁循环状态：当前目标和最新纠正、必要历史与有界摘要、授权工具、已获得观察、上一轮机械反馈，以及模型/工具/分类剩余额度。预算是上限而非目标；Harness 不要求输出思维链或完整计划对象。

`WebRequired` 另有一个最小、确定性的起步观察：Host 在首个回答回合前用用户原文执行一次搜索，并抓取最多两个不同候选 URL。它完全复用本循环的 executor、授权、预算、审计、Broker 与 evidence ledger，结果以 Host Observation 注入上下文；它不伪造 assistant 工具调用，也不关闭后续 Web 工具面。`WebPreferred` 和 Direct 没有该动作。

## 4. 有界行动批次

- 一个模型回合最多执行 2 个彼此独立的发现型调用，例如两个不同 Web 查询或两种本地检索方向。
- 已知 URL、本地文件和确定性资源的精确读取不是发现调用，但仍受 24 次总量与分类上限约束。
- 依赖上一结果才能确定参数的动作必须等待该批观察返回；Host 不替模型自动追加搜索。
- 同轮超出的发现调用返回成功状态 `deferred_for_feedback`，不消耗工具额度、不记为失败，模型可在看到当前观察后重新决定。
- 每批结束必回到模型；不存在后台持续研究、固定反思模型或无限 LOOP。
- 模型的工具提议不是执行事实。Host 先标记 `rejected`、`deferred` 或 `dispatched`：只有 dispatched 调用进入 canonical assistant/tool transcript、消耗预算、写审计并绑定 Provider 续轮；前两类只回传机械反馈，不能阻止无可见动作的 Provider failover。

## 5. 重复、进展与收束

- 工具 fingerprint 由 tool name 和规范化参数组成；成功 fingerprint 不再执行。
- 相同失败 fingerprint 最多执行两次，且必须受总预算约束。
- 进展只使用安全、可比较的事实：新 candidate/evidence/resource ID、新 canonical URL、更深的正文、新内容 hash、新 revision、目标文件 hash，或首次出现的可行动错误类别。
- 不同查询返回相同资源和内容不算新进展。
- 连续两个完整模型—工具回合没有新进展时，Host 关闭工具面并发出一次通用综合指令。
- 探索预算即将耗尽时同样关闭工具，保留最后一次模型轮次；不得先把全部轮次消耗完再返回 `ToolLoopLimit`。
- 强制综合后仍没有可见正文、发生权限越界或严格证据要求未满足，才进入失败终态。

## 6. 工具输入与结果

每个模型可见工具继续使用稳定名称和 JSON Schema。工具结果至少包含：

- success/error 和稳定错误码；
- 有界、已净化的模型可见内容；
- 资源类型与当前 Run 内安全标识；
- 可用时的标题、来源、时间、范围、截断状态和 revision；
- `newResourceCount`、`duplicateResourceCount`、观察深度、截断/访问限制和剩余预算；
- 供 Host 内部计算进展的 canonical identity，不进入日志或用户正文。

错误必须可行动，例如“无结果”“查询过宽”“资源已变化”“权限不足”“网络暂不可用”，不能只返回“工具失败”。原始 Provider 输出、凭证和不受限正文永不进入 transcript、事件或审计。

## 7. Web 候选、正文与本地工具

- 模型工具面只保留两个单一职责网络动作：`web_search { query }` 发现候选，`web_fetch { urls }` 读取已选正文。两者共用同一个 `web.search` 用户授权、network 分类预算、`WebEvidenceBroker`、冻结 Provider 顺序和 evidence ledger，不构成第二套循环。
- `web_search` 每次最多返回 4 个去重候选，每 Run 最多保留 8 个。候选只提供标题、来源、时间和有界片段，`evidenceIds` 为空；它不再接受 `urls` 重载。
- `web_fetch` 只接受当前 Run 候选或用户明确提供的 HTTPS URL。只有抓取到 URL 匹配的实质正文才登记 evidence 并获得 `Wn`；搜索片段绝不能在 `run_tool_loop` 中被升级为证据。
- 一批 URL 部分成功时，观察同时返回成功正文、失败 URL、剩余证据要求和预算，让模型选择换源、补充抓取或基于已取得正文完成；单个抓取失败不直接把整轮降级为限制回答。
- 搜索过但未选中的候选、抓取失败的页面和历史 Run URL 均不能支持最终结论。
- fetch 路由按当前来源优先、冻结 MCP 候选顺序和 native safe fetch 兜底依次尝试；单候选最多 5 秒、整批最多 18 秒、外层调用最多 20 秒。search 与 fetch 的健康度按 capability 分账，业务失败不改写 discovery 状态。
- 本地工具保持 `search_hybrid`、`search_semantic`、`search_keyword`、`read_note`、`get_outline`、`get_backlinks` 等正交能力。
- Web 和本地结果进入同一 evidence ledger，但保持不同权限、内容泄漏和来源展示策略。
- 模型可以在同一 Run 中交替使用本地和 Web 工具；Web query 只能来自用户公开子句和可信 runtime，不能包含自动检索笔记正文。

## 8. 结构化工具与可选适配器

结构化调用协议必须保留，因为它承担参数校验、授权、预算、审计和 Provider 中立。领域 operation 不再承担核心路由：

- 新 Run 的默认工具面不因 `FreshFactDomain` 自动暴露 weather/news/finance/entertainment/sports 工具；这五个旧 lookup 已从生产 catalog 和 dispatcher 删除。
- 有真实需求的精确数据源通过现有 catalog、MCP snapshot 和 capability 作为可选只读工具接入。
- 可选工具只返回 typed result，不创建领域 planner、独立 Run 状态或独立 finalization。
- migration 072、旧 mapping 和旧 envelope 保留读取兼容；未配置 Provider 不影响普通 Web 或本地任务。

## 9. 回答与来源

- 普通回答：自然正文，可附受控来源区；不要求 `submit_final_answer`。
- WebPreferred：有证据时展示当前 Run 来源；无证据时可以基于模型知识回答并说明时效限制。
- 普通 `VolatileExternalFact`：至少一份与核心结论相关、已抓取正文且被精确引用的当前 Run Web evidence 即可完成；不再无差别要求官方来源或两个域名。
- `HighStakesCurrentFact`、`CitationCheck` 或用户明确要求交叉核实：必须取得官方来源，或至少两个相互独立域名的合格正文。只有搜索片段、来源冲突或跨 Run evidence 均不得通过。
- 严格路径仍在验证后一次发布；证据不足时限制说明不得携带 citation map、source summary 或来源卡片。
- `ProvenancePolicy` 统一解析 `Wn`、`E{id}`、`L{id}`、`Mn`；`[Cn]` 和数据库裸 ID 只用于内部或展示。
- Harness 校验来源存在、归属、时效和声明的覆盖关系，不宣称完成自由文本 NLI。

## 10. Provider 恢复与失败连续性

- 模型成功但正文、工具调用均为空，按无效响应处理，不能完成 Run。
- 首次模型调用尚无可见正文、工具调用或 continuation 时，瞬态/无效响应先在原 Provider 重试一次；仍失败再切换具备相同冻结工具面的候选。
- 已有可见输出、工具调用、continuation 或副作用后禁止隐式跨 Provider 续接。
- Gateway 只在协议适配层处理 Provider 私有续轮字段：MiniMax 保持 `reasoning_details`，MiMo 的 custom-tool 回合关闭 thinking 并原样续接其返回的 `reasoning_content`；核心循环不按模型名分支。
- 现有 `provider_route_summary_json` 只追加有界诊断：Provider/模型 ID、尝试次数、协议阶段、错误类别、空响应、是否已有输出/工具，以及重试/切换/终止决定；不记录请求、响应或凭证正文。
- 下一用户 Run 可读取上一轮请求、终态、安全错误、模型/工具是否开始、重试与切换计数；这些运行事实只解释本地失败，不构成外部事实来源。

## 11. 澄清与暂停

普通缺少地点、范围、偏好或对象时，模型以自然 assistant 消息追问并完成当前 Run。下一条用户消息作为新 Run，通过已提交会话历史承接。

同 Run 暂停只用于：

- 冻结写入确认；
- 明确外部授权；
- 必须绑定原事务且不可安全重放的人工决定。

`AwaitingInput` 仅保留旧 Run 读取和安全终态兼容，新普通 Run 不再产生该状态。

## 12. 冻结变更集

DurableApply 在确认前允许模型反复使用已授权只读工具。最终变更集最多包含 6 个有序操作并影响 6 个文件，每项冻结：

- tool name 与规范化参数；
- vault 和相对路径；
- base content hash 与 expected post hash；
- tool call ID、计划 hash、过期时间和回滚摘要。

用户一次确认整个变更集。Host 执行时逐项重检授权和 hash；任何不一致停止剩余操作并以部分执行事实终态化。确认后的调度只能使用计划内目标，且只开放目标限定的 `read_note`，最多 2 次模型调用和 4 次工具调用。验证发现新修改需要时必须重新确认。

## 13. Provider 能力

Gateway 为本轮冻结 tools、continuation、parallel calls、streaming 和 structured output 能力。核心循环只消费这些协议事实，不读取模型名称：

- 支持 tools/continuation 的 Provider 使用完整循环。
- chat-only Provider 仅执行 Direct 或显示明确能力降级。
- 不稳定自定义 endpoint 不通过一次文本连通测试升级为 Agent-capable。
- 任何 Provider 都受同一权限、预算、来源和终态合同。
