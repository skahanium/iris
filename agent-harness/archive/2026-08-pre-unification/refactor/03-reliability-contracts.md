# 03. 可靠性契约

本文只定义可实现、可测试的规范性契约，不表示当前代码已经满足。实现状态以附录 A、B 和真实测试为准；类型名如需调整，必须同步修改施工计划和追踪表，不能削弱行为约束。

## 1. Run 接受与幂等契约

### 接受

持久化普通 Run 以全局唯一的 `client_request_id` 作为幂等键。接受必须是原子的，并返回：

```text
accepted: 已存在或新建的 Run 标识
is_new: 本次调用是否首次取得执行权
```

- 同 ID、同请求指纹：返回原 Run 和 `is_new=false`，调用方不得再次 spawn。
- 同 ID、不同请求指纹：返回 `agent_run_idempotency_conflict`，不得复用原 Run 或新建 Run。
- 新 ID：只有第一个成功接受者返回 `is_new=true` 并取得执行权；相同文本不会自动视为重放。
- 两个并发调用不能同时取得同一 `client_request_id` 和请求指纹的执行权。
- `session_key` 是活动顶层 Run 的单航班范围；普通会话已有活动顶层 Run 时，新 ID 返回 `agent_run_active_run_exists`，不能静默并行。

### 重试

- 对同一 retry `client_request_id` 和请求指纹的重复重试保持幂等；指纹不同必须冲突。
- 重试必须记录与原 Run 的关系，但不能复制旧 Run 的未提交副作用。
- 崩溃恢复后以同一 ID、同一指纹重放，仍只能返回原 Run，不能再次取得执行权。

## 2. 最终化与事件契约

成功路径严格遵循以下顺序：

1. 收集并校验本次输出、工具结果和证据绑定；
2. 在同一可恢复边界内持久化最终助手消息及引用元数据；
3. 持久化 Run 的 `Completed` 终态；
4. 发出 `AnswerComplete` 和最终 UI 投影。

任一步失败时：

- 不得发出虚假的完成事件；
- 已持久化的事实不得因事件 sink 失败而回滚成另一种语义；
- 前端重连后可从 Run 快照、消息和事件序列恢复到相同结果；
- 重放事件不得再次触发工具调用、写笔记或其他副作用。

用户拒绝变更确认时，终态为 `Cancelled`，reason 使用稳定机器码 `user_rejected_change`，展示文案由前端本地化。

## 3. Intake 与当前事实分类契约

Run 被接受时冻结以下输入：

- 用户请求和会话标识；
- Web 授权等用户设置快照；
- 模型能力与可用性；
- 确定性的请求分类结果；
- 当前绝对时间、语言和经确认的地点；
- 本次可见工具表面、目录版本和 provider snapshot。

`ExecutionEnvelope` 必须区分：

```text
FreshFactDomain = none | runtime | weather | news | finance |
                  entertainment | sports | generic_web
```

- `runtime`：本机日期、时间、星期、时区、应用版本和当前能力状态；默认不使用 Web。
- 五类领域：使用附录 D 的稳定操作、时间窗和地域规则。
- `generic_web`：不属于五类领域但依赖当前外部事实的问题。
- `none`：创作、变换、会话元问题或只依赖已授权本地材料的请求。

分类器只能识别请求所需事实类型，不能据此增权。Web 关闭、classified 或显式 local-only 时，外部领域仍保持不可调用，并返回与授权边界一致的安全结果。

相对时间必须在 intake 时转成绝对时间窗；执行过程中系统日期变化不重写已接受 Run 的语义。历史 envelope 缺少新字段时以 `none` 读取，不因此触发外部请求。

## 4. 工具表面与执行门禁契约

现有 `ToolSurfacePlan` 及其已解析工具列表是本次 Run 关于“模型可见且可能执行”的唯一事实，并随 Run 冻结：

- `Planned` 工具不可见、不可 dispatch；
- `HarnessOnly` 只在明确内部路径可见；
- `Dispatchable` 仍需通过用户授权、参数和确认门禁；
- `capabilities_read` 只能报告当前计划解析出的工具，不得把完整目录宣传成当前能力。

所有工具调用在 dispatch 前依次经过同一门禁：

1. 工具存在且属于当前 surface；
2. 参数符合目录 schema，未知或无消费方的字段拒绝；
3. 当前用户授权允许所需能力；
4. 输入数据流满足隐私规则；
5. 需要用户确认的副作用已取得与参数摘要匹配的确认；
6. 运行时实现状态确实可 dispatch。

任何旁路调用也必须复用该门禁。错误只返回稳定错误码与脱敏摘要，不包含密钥、笔记正文、完整网页正文或未经处理的工具参数。

## 5. 有界联网研究契约

严格 Web 不再等价于固定单次预取。研究预算由 fresh fact domain 和问题形态确定，并在 Run 接受时冻结：

| 请求形态                   | 搜索上限 | 抓取上限 | 结构化修复上限 |
| -------------------------- | -------: | -------: | -------------: |
| 明确单一事实               |        2 |        3 |              1 |
| 新闻、推荐、比较或范围检索 |        3 |        5 |              1 |

- 首批证据满足领域必需字段时必须停止，不为了耗尽预算继续搜索。
- 第二次及后续搜索必须携带明确的 `EvidenceGap`，例如缺少地域、发布日期、数据时点或独立来源；不得只换同义词重复查询。
- 查询必须包含冻结的绝对日期、语言和合法地域范围；自动检索的本地材料不得进入查询。
- 搜索、抓取和工具结果仍服从现有累计证据与字节预算；新预算是更严格的上限，不扩大旧上限。
- 达到预算、超时、服务商失败或证据冲突后不得继续模型循环；进入失败关闭或明确的不足回答。
- ToolLoop 在证据尚未充分时必须保留被授权的 Web/领域工具；“已完成一次预取”不能单独隐藏后续研究能力。

## 6. 领域工具与服务商契约

模型可见的稳定工具为：

- `system_time_now`
- `weather_lookup`
- `news_lookup`
- `finance_lookup`
- `entertainment_lookup`
- `sports_lookup`

服务商选择和字段差异对模型不可见。所有领域结果先规范化为附录 D 的 DTO，再进入 evidence ledger 和模型上下文。

Provider 选择顺序固定为：

1. 用户对该 domain operation 显式选择且当前健康的映射；
2. 当前选定 Web provider 上同 operation 的健康映射；
3. 当前只有一个健康映射时自动选择；
4. 否则使用通用 `WebEvidenceBroker`；
5. 通用 Web 也不能满足领域验证时失败关闭。

领域 MCP 映射必须经过现有只读名称/Schema 审查和用户确认，并冻结 provider/tool/schema、输入映射、输出映射、配置 hash 与凭证引用。联网总开关关闭时，即使映射存在也不能执行。普通 `external.read` 仍保持逐 Run 显式授权，不因领域自动映射而扩大。

## 7. 地域与偏好契约

地点解析优先级：

1. 当前用户消息明确地点；
2. 经确认的 global memory：`location.city`、`location.province`、`location.country`；
3. 若领域允许，按城市 → 省份 → 国家逐级放宽；
4. 若领域要求城市而城市缺失，返回 `agent_run_location_required` 并询问用户。

禁止从 IP、网络连接、Vault 内容、历史模型猜测或 provider 地址推断地点。使用了放宽范围时，答案必须显示最终地域层级；不得把全国发行信息表述为用户所在影院正在上映。

## 8. Web 证据与严格最终化契约

### 绑定模式

- `Exact`：回答片段可定位到当前 Run 的具体证据片段。
- `Normalized`：经确定性规范化后仍能建立具体证据对应关系，并记录规范化类型。
- `SourceGroupFallback`：只声明这些来源在本次检索中被获取，不声明逐句支持关系。

所有启用 Web 且产出来源的最终答案必须具有显式绑定模式。Direct、ToolLoop、恢复和降级路径遵守同一展示规则。

### 当前事实完成门

当 domain 不为 `none/runtime` 且答案包含当前外部事实时：

- 必须使用内部 `submit_final_answer` 或 Harness 的确定性模板提交；
- 每个事实块必须引用本 Run 的结构化记录或可定位证据；
- 实体、数字、日期、地域、渠道、单位和数据时点必须通过对应规则；
- 模型路由不支持所需终局协议时返回 `agent_run_grounded_finalization_unavailable`，不得降级为自由文本猜测；
- 一次结构化修复后仍缺失时返回 `agent_run_fresh_evidence_insufficient`；
- `SourceGroupFallback` 可以伴随不足说明展示检索来源，但不能把 Run 判定为已可靠回答当前事实。

没有适用结构化规则的普通自由文本仍保持 `uncalibrated`。通用 LLM/NLI 语义 judge 不进入生产完成门。

## 9. 上下文与最小记忆契约

`RunSituation` 是每次执行开始时构造的只读投影，至少包含：

- 当前请求与已提交的近期消息；
- 当前 Run 的冻结权限、fresh fact 决策和工具表面；
- 当前 Run 已产生的事件、工具结果和证据摘要；
- 仅在超出上下文预算时使用的有效会话摘要。

投影不得把未提交 UI 草稿、旧 Run 临时输出、过期摘要、第一条用户消息推断出的永久目标或未确认记忆当成当前事实。

数据库继续沿用 071 的 `(scope, key)` 唯一性；长期记忆只允许用户明确确认的短偏好。禁止自动写入 Web 内容、模型推断、敏感凭据、整段笔记或检索片段。

## 10. 前端 Run 隔离与恢复契约

- UI 状态是持久化 Run/消息事实与当前 Run presentation 的投影，不是独立真相源。
- `AssistantAnswerReveal` 必须返回其 `runId`；消费者同时比较 reveal、presentation 和活动 Run ID。
- 新 Run 接受后的同步首帧正文为空。清空不能只依赖 `useEffect`，因为 effect 运行前的 render 也必须安全。
- presentation 正常拥有内容且答案为空时，不得回退到消息行旧正文。
- 上一 Run 的 animation frame、pending presentation events 和迟到事件不得修改新 Run 行。
- 终态恢复只允许使用同 `runId` 的持久化正文；完成、失败、取消、等待确认和 capability degraded 均应恢复一致。
- 未知事件类型必须安全忽略或降级，不能导致整个会话无法打开。
- 诊断展示默认只包含工具名、阶段、稳定错误码和脱敏摘要。
