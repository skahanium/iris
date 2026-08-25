# 附录 B：当前事实合同矩阵

本矩阵按任务形态决定主路径，再由领域规则决定字段、时效和地域。它不指定商业 Provider，也不把 11 个 operation 变成近期交付数量目标。

## 1. 任务形态矩阵

| 任务形态         | 例子                                     | 默认路径                                          | 成功条件                             | 不足时                     |
| ---------------- | ---------------------------------------- | ------------------------------------------------- | ------------------------------------ | -------------------------- |
| RuntimeFact      | 今天日期、当前时区                       | trusted runtime                                   | 本机字段内部一致                     | runtime 失败，绝不联网补猜 |
| ExactCurrentFact | 当前天气、报价、比分、明确排片           | configured structured fast path；否则 bounded Web | 必需字段、时效、地域、单位和来源完整 | 说明缺口或失败关闭         |
| CurrentResearch  | 新闻综述、市场原因、比赛前瞻、推荐、比较 | Standard Web research                             | 关键时效结论有当前 Run 可定位证据    | 披露冲突、范围或证据不足   |
| DeepResearch     | 用户明确要求深入调查                     | Deep Web research                                 | 多来源、缺口收敛且未越界             | 达到 90s/预算即停止        |
| NonCurrent       | 改写、总结、本地笔记任务                 | direct/local tools                                | 不产生不必要 Web 请求                | 按本地能力降级             |

## 2. 领域事实合同

| 领域          | 现有 operation                                     | 精确事实必需字段                                                         | 默认时效                               | 地域/范围                        | 研究型问题主路径           |
| ------------- | -------------------------------------------------- | ------------------------------------------------------------------------ | -------------------------------------- | -------------------------------- | -------------------------- |
| runtime       | `system_time_now`                                  | datetime、date、weekday、timezone                                        | Run 接受时读取                         | 本机时区                         | 不适用                     |
| weather       | `weather.current`, `weather.forecast`              | location、condition、temperature、units、observation/issue time、source  | observation ≤3h；forecast issue ≤12h   | city 必需                        | 灾害背景、趋势解释走 Web   |
| news          | `news.search`                                      | title、publisher、publishedAt、URL、topic/location                       | 默认 72h                               | 可无地点，地域新闻按明确范围     | Web research 为默认        |
| finance       | `finance.quote`, `finance.metrics`, `finance.news` | instrument、asset kind、currency、`asOf`、delay、value/source            | 最近交易时点；称实时需 delay ≤15m      | 市场/交易所优先                  | 原因、趋势、比较走 Web     |
| entertainment | `now_playing`, `upcoming`, `streaming`             | title、region、channel、availability/release date、checkedAt、source     | checkedAt ≤24h；默认过去30天至未来60天 | 本地院线需 city；流媒体需 region | 推荐、口碑、近期综述走 Web |
| sports        | `schedule`, `score`                                | competition、participants、start time、status、score?、checkedAt、source | live ≤15m；非 live ≤24h                | 赛事/联盟优先                    | 前瞻、复盘、阵容背景走 Web |

以上时效是默认值；用户明确窗口优先，但不能放宽“必须披露数据时点”的要求。

## 3. 地域与歧义

地点只允许来自：

1. 当前请求明确输入；
2. 用户确认保存的 `location.city`、`location.province`、`location.country`。

禁止从 IP、网络端点、Vault 内容或模型推断地点。天气和附近影院缺 city 时询问；允许放宽的研究任务只能按 city → province → country 执行，并以 `LocationCoverage` 记录和披露。

补充信息遵循统一的硬前置条件判断：只有缺少字段就无法唯一、安全执行时才暂停同一 Run。宽泛的“最近有什么新上映的电影”“推荐新片”等属于区域可披露的研究任务，不要求 city，且不得暴露依赖本地排片的结构化 `now_playing` Provider 工具；“附近影院、排片、场次、几点、票价、购票”等明确本地可用性请求才要求 city。已提交字段是 Run 的结构化事实，恢复时不得再用有限实体词表从自然语言中重新识别。

标的、赛事、频道和同名实体歧义遵循相同原则：先澄清或在证据中完成唯一消歧，不能由模型默选。

## 4. 精确事实 Web 成功条件

没有结构化 binding 时，Web 并非自动失败，也不能自动成功。只有同时满足以下条件才可输出精确当前事实：

- 证据属于当前 Run，HTTPS、未 retired 且可定位；
- 对应领域必需字段全部可提取；
- 时间标签在允许窗口内；
- 地域、单位、币种、渠道、赛事或标的没有歧义；
- 事实块通过唯一来源协议绑定当前 Run 证据：Web 使用 Run-local `Wn`，结构化外部工具使用 `E{ledger_id}`；会话 `[Cn]`、裸 ledger ID 和 `SourceGroupFallback` 不能作为终局来源；
- 来源冲突已解决，或在回答中明确保留冲突；
- 最终文本没有引入证据字段之外的新实体、数字、日期或状态。

## 5. 研究型回答合同

- 每个“最新、当前、正在、即将、近期”等时效结论必须有邻近引用。
- 推荐理由中的可用性、价格、排期和状态仍按精确事实合同处理。
- 原因解释可以是基于证据的推断，但必须明确为分析，并区分来源事实与模型综合。
- 用户未要求多来源且风险低时，不为凑数量强制第二来源；冲突或高风险时使用 `MissingIndependentSource`。
- 证据不充分时允许给出检索到的有限范围和下一步建议，不允许把旧事实包装成当前结论。

上述门禁确定性验证来源格式、当前 Run 归属、结构化字段、时效和每块覆盖；它不等于自由文本语义蕴含判断。模型是否把来源内容准确综合为自然语言仍由评测衡量，不能标记为 NLI VERIFIED。

## 6. 金融安全边界

允许事实、新闻、基础指标和基于已支持数据的描述性比较。禁止个性化买卖、仓位、目标价、自动交易和收益保证。缺少 `asOf`、币种、标的或 delay 时不得输出伪精确数字。
