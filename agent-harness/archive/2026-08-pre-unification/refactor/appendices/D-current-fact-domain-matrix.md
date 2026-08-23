# 附录 D：当前事实领域契约矩阵

**状态：Framework implemented / instance unconfigured。** 本附录的 operation、DTO、字段验证、Run-local 授权与生产夹具已实现；这不表示任何用户实例已配置真实结构化 Provider，也不阻塞阶段 5–7 的核心缺陷收口。当前实例可用性仍以已保存且唯一的 binding 为准。

它定义 Iris 为常见当前事实应提供什么、如何验证以及何时拒绝回答；不指定某个商业服务商，也不构成通用数据平台。

## 1. 共同输入与输出

所有外部领域请求共享以下冻结输入：

```text
FreshFactRequest
  domain
  operation
  query/subject
  requestedAt
  locale
  absoluteWindow { start, end }
  location? { city?, province?, country? }
```

所有成功记录共享以下来源字段：

```text
EvidenceOrigin
  evidenceId
  providerId
  sourceUrl
  sourceTitle
  observedAt/asOf/publishedAt/checkedAt
```

共同规则：

- 时间使用 RFC 3339 并保留时区；只有日期语义时使用 `YYYY-MM-DD` 与明确地域。
- URL 必须是当前 Run 接受的 HTTPS 证据；provider 内部标识不能替代公开来源或已审核结构化来源身份。
- DTO 中用户可见的实体名、数字和日期必须来自 provider 字段或本 Run 可定位证据。
- 结构化 provider 优先；通用 Web 只有在能提取并验证同一组必需字段时才可成功。
- 缺少必需字段、时效超限、地域不匹配或来源冲突时返回确定性不足结果，不由模型补写。

## 2. 领域矩阵

| 领域          | 稳定操作                                                                         | 必需字段                                                                        | 默认时间窗/新鲜度                                                    | 地域要求                                                              | 允许的成功来源                                 | 失败关闭条件                                               |
| ------------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | -------------------------------------------------------------------- | --------------------------------------------------------------------- | ---------------------------------------------- | ---------------------------------------------------------- |
| runtime       | `system_time_now`                                                                | local datetime、date、weekday、timezone                                         | 接受 Run 时读取；不缓存为外部事实                                    | 本机时区                                                              | 本机可信 runtime                               | runtime 不可读或内部字段矛盾                               |
| weather       | `weather.current`、`weather.forecast`                                            | location、condition、temperature/units、observation/issue time、source          | 当前 observation ≤ 3 小时；forecast issue ≤ 12 小时；默认未来 7 天   | city 必需；province/country 仅用于消歧                                | 已审核结构化 provider；满足相同字段的 Web 证据 | 无城市、单位缺失、地点不符、时间超限、无来源               |
| news          | `news.search`                                                                    | title、publisher、publishedAt、URL、topic/location                              | 默认最近 72 小时；尊重用户明确窗口                                   | 可无地点；地域新闻按 city→province→country 放宽                       | WebEvidenceBroker；结构化 provider             | 无发布日期、窗口外、标题/URL 不可定位、把评论当新闻事实    |
| finance       | `finance.quote`、`finance.metrics`、`finance.news`                               | instrument identity、asset kind、currency、asOf、delay、value/source            | 最近交易时点；无 `asOf` 不成功；delay ≤ 15 分钟才可称实时            | 市场/交易所优先，不以用户地点替代                                     | 已审核结构化 provider；公司新闻可用 Web        | 标的歧义、币种/单位缺失、时点缺失、延迟未声明、证据外数值  |
| entertainment | `entertainment.now_playing`、`entertainment.upcoming`、`entertainment.streaming` | title、region、channel/platform、availability/release date、checkedAt、source   | 过去 30 天至未来 60 天；checkedAt ≤ 24 小时                          | 本地院线需 city；全国档期可 province/country；流媒体需 country/region | WebEvidenceBroker；结构化 provider             | 无地域/渠道/日期、全国档期冒充本地排片、旧作品冒充近期可用 |
| sports        | `sports.schedule`、`sports.score`                                                | competition/league、participants、start time、status、score?、checkedAt、source | 赛程默认当天至未来 7 天；live checkedAt ≤ 15 分钟；非 live ≤ 24 小时 | 以赛事/联盟为主，地点仅作筛选                                         | WebEvidenceBroker；结构化 provider             | 赛事或参赛方歧义、live 数据陈旧、状态/时间缺失、无来源     |

## 3. 地域解析

地点来源仅允许：

1. 当前请求明确地点；
2. 用户确认写入的 global memory：
   - `location.city`
   - `location.province`
   - `location.country`

禁止从 IP、系统网络、provider 端点、Vault 笔记或模型推断地点。

领域行为：

- 天气和附近影院必须有 city；缺失时返回 `agent_run_location_required`。
- 新闻、全国档期和部分体育筛选允许城市 → 省份 → 国家逐级放宽。
- 每次放宽必须由首个范围证据不足触发，并记录 `EvidenceGap::LocationCoverage`。
- 回答必须显示最终使用的地域，不能省略放宽事实。
- 本轮明确地点不会未经确认覆盖长期 memory。

## 4. 研究预算与证据缺口

允许触发后续查询的缺口枚举：

```text
EvidenceGap = MissingEntity
            | MissingLocation
            | LocationCoverage
            | MissingTimestamp
            | StaleObservation
            | MissingUnit
            | MissingChannel
            | MissingIndependentSource
            | SourceConflict
```

- 单一事实最多 2 次搜索、3 次抓取、1 次结构化修复。
- 新闻、推荐、比较最多 3 次搜索、5 次抓取、1 次结构化修复。
- 后续查询必须对应一个未解决的 `EvidenceGap`；相同 query 和相同缺口不得重复执行。
- `MissingIndependentSource` 只在请求本身需要核实、来源冲突或高风险分析时触发；普通单一事实不强制为了数量搜索第二来源。
- `SourceConflict` 无法在预算内解决时必须在不足结果中说明冲突，不选择更顺眼的答案。

## 5. 严格最终化

以下内容视为当前事实块：

- 当前或未来的日期、时间、天气、价格、比分、排期和可用状态；
- 以“最近、最新、现在、正在、即将、本周、今天”等相对时间表述的外部结论；
- 基于上述数据的数值比较、趋势描述和推荐理由。

当前事实块必须通过内部 `submit_final_answer` 绑定到领域记录或可定位证据。允许模型组织语言，但不允许：

- 引入候选列表之外的新实体；
- 改写数字、币种、单位、日期或地域；
- 把推测写成上映、可观看、实时、官方或已确认；
- 用 `SourceGroupFallback` 替代字段级支持。

纯标题、分隔线以及明确标注的非事实建议可以不绑定领域记录；金融趋势/比较仍必须引用其数据输入，并标注为描述性分析。

## 6. 金融边界

允许：

- 行情、指数、基金、外汇、加密资产的事实数据；
- 公司/市场新闻与基础财务指标；
- 基于已提供数据的趋势、变化和横向比较；
- 清楚展示数据时点、延迟、币种和来源。

禁止：

- 个性化买入、卖出、仓位或价格目标建议；
- 自动交易、下单或连接券商写操作；
- 在缺少时点、币种或标的消歧时给出确定性数值；
- 把历史表现描述成未来收益保证。

## 7. Provider 与降级

用户不需要为每轮请求选择 provider。Run 接受时只处理已冻结的一个 operation：恰好一个 eligible binding 则冻结；没有 binding 时仅 `news.search` 可走 WebEvidenceBroker，其余 operation 在模型调用前以 provider unavailable 失败关闭；多个 eligible binding 一律 `agent_run_structured_provider_ambiguous`。不按通用 Web provider 偏好、名称、更新时间或插入顺序静默挑选。

这里的 `eligible` 只表示 enabled、hash-current、user-trusted 且具有 output mapping；它不是运行时健康排序。本阶段没有自动 failover 或 REST adapter。

Provider mapping 必须冻结：operation、provider/tool、输入 schema、argument mapping、output mapping、provider/config hash、transport、credential refs 与 review 状态。运行中 provider 被禁用或 hash 漂移时立即失败关闭，不切换到未经本 Run 冻结的工具。

## 8. 用户可见失败语义

- `agent_run_location_required`：说明需要哪个地域粒度，并直接询问地点。
- `agent_run_fresh_evidence_insufficient`：说明缺失的是时点、地域、来源或字段，不输出猜测结论。
- `agent_run_grounded_finalization_unavailable`：说明当前模型无法完成受证据约束的回答，建议更换支持工具协议的模型。
- provider 暂时失败但存在安全 fallback 时可以继续；所有候选失败后使用现有 provider unavailable 语义，不泄露端点或凭证。

这些错误可以附带本次检索来源组供用户核对，但来源组展示不改变 Run 的失败关闭结论。
