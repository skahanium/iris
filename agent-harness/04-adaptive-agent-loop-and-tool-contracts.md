# 04. 自适应 Agent 循环与工具合同

> **文档状态**：现行
> **文档类型**：目标合同
> **事实基线**：2026-08-27，审计提交 `6c5dbd40`

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

## 4. 重复、进展与收束

- 工具 fingerprint 由 tool name 和规范化参数组成；成功 fingerprint 不再执行。
- 相同失败 fingerprint 最多执行两次，且必须受总预算约束。
- 进展只使用安全、可比较的事实：新 evidence/resource ID、新 canonical URL、新内容 hash、新 revision 或目标文件 hash 变化。
- 不同查询返回相同资源和内容不算新进展。
- 连续两个完整模型—工具回合没有新进展时，Host 关闭工具面并发出一次通用综合指令。
- 探索预算即将耗尽时同样关闭工具，保留最后一次模型轮次；不得先把全部轮次消耗完再返回 `ToolLoopLimit`。
- 强制综合后仍没有可见正文、发生权限越界或严格证据要求未满足，才进入失败终态。

## 5. 工具输入与结果

每个模型可见工具继续使用稳定名称和 JSON Schema。工具结果至少包含：

- success/error 和稳定错误码；
- 有界、已净化的模型可见内容；
- 资源类型与当前 Run 内安全标识；
- 可用时的标题、来源、时间、范围、截断状态和 revision；
- 供 Host 内部计算进展的 canonical identity，不进入日志或用户正文。

错误必须可行动，例如“无结果”“查询过宽”“资源已变化”“权限不足”“网络暂不可用”，不能只返回“工具失败”。原始 Provider 输出、凭证和不受限正文永不进入 transcript、事件或审计。

## 6. Web 与本地工具

- `web_search` 继续作为核心模型可见网络入口；Host 可在同一受控实现中搜索和抓取 current-Run URL。
- 本地工具保持 `search_hybrid`、`search_semantic`、`search_keyword`、`read_note`、`get_outline`、`get_backlinks` 等正交能力。
- Web 和本地结果进入同一 evidence ledger，但保持不同权限、内容泄漏和来源展示策略。
- 模型可以在同一 Run 中交替使用本地和 Web 工具；Web query 只能来自用户公开子句和可信 runtime，不能包含自动检索笔记正文。

## 7. 结构化工具与可选适配器

结构化调用协议必须保留，因为它承担参数校验、授权、预算、审计和 Provider 中立。领域 operation 不再承担核心路由：

- 新 Run 的默认工具面不因 `FreshFactDomain` 自动暴露 weather/news/finance/entertainment/sports 工具。
- 有真实需求的精确数据源通过现有 catalog、MCP snapshot 和 capability 作为可选只读工具接入。
- 可选工具只返回 typed result，不创建领域 planner、独立 Run 状态或独立 finalization。
- migration 072、旧 mapping 和旧 envelope 保留读取兼容；未配置 Provider 不影响普通 Web 或本地任务。

## 8. 回答与来源

- 普通回答：自然正文，可附受控来源区；不要求 `submit_final_answer`。
- WebPreferred：有证据时展示当前 Run 来源；无证据时可以基于模型知识回答并说明时效限制。
- WebRequired、CitationCheck 和高风险当前事实：必须满足当前 Run evidence 要求，必要时使用结构化终局。
- `ProvenancePolicy` 统一解析 `Wn`、`E{id}`、`L{id}`、`Mn`；`[Cn]` 和数据库裸 ID 只用于内部或展示。
- Harness 校验来源存在、归属、时效和声明的覆盖关系，不宣称完成自由文本 NLI。

## 9. 澄清与暂停

普通缺少地点、范围、偏好或对象时，模型以自然 assistant 消息追问并完成当前 Run。下一条用户消息作为新 Run，通过已提交会话历史承接。

同 Run 暂停只用于：

- 冻结写入确认；
- 明确外部授权；
- 必须绑定原事务且不可安全重放的人工决定。

`AwaitingInput` 仅保留旧 Run 读取和安全终态兼容，新普通 Run 不再产生该状态。

## 10. 冻结变更集

DurableApply 在确认前允许模型反复使用已授权只读工具。最终变更集最多包含 6 个有序操作并影响 6 个文件，每项冻结：

- tool name 与规范化参数；
- vault 和相对路径；
- base content hash 与 expected post hash；
- tool call ID、计划 hash、过期时间和回滚摘要。

用户一次确认整个变更集。Host 执行时逐项重检授权和 hash；任何不一致停止剩余操作并以部分执行事实终态化。确认后的调度只能使用计划内目标，且只开放目标限定的 `read_note`，最多 2 次模型调用和 4 次工具调用。验证发现新修改需要时必须重新确认。

## 11. Provider 能力

Gateway 为本轮冻结 tools、continuation、parallel calls、streaming 和 structured output 能力。核心循环只消费这些协议事实，不读取模型名称：

- 支持 tools/continuation 的 Provider 使用完整循环。
- chat-only Provider 仅执行 Direct 或显示明确能力降级。
- 不稳定自定义 endpoint 不通过一次文本连通测试升级为 Agent-capable。
- 任何 Provider 都受同一权限、预算、来源和终态合同。
