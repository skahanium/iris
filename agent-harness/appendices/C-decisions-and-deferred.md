# 附录 C：现行决策、撤回决策与延期项

> **文档状态**：现行
> **文档类型**：架构决策记录
> **事实基线**：2026-09-03，审计起点 `e30f47d1`

决策 ID 一经使用不重新赋义。被生产证据推翻的决定进入撤回表，避免在 Git 历史和现行文档之间产生两个“当前结论”。

## 1. 现行决策

| ID      | 决策                                  | 理由                                                               |
| ------- | ------------------------------------- | ------------------------------------------------------------------ |
| DEC-010 | `agent-harness/` 继续作为唯一现行入口 | 防止文档双轨；历史通过 Git 和唯一归档入口追溯                      |
| DEC-011 | 采用受约束的模型自主性                | 模型更适合语义规划；Host 继续掌握确定性边界                        |
| DEC-012 | 现有 `AgentToolLoop` 成为唯一多轮循环 | 已支持 Provider-neutral 多轮工具，无需第二研究引擎                 |
| DEC-013 | Intake 使用正交任务轴                 | AgentIntent/Effect/Context/Freshness/Effort/Risk/Capability 已存在 |
| DEC-014 | 普通外部事实默认 WebPreferred         | 避免把普通问答升级为严格当前事实事务                               |
| DEC-015 | 使用总预算 + `cost_class` 分类预算    | 复用 RunBudgetPolicy 和 catalog，不引入领域 planner                |
| DEC-016 | 连续无进展后强制综合                  | 有限材料仍可形成诚实回答，不应默认红色失败                         |
| DEC-017 | 普通回答不强制结构化终局              | 来源协议不能成为自然问答的普遍可用性门槛                           |
| DEC-018 | 普通缺参使用自然对话                  | 减少 AwaitingInput、恢复、投影和卡片耦合                           |
| DEC-019 | 写入采用一次确认的有界变更集          | 同时支持多步修改和确定性授权边界                                   |
| DEC-020 | 领域 operation 退出核心               | 结构化协议保留，真实 Provider 作为可选工具接入                     |
| DEC-021 | Provider 差异只在 Gateway 适配        | 禁止 MiniMax 或其他模型专用核心分支                                |
| DEC-022 | `ProvenancePolicy` 是唯一来源解释器   | 防止 W/C/ledger ID 再次分裂                                        |
| DEC-023 | 发现调用采用每轮最多 2 个的有界批次   | 保留独立方向并行，同时保证模型能先观察再做依赖动作                 |
| DEC-024 | Web 候选与最终 evidence 分层          | 标题片段不能占满来源容量或冒充正文支持                             |
| DEC-025 | 无输出先同路由重试再等能力切换        | 瞬态/协议故障不应误报能力不足；已有动作后禁止跨路由暗接            |
| DEC-026 | 产品门要求两条 live 路由与人工评分    | fixture、单路由和自动引用检查都不能证明真实回答质量                |
| DEC-027 | 强时效最低取证是 Host observation     | 只保证真实搜索/抓取起步，不替模型规划后续研究，也不建立第二个循环  |
| DEC-028 | 仅实际派发调用锁定 Provider           | 无效或延迟提议没有可续接的 canonical tool transcript，安全可切换   |

## 2. 已撤回决策

| 原 ID/方向          | 撤回内容                                    | 生产或代码依据                                       | 替代        |
| ------------------- | ------------------------------------------- | ---------------------------------------------------- | ----------- |
| 旧 DEC-002          | 撤回 Web 专用多轮 research 作为通用事实骨架 | 通用循环被 ResearchBudget/EvidenceGap 反向污染       | DEC-012/015 |
| 旧 DEC-004          | 撤回 11 个 operation 作为核心精确事实快路径 | 无真实 Provider 仍侵入 Intake、surface、finalization | DEC-020     |
| 旧 DEC-005          | 撤回“任务形态 + 领域字段合同”双重核心路由   | 领域规则继续决定缺参和完成                           | DEC-013     |
| 旧 DEC-006          | 撤回 Quick/Standard/Deep Web 专用 profile   | 与现有 RunBudgetPolicy 并存且只约束 Web              | DEC-015     |
| 旧 strict-fact 路线 | 撤回所有非排除事实必须当前 Run Web 证据     | 普通推荐和问答被误拒绝                               | DEC-014/017 |
| 旧 EvidenceGap 闭集 | 撤回模型后续研究必须映射九种 gap            | 语义缺口不应由 Host 枚举，且本地工具无法复用         | DEC-011/016 |
| 旧普通补充输入      | 撤回城市等普通缺参暂停同一 Run              | 状态机和 UI 复杂，且对自然对话不必要                 | DEC-018     |

撤回不表示立即删除持久化字段。删除顺序由 HR 路线控制，兼容读取与新写入停止必须分开实施。

## 3. 明确保留

- Run engine、durable finalization、幂等和恢复；
- 冻结 capability、工具表面、权限门禁和审计；
- Provider Gateway 和现有 MCP snapshot；
- 单一 evidence ledger 与 `ProvenancePolicy`；
- 结构化工具名称、JSON Schema、typed result 和确认协议；
- migration 072 及旧 Run/旧 mapping 读取兼容；
- Markdown 写入的用户确认和内容 hash 复核；
- classified、local-only、Web 开关和本地内容防外泄边界。

## 4. 延期项

以下能力不进入 HR-0 至 HR-6，只有真实重复需求、评测、隐私边界和删除方案齐备后才能立项：

- 11/11 结构化 Provider 覆盖或 readiness 管理中心；
- 通用 REST adapter 与跨工具 Provider 健康平台；模型 Gateway 只保留同能力、无可见动作前的有界 failover；
- 自由文本 NLI/LLM judge 作为生产 VERIFIED 门禁；
- 通用浏览器自动化、登录态操作和表单提交；
- 跨会话语义记忆中心；
- 第三方工具市场、任意代码执行和通用多 Agent 平台；
- 个性化金融买卖、自动交易和其他高风险外部写入；
- 超出当前两条路由、12 Run 上限的真实 Provider 扩展试点。

## 5. Provider Decision Record

只有准备接入一个真实结构化 Provider 时才创建 PDR，必须在编码前确认：

- 真实用户任务和为什么通用 Web/local/external 工具不足；
- 工具名、capability、Schema、typed result 和现有 catalog 接入方式；
- Provider、协议、许可、ToS、成本、限流、时效、地域和失败模式；
- 配置 hash、撤销、隐私、日志和安全预览边界；
- 没有 Provider、部分覆盖、协议漂移和删除时的行为；
- 是否需要依赖、schema 或 IPC，以及为什么不能复用现有边界。

PDR 不得创建领域 Run 状态、第二 registry、第二 evidence store 或模型专用路由。

## 6. 决策变更规则

变更现行决策必须：

1. 提供当前代码、命名评测或生产复现；
2. 比较安全、质量、复杂度、兼容和迁移成本；
3. 明确替代方案会删除什么；
4. 影响版本范围时先更新 `ROADMAP.md`；
5. 同步目标架构、路线、验收、状态账本和文档事实检查；
6. 原决策移入撤回表，不直接改写为相反含义。
