# Iris Agent Harness 现行建设文档

> **文档状态**：现行
> **文档类型**：入口与索引
> **事实基线**：2026-09-01，审计起点 `e30f47d1`

本目录是 Iris Agent Harness 的唯一现行建设入口，分别记录代码当前事实、长期边界、目标合同、重构阶段和验收证据。它不建立第二份产品路线图，也不把目标设计冒充为已经部署的能力。

## 本次事实重置

2026-08-24 至 09-01 的真实交互连续暴露出地点补充后无回答、回答正文与 Run 投影脱节、合法联网回答被来源协议误拒绝、搜索摘要被当作正文证据、单轮搜索挤满证据容量，以及 Provider 空响应被误报为“当前运行能力无法处理”等缺陷。代码复核进一步确认：旧 fixture 主要验证管线连通性，既没有要求模型选取并抓取来源正文，也允许单条匿名路由冒充真实产品质量。

因此，本体系撤回此前把旧 AH-2/AH-3 写成已验证终局的结论。旧实现仍作为“当前存在的代码”记录，但不再作为目标架构。新施工阶段统一使用 `HR-*` 编号，避免与历史判断混淆。

## 权威边界

- 版本、里程碑和产品范围以 [`ROADMAP.md`](../ROADMAP.md) 为唯一来源。
- 已部署模块、数据流和兼容边界以 [`ARCHITECTURE.md`](../ARCHITECTURE.md) 与当前代码为准。
- 本目录负责 Harness 的问题审计、目标合同、处置决定、阶段依赖和验收证据。
- `CHANGELOG.md` 只记录已交付变化；规划和局部测试不得进入交付事实。
- 历史通过 Git 追溯；统一前材料仅可从[受控归档入口](archive/2026-08-pre-unification/MANIFEST.md)访问，不构成现行规范。

## 统一方向

1. 复用现有 Provider-neutral `AgentToolLoop`，建立一个适用于本地检索、Web、runtime 和外部只读工具的通用有界循环。
2. 模型负责理解问题、选择工具、调整查询、判断语义缺口和组织回答；Host 负责授权、预算、状态、来源身份、持久化和副作用。
3. Intake 只冻结 `AgentIntent + Effect + ContextMode + Freshness + Effort + RiskClass + CapabilityId`，不再把领域分类当作执行骨架。
4. 普通事实使用 `WebPreferred`；只有明确联网、指定 URL、强时效或高风险当前事实使用 `WebRequired`。
5. 结构化工具调用协议继续保留；11 个领域 operation 退出核心路由，真实需求可通过统一工具目录作为可选 Provider 适配器接入。
6. 普通回答自然完成，普通缺参自然追问；结构化终局和暂停状态只用于真正需要确定性合同的任务。
7. 写入由模型提出、Host 冻结、用户确认和确定性执行；模型不能把一次确认扩展成开放写权限。
8. 每个阶段必须同步删除被替代分支、测试和文档；只增加抽象而没有净简化的阶段不得结案。
9. 真实质量门禁必须区分确定性合同、两条独立真实路由和人工答案评分；任一层缺失时 `agent:eval` 返回非零。

## 阅读顺序

1. [`01-authority-and-invariants.md`](01-authority-and-invariants.md)：不可违反的产品与运行边界。
2. [`02-current-state-and-debt.md`](02-current-state-and-debt.md)：代码当前具备什么、为什么仍然失败。
3. [`03-target-architecture.md`](03-target-architecture.md)：统一后的职责和数据流。
4. [`04-adaptive-agent-loop-and-tool-contracts.md`](04-adaptive-agent-loop-and-tool-contracts.md)：通用多轮工具与回答合同。
5. [`05-implementation-roadmap.md`](05-implementation-roadmap.md)：`HR-0` 至 `HR-7` 的依赖、删除项和退出条件。
6. [`06-evaluation-performance-and-acceptance.md`](06-evaluation-performance-and-acceptance.md)：如何证明管线正确、回答有用且没有越权。

附录分别提供[状态与证据追踪](appendices/A-status-and-test-traceability.md)、[任务能力与风险矩阵](appendices/B-task-capability-and-risk-matrix.md)和[现行/撤回决策](appendices/C-decisions-and-deferred.md)。

## 状态语言

| 维度     | 允许值                               | 含义                                                         |
| -------- | ------------------------------------ | ------------------------------------------------------------ |
| 当前状态 | 已验证、部分实现、已知缺陷、未配置   | 只描述当前代码和当前证据                                     |
| 目标处置 | 保留、重构、退出核心、兼容读取、延期 | 只描述目标方向                                               |
| 阶段状态 | 未开始、进行中、阻塞、已验收         | 只有满足命名退出条件后才能升级                               |
| 证据类型 | 代码、命名测试、生产复现、真实试点   | fixture 只能证明被覆盖的合同，不能自动覆盖生产反例或语义质量 |

生产复现与现行测试冲突时，能力状态必须降级并补充回归；测试数量不得代替具体能力证据。
