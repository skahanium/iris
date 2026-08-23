# 04. 研究与工具合同

## 1. `web_search` 单一网络工具合同

继续使用现有模型可见工具名 `web_search`，不增加独立 fetch 工具。目标输入保持向后兼容：

```text
query: string?       搜索查询；首轮必需，后续可调整
gap: EvidenceGap?   首轮之后继续研究的明确原因
urls: string[]?     用户明确提供或当前 Run 已登记的 URL
```

调用规则：

1. 首轮只能使用用户问题、可信 runtime 时间、明确地点和非敏感上下文生成查询。
2. 首轮之后必须提供仍未解决的 `gap`；没有缺口不得继续搜索。
3. `urls` 只能包含用户本轮明确给出的 URL，或已由当前 Run 搜索结果登记的 URL。
4. 提交 `urls` 表示请求 Host 深入抓取这些页面，不表示模型取得任意网络访问能力。
5. Host 对 query 规范化去重，对 URL canonicalize、去重并执行 HTTPS/SSRF/redirect/size/content-type 检查。
6. 工具返回只包含受限标题、摘要、抓取摘录、时间标签和 Iris evidence ID，不返回凭证、原始 transport 配置或不受限正文。

旧 `web.fetch`、`fetch_web_page` 等名称只允许留在内部 adapter 或历史数据兼容层，不重新进入模型 surface。

## 2. 证据缺口

复用以下闭集；新增成员必须同时更新 planner、schema、恢复序列化、测试和本文件：

```text
MissingEntity
MissingLocation
LocationCoverage
MissingTimestamp
StaleObservation
MissingUnit
MissingChannel
MissingIndependentSource
SourceConflict
```

- 同一规范化查询即使换一个 gap 也不能重复执行。
- `MissingIndependentSource` 只用于用户要求核实、来源冲突或高风险描述性分析。
- `LocationCoverage` 只能按已定义地域顺序放宽，并在回答中披露最终范围。
- `SourceConflict` 在预算内无法解决时必须保留冲突说明。

## 3. 自适应研究档位

| 档位     | 默认用途             | 搜索上限 | 抓取上限 | 模型续接上限 | 证据上限 | 硬时限 |
| -------- | -------------------- | -------: | -------: | -----------: | -------: | -----: |
| Quick    | runtime、简单单事实  |        1 |        2 |            2 |        4 |  20 秒 |
| Standard | 默认时效研究         |        3 |        6 |            4 |        8 |  45 秒 |
| Deep     | 用户明确要求深入研究 |        5 |       10 |            6 |       12 |  90 秒 |

共同硬上限仍为 8 个模型轮次、24 次工具调用和 32K Web evidence packet。单轮抓取并发最多 3 个。

档位选择：

- runtime 事实无需 Web；简单、单实体且低歧义的事实使用 Quick。
- 需要比较、推荐、原因解释、多来源或首次证据不足的任务默认 Standard。
- 只有用户明确使用“深入研究”等请求或 UI 明确选择时使用 Deep。
- 模型可以在档位内选择研究重点，不能升级档位或扩大任何计数。

提前停止条件：

- 必需字段、时效、地域和来源已经满足；
- 已达到质量合同需要的独立来源数；
- 连续两轮没有新增有效 evidence ID；
- query/URL 去重后没有新动作；
- 确定性首轮预取、模型调用与模型工具调用共享的 profile deadline 耗尽；
- 用户取消、权限撤销或 provider snapshot 漂移。

## 4. 结构化快路径与 Web 路径

路由按问题形态，而非简单按领域：

| 问题形态                                 | 首选路径             | 无结构化 binding 时                        |
| ---------------------------------------- | -------------------- | ------------------------------------------ |
| 本机日期、时间、时区                     | trusted runtime      | 不联网                                     |
| 精确天气观测、报价、比分、明确排期       | 已验证结构化 binding | Web 能满足相同字段合同时继续，否则失败关闭 |
| 新闻综述、市场原因、比赛前瞻、推荐与比较 | Web research         | 继续有界研究                               |
| 高风险精确结论                           | 结构化或多证据 Web   | 缺时点、单位、地域或来源时拒绝伪精确值     |

结构化输出必须经过白名单 mapping、DTO validator、Iris evidence registration 和 deterministic finalization。普通 Web 不能伪装成结构化 Provider，但可以在满足相同最终事实合同时支持答案。

## 5. 模型协议适配

- 已验证支持工具续接：模型阅读当前 evidence packet，选择下一 query、gap 或 current-Run URLs。
- chat-only/不支持续接：Host 执行对应档位的有限预取，模型只做一次综合；不得伪造多轮研究状态。
- 工具协议失败：保留已登记证据，按剩余预算决定安全重试或降级；不得切换到未冻结 Provider。
- 不通过模型自述判断能力，不保存“强模型/弱模型”标签。

## 6. 失败语义

- Web 关闭或 classified/local-only：不执行外部请求，返回权限或能力不足。
- URL 不属于用户输入或当前 Run：拒绝并记录稳定安全码，不回显 URL 全文。
- 证据字段不足：`agent_run_fresh_evidence_insufficient`，说明缺失类型而不补写事实。
- 结构化 Provider 不可用：允许转入符合任务形态的 Web research；只有需要伪精确字段且 Web 也不足时失败。
- 最终化协议不可用：`agent_run_grounded_finalization_unavailable`，不得退化成无约束猜测。
- deadline 或预算耗尽：使用已支持的部分事实并披露限制，或在无法安全回答时失败关闭。

## 7. 金融与其他高风险边界

金融能力只提供带 instrument、currency、`asOf`、delay 和来源的事实或描述性比较，不提供个性化买卖、仓位、目标价、自动交易或未来收益保证。医疗、法律和外部交易不因通用 Web loop 自动进入支持范围。
