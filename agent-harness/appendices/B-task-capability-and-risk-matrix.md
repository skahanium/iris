# 附录 B：任务、能力与风险矩阵

> **文档状态**：现行
> **文档类型**：目标合同矩阵
> **事实基线**：2026-08-27，审计提交 `6c5dbd40`

本矩阵按任务、上下文、新鲜度、效果和风险决定工具边界，不按电影、天气等领域建立执行状态机。

## 1. 顶层任务矩阵

| 任务           | 典型 envelope                                                        | 默认工具面                       | 证据与完成                                 | 缺参/不足                        |
| -------------- | -------------------------------------------------------------------- | -------------------------------- | ------------------------------------------ | -------------------------------- |
| Chat           | Answer + Conversation + Offline + Direct + ReadOnly                  | 无                               | 自然正文                                   | 直接说明或自然追问               |
| Local Research | Answer + ImplicitVault/ExplicitScope + Offline + ToolLoop + ReadOnly | runtime + local                  | 使用当前 Run 的 L/M 来源；无材料时诚实说明 | 自然追问范围，不联网替代         |
| WebPreferred   | Answer + Conversation/None + WebPreferred + ToolLoop + ReadOnly      | runtime + Web；按需 local        | 有证据则展示来源，无证据可披露知识限制     | 强制综合或能力降级               |
| WebRequired    | Answer + scope + WebRequired + ToolLoop + ReadOnly                   | runtime + Web；显式授权 external | 当前 Run 证据为完成前置条件                | 证据不足时明确失败               |
| External Read  | Answer + ExplicitScope + Offline/Web\* + ToolLoop + ReadOnly         | 精确冻结的 external binding      | E 来源属于当前 Run                         | 缺授权时不调用                   |
| Draft          | Draft + scope + Offline/Web\* + Direct/ToolLoop + ReadOnly           | 按材料需求开放只读工具           | 返回草稿/预览，不修改 Markdown             | 自然追问目标或风格               |
| Apply          | Apply + ExplicitScope + Offline/Web\* + Durable + BoundedWrite       | 确认前只读 + 变更计划            | 冻结变更集、确认、执行、限定验证           | 目标不明时自然追问；变更必须确认 |

`Web*` 表示联网仍由用户授权和任务 Freshness 决定，Effect 本身不能增权。

## 2. Freshness 决策

| 条件                                                       | Freshness       | VerificationRequirement       |
| ---------------------------------------------------------- | --------------- | ----------------------------- |
| 问候、创作、转换、本地材料、runtime、classified/local-only | Offline         | None                          |
| 普通知识、推荐、比较、一般研究，且用户允许联网             | WebPreferred    | None                          |
| 用户明确要求搜索/核实、提供 URL、依赖最新状态              | WebRequired     | CurrentRunWeb                 |
| 当前医疗、法律、金融或合规结论                             | WebRequired     | CurrentRunWeb 或明确 external |
| 用户明确选择外部只读 binding 且问题依赖它                  | 按 Web 授权保持 | CurrentRunExternal            |

Web 开关关闭时不能外发；对于 WebPreferred 可直接诚实降级，对于 WebRequired 必须说明当前能力不足。

## 3. Effort 与预算

| Effort   | 条件                                             | 预算                                                    |
| -------- | ------------------------------------------------ | ------------------------------------------------------- |
| Direct   | 不需要工具或已装配上下文足够                     | 1 模型、0 工具                                          |
| ToolLoop | 需要搜索、读取、Web、external、vision 或多步综合 | 8 模型、24 总工具及分类上限                             |
| Durable  | 产生确认型 Markdown 变更                         | 确认前 8/24；最多 6 操作/6 文件；确认后 2 模型/4 本地读 |

模型不能自行从 Direct 升级 ToolLoop、从 ToolLoop 升级 Durable 或扩大预算。升级只能由 Intake/用户动作产生并被冻结。

## 4. 工具类别与 capability

| 类别             | 典型 capability               | 典型工具                      | 特殊边界                                  |
| ---------------- | ----------------------------- | ----------------------------- | ----------------------------------------- |
| runtime          | `runtime.read`                | 时间、应用可信状态            | 小快照，不联网                            |
| local            | `context.read` / `vault.read` | search/read/outline/backlinks | 每次读取复核文档权限                      |
| network          | `web.search`                  | `web_search`                  | HTTPS、SSRF、query taint、current-Run URL |
| external_read    | `external.read`               | 用户选定 MCP 工具             | binding/schema/provider hash 冻结         |
| confirmed_change | `note.apply_patch`            | 插入、替换和后续变更集        | 必须确认和 hash 复核                      |

`cost_class` 只用于预算和输出策略，不授予 capability。

## 5. 回答合同

| 回答类型      | 正文               | 来源                      | 失败条件                                  |
| ------------- | ------------------ | ------------------------- | ----------------------------------------- |
| 普通回答      | 自然文本           | 可选来源组/受控引用       | 无可见正文或持久化失败                    |
| WebPreferred  | 自然文本并披露时效 | 有则绑定当前 Run          | 权限/系统故障；无证据本身不必失败         |
| WebRequired   | 严格但仍自然       | 必须有当前 Run 可用证据   | 缺证据、来源越权、严格协议无效            |
| CitationCheck | 结构化覆盖结果     | 必须逐项绑定              | 引用不存在、跨 Run 或覆盖不足             |
| Apply         | 变更摘要和执行结果 | 计划 hash、目标和结果事实 | 未确认、hash 漂移；已执行前缀必须明确报告 |

普通回答不得因为模型没有调用保留的结构化终局工具而失败。

## 6. 澄清与确认

- 问题范围、偏好、地点、语言、目标笔记等普通信息通过自然 assistant 消息澄清。
- 如果模型可以在回答中披露合理默认范围，就不强制追问。
- 写入目标、外部授权和不可重复事务决定必须由 Host 形成明确确认。
- 旧 `AwaitingInput` Run 可读取和安全终态化，但新普通对话不再进入该状态。

## 7. 可选结构化 Provider

真实结构化 Provider 只有在以下条件同时满足时才进入某 Run 工具面：

1. 用户授权的 capability 与 Web/外部边界允许；
2. catalog 中存在稳定名称和 JSON Schema；
3. Provider/binding/config hash 已冻结；
4. 输出可映射为有界 typed result；
5. 不需要新的领域路由、Run 状态或 finalization。

没有 Provider 时，普通任务继续使用通用工具或诚实降级，不因缺少领域 binding 在模型前失败。
