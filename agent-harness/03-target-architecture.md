# 03. 目标架构

## 1. 单一 Harness 形态

```text
User Turn
  -> Run Intake / Permission Snapshot / Task Shape
  -> ExecutionEnvelope + ToolSurfacePlan
  -> one Run Engine
       -> trusted runtime fast path
       -> structured exact-fact fast path (when configured)
       -> bounded web research loop
  -> one Evidence Ledger
  -> deterministic evidence/finalization gates
  -> Durable Message + Run State + Recoverable UI Projection
```

以下组件始终保持单一：

- Run engine 与 durable state machine；
- prompt compiler 与 provider protocol adapter；
- tool catalog 与当前 Run tool surface；
- MCP/provider registry；
- Web evidence broker 与 evidence ledger；
- 前端 Run presentation 与恢复来源。

## 2. Intake：从领域优先转为任务形态优先

Intake 仍可识别 `FreshFactDomain` 和 `DomainOperation`，但路由决策首先区分：

- `RuntimeFact`：本机时间、日期、时区等可信 runtime 事实；
- `ExactCurrentFact`：报价、观测、比分、明确排期等需要固定字段的事实；
- `CurrentResearch`：新闻综述、原因解释、比较、推荐、前瞻和跨来源综合；
- `NonCurrent`：无需联网的普通对话、转换和本地任务。

领域只决定字段、时效和地域规则，不再决定“能否让模型研究”。无结构化 binding 时，ExactCurrentFact 可以尝试受控 Web 证据合同；CurrentResearch 默认进入 Web loop。

## 3. 统一网络研究循环

`web_search` 是唯一模型可见网络工具，Host 内部可以执行搜索和受控页面抓取：

```text
initial query
  -> search results registered in current Run
  -> assess deterministic fields + model EvidenceGap
  -> next query OR current-Run URL selection
  -> bounded concurrent fetch (max 3)
  -> evidence ledger update and dedupe
  -> early stop / finalization / explicit insufficiency
```

模型负责在已有证据基础上调整查询、缺口和选定 URL；Host 负责授权、provenance、SSRF、预算、重复检测、证据登记和最终化。模型不能请求任意未登记 URL，也不能自行提高预算。

支持原生工具续接的协议使用模型驱动循环；chat-only 或未经验证的自定义 endpoint 使用 Host 驱动的有限预取加一次综合。该差异来自现有协议能力，不建立新的持久化模型评分库。

## 4. 结构化精确事实快路径

```text
ExactCurrentFact
  -> matching trusted Run-frozen binding exists?
       yes -> provider call -> mapping -> DTO validator -> Iris evidence ID
       no  -> bounded Web research -> same field/freshness contract
  -> deterministic finalization
```

结构化快路径的价值是低延迟、固定字段和更强失败语义，不是架构中心。它继续复用现有 operation、snapshot、mapping、DTO、renderer 和 migration，不建设独立 readiness 真相源。

只有真实 Provider 被选择时才创建 PDR；PDR 决定覆盖、许可、成本、字段和接入路径。没有合规 Provider 时允许保持未配置，而不是降低验证标准或补造 binding。

## 5. 证据与最终化数据流

所有成功路径都必须先把证据登记到当前 Run：

```text
provider/search/fetch output
  -> sanitize + normalize
  -> field/freshness/location validation
  -> current-Run evidence registration
  -> answer claim to evidence binding
  -> durable final message
```

- ExactCurrentFact 必须满足对应字段合同，不能只依赖来源组。
- CurrentResearch 允许综合多个 Web 证据，但每个时效结论仍需可定位引用。
- 证据冲突在预算内无法解决时必须显示冲突，不由模型静默选择。
- 恢复只读取已持久化消息和绑定，不重新调用 Provider。

## 6. 性能控制面

性能控制复用 `ResearchBudget`，至少包含 profile、剩余搜索、剩余抓取、剩余修复、剩余模型轮次、证据上限和 deadline。预算状态随 Run 可恢复，技术重试与业务研究轮次分别计数。

- 搜索轮次串行，防止模型同时扩散多个无关方向。
- 同一轮选定页面最多并发抓取 3 个。
- 证据充分立即停止；连续两轮没有新增有效证据立即停止。
- first progress event 目标为接受 Run 后 500ms 内。
- Deep 仅由用户明确请求或 UI 明确选择，不能由模型静默升级。

## 7. 禁止的平行架构

- 第二套 `web_fetch` 模型工具或浏览器研究引擎；
- 新 provider registry、evidence table、Run 状态表或会话真相表；
- 以模型名称为键的长期能力评分数据库；
- 为兼容旧路由建立长期 facade 或双写；
- 没有真实 Provider 需求时建设通用 REST adapter、health ranking 或 readiness 控制台。
