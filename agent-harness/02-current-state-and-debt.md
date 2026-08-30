# 02. 当前状态、生产缺陷与技术债

> **文档状态**：现行
> **文档类型**：当前事实审计
> **事实基线**：2026-08-27，审计提交 `6c5dbd40`

本文件只描述基线提交上的代码和证据，不描述目标已经落地。生产交互与测试结论冲突时，以“存在未覆盖缺陷”记录，而不是用测试数量覆盖问题。

## 1. 可保留的可靠基础

| 能力                                     | 当前状态 | 处置       | 事实依据                                       |
| ---------------------------------------- | -------- | ---------- | ---------------------------------------------- |
| Run 幂等、单航班、durable finalization   | 已验证   | 保留       | 持久化 Run、指纹、终态事务和恢复测试存在       |
| Run-local UI 投影与迟到事件隔离          | 已验证   | 保留       | 同 Run upsert、终态正文收敛与迟到隔离回归存在  |
| 冻结工具表面、权限门禁与工具审计         | 已验证   | 保留       | 每次调用重入 catalog、authorization 和 audit   |
| Provider-neutral `AgentToolLoop`         | 部分实现 | 重构为核心 | 已支持多轮、多工具、重复抑制和全局 8/24 上限   |
| Run-local evidence 与 `ProvenancePolicy` | 已验证   | 保留并简化 | W/E/L/M 单一解释器；普通自然回答使用受控来源组 |
| `system_time_now` 等可信 runtime 工具    | 已验证   | 保留       | 本机事实可不联网完成                           |
| 冻结变更集、确认、hash 复核与限定验证    | 已验证   | 保留       | HR-5 已支持有序变更集、前缀恢复和受限只读验证  |
| migration 072 与领域旧 Run 读取          | 已实现   | 兼容读取   | 不能删除迁移或要求用户重建数据库               |

## 2. 已复现的生产问题

2026-08-24 至 26 日的对话验收至少暴露了四类问题：

1. 用户提交地点后 Run 状态发生变化，但回答没有进入当前会话投影；状态机修复没有覆盖 UI 恢复路径。
2. 宽泛推荐被强制索取地点，补充输入通过侧栏顶部或专用卡片接管自然对话，领域规则替代了模型澄清。
3. 工具已经返回证据、正文也已生成，最终却显示“来源归因协议”错误；曾确认存在 Run-local `W1`、会话 `[C1]` 和全局 evidence ID 的偶合盲区。
4. 回答能够显示并带引用，但把年度片单、未定档作品、流媒体预告和院线上映混在一起；现有 fixture 证明工具链连通，却没有证明真正回答了用户的问题。

这些现象不是电影领域的独立缺陷，而是 Intake、工具循环、最终化、投影和评测职责分裂的共同结果。

## 3. 系统性根因

### 3.1 HR-2 已收敛 Intake，但普通最终化仍待处理

[`run_intake.rs`](../src-tauri/src/ai_runtime/run_intake.rs) 已在新 Normal Run 中移除“未命中排除项即 `StrictExternalFact`”的终局分支：普通外部问题在 Web 开启时冻结为 `WebPreferred + None`，Web 关闭时为 `Offline + None`；明确联网或 URL、强时效和高风险当前事实才冻结为 `WebRequired + CurrentRunWeb`。Web 开关仍是唯一能写入 `web.search` 的授权事实。

同时，新 envelope 一律写入空的兼容 `FreshFactPolicy`，不再冻结 `web.domain.read`、领域 operation、时间窗口或城市输入。旧 Run 的非空领域字段只能用于兼容读取和恢复。

这一入口收敛已在 2026-08-30 由 HR-4 衔接：普通 `WebPreferred` 与普通 `WebRequired` 都不再因内部结构化终局而拒绝自然正文；当前 Run evidence 的完成门禁保持不变。

### 3.2 通用 ToolLoop 被 Web 专用控制层污染

[`agent_tool_loop.rs`](../src-tauri/src/ai_runtime/agent_tool_loop.rs) 已经是 Provider-neutral 多轮循环，支持 8 次模型调用、24 次工具调用、重复成功拒绝和失败重试。但它同时直接依赖 `ResearchBudget`、Web evidence 计数、Web deadline 和 Web 专用无进展错误。

[`run_tool_loop.rs`](../src-tauri/src/ai_runtime/run_tool_loop.rs) 进一步维护通用 Web budget、Fresh research budget、search/fetch/repair 计数和 `ResearchQueryLedger`。结果是本地检索与 Web 研究没有共享同一套进展和预算语义。

### 3.3 领域分类进入核心执行合同

`FreshFactDomain`、`DomainOperation`、`FreshFactPolicy`、五类工具表面和 11 个 operation 分布在 Intake、tool surface、dispatcher、finalization、恢复和评测中。当前代码把“工具协议存在”误当成“领域能力应当成为核心路由”。

这带来三类维护成本：

- 领域关键词和字段规则持续侵入普通对话；
- 没有真实 Provider 时仍维护 DTO、mapping、renderer 和专用测试；
- 新领域会诱导新增 classifier、operation 和完成门禁，而不是复用通用工具循环。

### 3.4 普通回答承担过强终局协议

模型可见 Run 来源号、会话展示号和 ledger ID 已经统一过一次。2026-08-30 前，普通联网回答仍经结构化 `submit_final_answer` 修复和逐块覆盖门禁，模型可能已经给出可用自然回答，却因格式、引用粒度或 Provider 工具续接差异被改为失败终态；HR-4 已将该门禁收窄到高风险与历史严格合同。

确定性校验可以证明来源归属、时效字段和引用存在，不能证明自然语言每个结论都得到语义支持。当前文档和测试曾夸大这一能力。

### 3.5 澄清状态代替自然对话

`AwaitingInput` 曾被用于城市等普通字段收集，并要求同 Run 恢复。即使状态恢复可靠，这仍让普通澄清承担事务恢复、历史投影、卡片位置和幂等提交等额外复杂度。HR-4 已使新普通 Run 只接受“未调用工具、单条无来源问题”的自然澄清终态；只有真正不可重复的事务输入才需要暂停同一 Run。

### 3.6 写入曾只支持单操作（HR-5 已关闭）

基线时的 `FrozenChangePlan` 只能冻结一个 operation：模型第一次调用确认型工具后，Run 立即进入 `AwaitingConfirmation`，确认后 Host 只执行该操作，所有 budget profile 的 `post_confirmation_max_model_turns` 都是 0。

HR-5 已保留该格式的兼容读取，并新增有序、至多六项的冻结变更集、逐项 base hash 重验、前缀恢复和仅限已执行目标的 `2` 次模型 / `4` 次 `read_note` 验证。实现与命名证据见 [`05-implementation-roadmap.md`](05-implementation-roadmap.md#8-hr-5冻结变更集)。

### 3.7 评测证明了错误的事情

现有 deterministic matrix 和命名测试覆盖 Run、工具、恢复、来源和安全边界，价值应当保留。但部分场景预编排模型调用，只断言使用了 `W1` 并完成，没有评估：

- 首轮结果差时是否调整查询；
- 回答是否区分时间、地域、渠道和状态；
- 引用内容是否真的支持推荐；
- 无进展时是否仍能生成诚实、有用的回答；
- 不同工具能力 Provider 是否得到一致的降级结果。

成熟数据库中 `W1` 与全局 evidence ID 不同后暴露的假通过，证明 fixture 必须主动破坏 ID 偶合和理想调用顺序。

## 4. 当前结论

- 不需要新建第二 Agent 系统；现有 Run、权限、catalog、Gateway、ledger 和通用 ToolLoop 是正确基础。
- 需要删除的是 Web/领域专用平行控制层和普通回答上的过强门禁，而不是删除多轮工具能力。
- 11 个领域 operation 当前确实存在，但目标处置是退出核心；是否保留某个真实 Provider 适配器必须由实际需求和 PDR 单独决定。
- 真实 Provider 尚未纳入本轮，任何 fixture 结果都不能写成真实模型质量结论。
