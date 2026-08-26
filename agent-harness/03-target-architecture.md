# 03. 目标架构

> **文档状态**：现行
> **文档类型**：目标合同
> **事实基线**：2026-08-27，审计提交 `6c5dbd40`

## 1. 单一 Harness 数据流

```text
User Turn
  -> Request Intake
       AgentIntent + Effect + ContextMode + Freshness
       + Effort + RiskClass + CapabilityId
  -> frozen ToolSurface + RunBudgetPolicy
  -> one Run Engine
       -> Direct, or
       -> one bounded AgentToolLoop
            model <-> authorized runtime/local/Web/external-read tools
       -> optional frozen change-set confirmation
       -> bounded read-only verification
  -> one Evidence Ledger + one ProvenancePolicy
  -> durable assistant message + terminal Run state
  -> recoverable Run-local UI projection
```

Run engine、prompt compiler、tool catalog、Gateway、provider registry、evidence ledger、AgentToolLoop 和前端投影都只能有一套。

## 2. 模型与 Host 的职责

| 模型负责                     | Host 负责                                    |
| ---------------------------- | -------------------------------------------- |
| 理解问题和用户真实意图       | 冻结权限、工具表面和预算                     |
| 选择已授权工具               | 校验参数、权限、重复、取消和调用上限         |
| 判断结果是否相关、过时或冲突 | 登记资源身份、时间、Run 所有权和安全元数据   |
| 调整关键词、范围和搜索方向   | 阻止无界循环、SSRF、隐私外发和未确认副作用   |
| 综合材料并说明不确定性       | 持久化消息、来源绑定、Run 终态和恢复         |
| 提出文档变更                 | 冻结变更集、展示确认、确定性执行和 hash 复核 |

Host 不实现电影、天气、体育等语义规划器；模型也不能用自然语言声明绕过确定性门禁。

## 3. Intake 的正交决策

- `AgentIntent`：Chat、AskNotes、Research、CitationCheck、Write 等用户任务形态。
- `Effect`：Answer、Draft、Apply。
- `ContextMode`：None、Conversation、ImplicitVault、ExplicitReferences、ExplicitScope。
- `Freshness`：Offline、WebPreferred、WebRequired。
- `Effort`：Direct、ToolLoop、Durable。
- `RiskClass`：ReadOnly、BoundedWrite、Destructive、ExternalSideEffect。
- `CapabilityId`：模型、runtime、本地、Web、外部只读和写入权限。

领域、关键词和 Provider mapping 可以帮助模型选择工具，但不能产生第二套权限或完成语义。

## 4. 渐进联网语义

- `Offline`：对话、创作、转换、本地材料、classified/local-only 和可信 runtime。
- `WebPreferred`：普通外部事实、推荐、比较和一般研究。Web 可用时模型决定是否调用；无 Web 证据仍可诚实回答或说明知识限制。
- `WebRequired`：用户明确要求联网或核实、指定 URL、强时效数据以及医疗、法律、金融等高风险当前事实。缺少当前 Run 证据时不得伪造确定性结论。

联网开关只决定授权；`Freshness` 决定答案义务，两者不能互相增权。

## 5. 通用自适应工具循环

Web、本地检索、runtime 和外部只读工具全部进入同一个循环：

```text
model turn
  -> zero or more authorized tool calls
  -> sanitized bounded results
  -> model assesses relevance / coverage / conflict
  -> refine query, read another resource, or answer
  -> Host stops on success, no progress, cancellation or budget
```

Host 只用稳定资源 ID、URL、内容 hash、revision 和调用 fingerprint 判断是否有新进展，不实现语义 `EvidenceGap` 闭集。连续无进展或探索额度用尽后关闭工具，并保留最后一次综合机会。

## 6. 结构化工具的定位

结构化 tool name、JSON Schema、typed result、权限和审计仍是必要执行合同。应退出核心的是领域专用路由，而不是结构化调用本身。

- 核心提供正交工具：runtime、本地搜索/读取、Web 搜索/抓取、外部只读和确认型写入。
- 天气、报价、法规数据库等真实结构化来源可通过统一 catalog/MCP/provider adapter 接入。
- 可选适配器不增加领域 Run 状态、Intake classifier 或独立 finalization。
- migration 072 和旧 envelope 仅在读取边界兼容；新 Run 不再写入领域规划合同。

## 7. 回答、澄清与来源

- 普通回答直接返回自然正文；来源区由 Harness 根据当前 Run ledger 投影。
- 用户明确 CitationCheck 或严格事实才要求结构化来源覆盖。
- 普通缺参由模型自然追问并完成当前 Run，不使用持久化输入事务。
- 来源 ID 仍由 `ProvenancePolicy` 解释；来源归属错误不能通过语言修复掩盖。
- 无进展、WebPreferred 降级和部分材料不足优先形成有用回答并披露限制，而不是默认失败。

## 8. 有界写入

模型可在确认前多轮读取和规划，随后形成一个有序冻结变更集：最多 6 个操作、6 个文件。确认后 Host 按 hash 绑定执行；成功后只允许最多 2 次模型调用和 4 次目标限定的本地只读验证。任何新写入都必须重新确认。

该能力扩展现有 `FrozenChangePlan`，不增加数据库表、第二写入引擎或开放式文件权限。

## 9. Provider 中立与降级

Gateway 冻结 Provider 是否支持 tools、continuation、parallel calls、streaming 和结构化输出。核心不按模型名称硬编码行为：

- 工具能力完整：进入相同通用循环。
- 不支持 continuation：Direct 或明确降级，不能伪造多轮研究。
- 协议中途漂移：保留已提交事实，返回稳定能力错误，不切换未冻结权限。

## 10. 禁止的平行架构

- Web 专用研究引擎或领域专用 Run 状态机；
- 第二工具目录、provider registry、evidence store 或 finalization 解释器；
- 按模型名称维护长期强弱表；
- 普通澄清专用事务系统；
- 未经一次统一确认的连续写入；
- 新能力长期叠加在旧分支上而不做删除。
