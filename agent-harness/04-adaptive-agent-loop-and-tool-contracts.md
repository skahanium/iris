# 04. 自适应 Agent 循环与工具合同

> **文档状态**：现行
> **文档类型**：目标合同
> **事实基线**：2026-09-05，审计起点 `70c929ac`

## 1. 唯一多轮循环

`AgentToolLoop` 是唯一的多轮模型—工具编排器。普通 Direct 保持一次直接流式调用；只有冻结的长 Direct 压缩形状会以空工具面复用同一循环。它接收已冻结的消息、工具表面和 `RunBudgetPolicy`，持续到模型给出答案、进入确认、被取消或触发 Host 停止条件。

本地检索、Web、runtime 和外部只读工具不再拥有各自的 planner、deadline 或无进展状态机。工具实现仍可拥有单次调用的超时、结果大小和安全策略，但不能建立第二循环。

## 2. 冻结预算

初始预算复用现有 profile，不继续保留 Quick/Standard/Deep Web 专用档位：

| Profile      | 模型轮次 | 工具总数 | 用途                            |
| ------------ | -------: | -------: | ------------------------------- |
| Direct       |        1 |        0 | 无工具普通回答                  |
| Standard     |        8 |       24 | 普通本地、Web 和外部只读任务    |
| Delegated    |        8 |       24 | 现有有界 ChildRun；不在本轮扩张 |
| DurableApply |        8 |       24 | 确认前读取与变更计划            |

当历史即将移出窗口且需要压缩时，Direct 只可冻结为唯一的受限形状：最多两次模型调用、零工具、零 ChildRun、零确认后调用。它先使用一次无工具压缩，再以同一公共 `AgentToolLoop` 回答；没有待压缩历史时仍保持一次模型调用。该形状不是新的 profile，不能由运行时任意扩张。

Standard、Delegated 和 DurableApply 同时受以下分类上限约束：

| 工具类别          | 上限 | 说明                                |
| ----------------- | ---: | ----------------------------------- |
| `local`           |   12 | 搜索、读取、提纲、反向链接等        |
| `network`         |    6 | Web 搜索和公开 HTTPS URL 的受控抓取 |
| `external_read`   |    6 | 用户明确授权的外部只读工具          |
| `runtime`         |    4 | 时间、应用状态等可信小快照          |
| confirmed changes |    6 | 一个冻结变更集内的有序写操作        |

分类上限不是额外额度，所有调用仍受 24 次总上限。预算在 Run 接受时冻结并持久化；旧 schema 按保守默认值读取，新 Run 只写当前 schema。

## 3. 模型自适应原则

Prompt 只提供通用研究行为，不提供电影、天气等领域脚本：

1. 识别用户问题中的时间、地域、对象、范围和输出要求。
2. 优先调用最直接、成本最低且已授权的工具。
3. 阅读结果后判断相关性、覆盖、时效、权威性和冲突。
4. 结果不佳时调整关键词、语言、时间范围、地域或来源方向。
5. 需要原文时读取已发现资源或公开 HTTPS URL；地址本身不是证据，读取仍经过统一安全边界。
6. 已有材料足够时立即停止工具调用并回答。
7. 预算不足时区分已确认内容、分析和未核实内容。

模型可以产生新的自然语言搜索方向；Host 不再要求闭集 `EvidenceGap` 枚举，也不解析模型的内部推理。

每个模型轮次都会看到同一种简洁循环状态：当前目标和最新纠正、必要历史与有界摘要、授权工具、已获得观察、上一轮机械反馈，以及模型/工具/分类剩余额度。预算是上限而非目标；Harness 不要求输出思维链或完整计划对象。

当前实现对 `WebRequired` 及需要实际检索的 `WebPreferred / VolatileExternalFact` 提供最低起步观察：Host 在首个模型回答回合前用允许外发的用户问题和可信日期搜索，并抓取最多两个不同候选 URL；指定 URL 优先读取。它复用同一 executor、授权、预算、审计、Broker 与 evidence ledger，结果以 Host Observation 注入上下文，不伪造 assistant 工具调用，也不关闭后续工具面。无检索义务的普通 Direct 没有该动作。

2026-09-14 的[修复方案](../docs/superpowers/plans/2026-09-14-agent-correctness-remediation.md)拟让自然问句先在既有模型回合中理解上下文，将 Host 保底移至未满足观察义务的提前终局校验点；这是待实现目标，不能作为当前执行事实。精确预算向模型的现行投影及其正文外露问题也由该方案收敛；内部预算不属于普通用户回答内容。

## 4. 有界行动批次

- 一个模型回合最多执行 2 个彼此独立的发现型调用，例如两个不同 Web 查询或两种本地检索方向。
- 已知 URL、本地文件和确定性资源的精确读取不是发现调用，但仍受 24 次总量与分类上限约束。
- 依赖上一结果才能确定参数的动作必须等待该批观察返回；Host 不替模型自动追加搜索。
- 同轮超出的发现调用返回成功状态 `deferred_for_feedback`，不消耗工具额度、不记为失败，模型可在看到当前观察后重新决定。
- 每批结束必回到模型；不存在后台持续研究、固定反思模型或无限 LOOP。
- 模型的工具提议不是执行事实。Host 先标记 `rejected`、`deferred` 或 `dispatched`：只有 dispatched 调用进入 canonical assistant/tool transcript、消耗预算、写审计并绑定 Provider 续轮；前两类只回传机械反馈，不能阻止无可见动作的 Provider failover。

## 5. 重复、进展与收束

- 工具 fingerprint 由 tool name 和规范化参数组成；成功且仍可见的观察不再执行。既有 fingerprint 状态保存本 Run 的有界执行结果和压缩标记，不另建结果账本。
- 相同失败 fingerprint 最多执行两次，且必须受总预算约束。
- 阅读进展使用组合身份 `(resource, revision/hash, start, actualEnd, unit)`，本地单位为 UTF-8 字节，网页为 Unicode scalar。直接读取与检索 `ContextPacket` 按各自现行字段合同提取相同身份；同一笔记的新片段是进展，重新排序不是。空窗口、同页重放和部分成功批次中的失败 URL 不是新进展。非阅读工具保留既有安全资源身份。
- 不同查询返回相同资源和内容不算新进展。
- 连续两个完整模型—工具回合没有新进展时，Host 关闭工具面并发出一次通用综合指令。
- 探索预算即将耗尽时同样关闭工具，保留最后一次模型轮次；不得先把全部轮次消耗完再返回 `ToolLoopLimit`。
- 强制综合后仍没有可见正文、发生权限越界或严格证据要求未满足，才进入失败终态。

### 5.1 终态类型与两个循环状态投影

`AgentToolLoopOutcome` 携带显式 `AgentTerminalType`：`ModelAnswer`、`RepairedModelAnswer`（Host 先扣下一次草稿，最终仍由 Provider 发布）或 `HostEvidenceLimited`。正常回答、证据补修、强制综合和 Host 兜底四个构造点全部显式赋值；校验、引用绑定、来源与证据提交消费同一字段。

- Provider `finish_reason` 只表达 Provider 停止原因，不再兼任 Host 身份；Host 兜底终态的 `finish_reason` 为 `stop`，不再写入合成值 `evidence_limited`。
- 只有 `HostEvidenceLimited` 可以跳过 Provider 输出的完整性/停止原因校验，并且它不登记 citation map、source summary 或来源卡片。该判定来自循环控制流，永不来自正文内容。
- 旧 Run 的持久化正文没有终态类型，只有开头文字可查。该识别隔离在 [`legacy_terminal_records.rs`](../src-tauri/src/ai_runtime/run_engine/legacy_terminal_records.rs)，并要求完整旧披露形状（开头词加验证声明），因此以相同开头词写出的模型回答仍是模型输出，照常受校验。
- 循环状态分为两个投影，不复制两份预算。Host 保留精确计数、超时阶段与 Provider 尝试；模型只取得 `canContinue`、`mustSynthesize`、`failureType` 与 `nextAction`。模型观察与工具提议反馈不再出现 `remainingModelTurns`、`remainingToolCalls`、`remainingCategoryCalls`、`remainingBudgetMs`、`webUsage` 或开启时的预算清单。
- 执行器的预尺寸必须预留 `LoopProjection::widest()`；真实投影永不比它更宽，因此已登记的 Web 摘录不会被模型侧二次缩短。

工具观察压缩仅省略旧正文，保留原始 success/error、身份、范围、指针和完整写入回执；最近一批完整 assistant/tool 交互与用户约束保持原样。模型以相同参数请求已压缩观察时，Host 重检当前权限并重放已保存的有界结果，标记 `historicalObservation`，不表示重新读取最新资源。重放消耗逻辑调用和类别预算，但不访问 Provider、不重登记证据、不增加进展；写入回执保持可见，绝不因恢复正文而重执行写入。记录容量受原 Run 调用和工具载荷上限约束。

观察只按合法 JSON 值收缩，并核算实际序列化后的转义开销。列表省略完整尾项，读取适配层更新实际范围与续读指针，再登记最终摘录；通用循环不得再缩短已登记的 Web 摘录。最小合法信封或受保护的最新批次仍超限时，走既有预算终态，不制造工具失败、删除最新观察或发送 JSON 片段。

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

- 模型工具面只保留两个单一职责网络动作：`web_search { query }` 发现候选，`web_fetch { urls, startChar? }` 读取已选正文。两者共用同一个 `web.search` 用户授权、network 分类预算、`WebEvidenceBroker`、冻结 Provider 顺序和 evidence ledger，不构成第二套循环。
- native 与 MCP 的受控正文进入同一 Run 内存快照，保留规范 URL、内容 hash 和来源；每页最多 12,000 字，快照数受原 Run 证据容量约束。首读复用快照；`startChar > 0` 只能读取已有快照，缺失时返回 `snapshot_unavailable`，不能重新抓取后拼接旧偏移。Run 结束后不沿用快照身份。
- 每个窗口最多 2,000 个 Unicode scalar 字符；先取窗口，再按实际 JSON 大小收缩，最后将同一摘录登记进 ledger。`endChar` 和 `nextStartChar` 按实际可见字符数计算，每个结果独立携带窗口；仅单页兼容顶层 `excerptWindow`。引用按返回的 evidence ID 取 Run 内标签；同 URL 的后页不借用首页引用，也不增加独立来源数。
- `startChar == snapshotChars` 返回 EOF，大于该值返回范围错误，两者不登记空证据。`nextStartChar: null` 仅表示快照结束；`upstreamCompleteness` 的 `complete`、`bounded`、`unknown` 分别表示上游提取正文已完整返回、明确受限和未提供完整性证明，不等于整张网页或其中全部报道已核实。
- MCP 正文与完整性由同一次解析选取，完整性只读取实际选中正文所属对象的布尔声明；其他结果、旁支元数据或正文中的同名文字不能提供完整性证明。未声明或声明类型不符时保持 `unknown`。
- `web_search` 每次最多返回 4 个去重候选，每 Run 最多保留 8 个。候选只提供标题、来源、时间和有界片段，`evidenceIds` 为空；它不再接受 `urls` 重载。
- `web_fetch` 接受公开 HTTPS URL。只有抓取到 URL 匹配的实质正文才登记 evidence 并获得 `Wn`；搜索片段绝不能在 `run_tool_loop` 中被升级为证据。
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

- 普通回答：自然正文，可附 Host 绑定的受控来源区；不要求模型复述内部 `Wn` 标记或调用 `submit_final_answer`。
- WebPreferred：有证据时展示当前 Run 来源；日常 `VolatileExternalFact` 必须实际搜索，有适合候选时读取正文。缺少正文可基于实际观察说明缺口，不能把训练知识冒充当前事实，也不因无摘录整轮失败。最低观察只负责启动，不得因首次搜索或抓取无结果就跳过仍有预算的调整机会。
- `HighStakesCurrentFact`、用户明示核实、显式 URL、`CitationCheck` 或交叉核实：必须取得合格正文。只有搜索片段、来源冲突或跨 Run evidence 均不得通过；无摘录时 Host 有内容降级，不得下适用结论。
- 普通直答、联网、笔记问答统一在现有检查及终态提交后一次发布；证据不足时限制说明不得携带 citation map、source summary 或来源卡片。该限制说明由 Host 撰写并带 `HostEvidenceLimited` 终态类型；它不经过模型输出的停止原因/完整性恢复，也不参与引用绑定。
- 回答正文围绕问题组织事件、结论和必要的不确定性。候选摘要、已读摘录、来源绑定与独立印证必须语义区分：搜索结果只是候选，citation 标记只说明哪份来源支持哪项主张，`[Wn]` 存在不代表该来源内部每条报道都已被核对，同一来源的多页不构成独立印证。运行正常时不显示工具名、轮次和预算；用户主动询问运行诊断时由诊断入口说明真实执行状态。
- `ProvenancePolicy` 统一解析 `Wn`、`E{id}`、`L{id}`、`Mn`；`[Cn]` 和数据库裸 ID 只用于内部或展示。
- Harness 校验来源存在、归属、时效和声明的覆盖关系，不宣称完成自由文本 NLI。

## 10. Provider 恢复与失败连续性

- 模型成功但正文、工具调用均为空，按无效响应处理，不能完成 Run。
- 首次模型调用尚无可见正文、工具调用或 continuation 时，瞬态服务响应先在原 Provider 重试一次；仍失败时，只有此前动作全部只读且不再开放工具，才可切换一次候选综合已有观察。工具提议格式修复不触发模型切换。
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

## Agent 正文确认后发布（2026-09-08）

整个 Agent 对话（普通直答、联网及笔记问答）先在内部完成生成、安全净化、完整性及来源处理，再确认唯一正文并平滑显示。工具结束不得解除草稿封存；正常路径不得撤回已显示正文。普通域最终正文分块、会话消息、来源与 completed 在同一事务提交后才投影；涉密域仍只使用既有内存结果。前端只消费已完成的权威正文，晚到 reset 或较短快照不能改写目标；本地播放结束才显示“答复完毕”。首字更晚出现是明确选择，不增加核验模型调用。实现与回放证据未完成前不宣称无撤回验收通过。
