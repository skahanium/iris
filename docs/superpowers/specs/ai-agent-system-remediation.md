# AI Agent System Remediation Baseline

本文是阶段 0：基线审计与架构定标的主规格。它不修复缺陷，不改变 IPC，不新增 migration，也不引入依赖；它只把当前 AI agent runtime 的真实状态、后续阶段边界和验收方式固化为可验证基线。

## 目标

Iris 的 AI agent 体系要从“AI 面板 + 工具调用 + 局部安全策略”升级为面向个人高价值知识工作的可信 agent runtime。阶段 0 的目标是先回答清楚当前系统已经具备什么、哪些审计问题成立、哪些能力必须后续建设、哪些平台级能力明确不做。

阶段 0 的产物：

- 本规格：`docs/superpowers/specs/ai-agent-system-remediation.md`
- 问题矩阵：`docs/audits/2026-06-20-ai-agent-issue-matrix.md`
- 基线 contract tests：`tests/ai-agent-stage0-contract.test.ts` 与 `src-tauri/tests/ai_agent_baseline_contracts.rs`

## 审计矩阵

| 维度         | 当前基线                                                                                              | 阶段 0 裁决                                      |
| ------------ | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------ |
| 长对话       | SQLite 持久化会话与有限历史压缩已存在，但没有真正 ConversationMemory。                                | 记录为阶段 3 的核心缺口。                        |
| 复杂推理     | Harness 多轮 loop、checkpoint、reflection 已存在，但没有显式 DeliberationState。                      | 记录为阶段 4 的核心缺口。                        |
| subagent     | `run.rs` 中 `spawn_subagent` 通过显式分区和 `join_all` 聚合，仍缺资源锁、结构化报告和证据继承。       | 记录为阶段 7 的重构对象。                        |
| 工具         | ToolCatalog、ToolPolicy、dispatch、audit 已存在，但执行入口和权限语义分裂。                           | 记录为阶段 2 的重构对象。                        |
| skills       | 安装、激活、prompt 注入和资源读取已存在，但 trust profile 与 capability 风险闭环不足。                | 记录为阶段 8 的重构对象。                        |
| 检索         | `retrieval_broker` 融合 FTS、vector、graph、exact、template，但错误透明度和 EvidencePipeline 仍不足。 | 记录为阶段 6 的重构对象。                        |
| 文件权限     | Vault 路径验证、`.iris`/`.classified` 过滤和原子写入已存在。                                          | 继续作为个人项目必要边界，后续接入统一权限决策。 |
| agent 权限   | `agent_permissions.rs` 定义权限原子和 preflight，但主执行门控仍以 ToolPolicy 为主。                   | 记录为阶段 2 的关键结构缺口。                    |
| 沙箱         | 目前主要是应用层边界、子进程白名单、timeout 和 env 清理，不是 OS 级沙箱。                             | 记录为阶段 9 的诚实分级目标。                    |
| 前端协作状态 | AI 面板、确认弹窗、任务状态 hook 已存在，但状态表达碎片化。                                           | 记录为阶段 10 的收敛目标。                       |

## 必要实体

后续阶段允许新增这些个人知识工作必要实体，并应保持最小可用接口：

- `ToolExecutionPipeline`：统一 catalog、schema validate、tool policy、permission decision、confirmation、sandbox profile、dispatch、audit、evidence ingest。
- `PermissionDecisionEngine`：统一解释 tool access level、permission atom、skill capability、用户确认、resume preflight 和 audit。
- `ConversationMemory`：把长对话从最近消息列表升级为可追溯摘要、事实、决策和 open threads。
- `DeliberationState`：持久化 current goal、plan outline、assumptions、open questions、evidence gaps、verification items 和 failure recovery。
- `WritingState`：维护文稿目标、受众、体裁、结构、论点、素材、引用、风格、版本和修改理由。
- `ResearchState`：维护研究问题、来源、可信度、新鲜度、冲突、反方观点、证据缺口和结论边界。
- `EvidencePipeline`：统一本地检索、网页搜索、网页抓取、skill resource 和工具结果为 `EvidenceItem`。
- `SubAgentCoordinator`：约束 role、task、allowed tools、input evidence、output schema、resource locks、budget 和 failure behavior。
- `SkillTrustPolicy`：记录 source、integrity hash、declared capabilities、requested tools、risk level，并防止 skill 提权。
- `SandboxProfile`：诚实区分 L0 应用层边界、L1 子进程边界、L2 OS 级边界与 unsupported 状态。

## 后续阶段验收清单

### 阶段 1：止血修复与协议正确性

验收：token fallback 不严重低估；Anthropic agent tool use 和 streaming 主链路可用；工具确认失败不乱码；`session_messages.content_hash` 可追踪；`retract_messages(from_seq <= 0)` 被拒绝；`ToolRegistry::execute_tool()` 存根被删除或废弃；`rendered_fetch` 名实一致；Git skill install 禁 hooks 和 smudge filters；检索层区分索引未准备好和真实错误。

### 阶段 2：统一 Tool + Permission + Audit 主链路

验收：任意工具执行都能证明经过 `ToolExecutionPipeline`；`PermissionDecisionEngine` 是主执行路径权限语义来源；`supported: false` permission 永远无法执行；approve、reject、deny、timeout 都有 audit；resume 与主执行使用同一权限语义。

### 阶段 3：Conversation Memory 与长对话能力

验收：50+ 轮后仍能复述目标、偏好、决策和未解决问题；summary 可追溯原始 seq 区间；旧消息清理不会破坏当前任务继续能力。

### 阶段 4：复杂任务 Deliberation 与 Verification

验收：paused/resume 后不丢 plan、假设、证据缺口和验证项；复杂研究显示未验证结论和证据不足点；工具失败进入 failure recovery，而不是普通错误文本。

### 阶段 5：Writing State 与重要文稿协作

验收：agent 能连续协作长文，不丢结构和风格目标；每次写入都说明范围、理由、风险和回滚方式；不经确认不修改 `.md`。

### 阶段 6：Research State 与行业研究分析

验收：行业研究输出包含问题分解、证据链、可信度、新鲜度、冲突和结论边界；没有证据的判断标为推断；关键结论可追踪到 `EvidenceItem`。

### 阶段 7：SubAgent Coordinator

验收：多个 subagent 可并发读；同一 note 写入通过资源锁避免冲突；父 agent 能看到每个 `SubagentReport` 的 confidence、errors、open questions；子 agent 失败不会污染最终结论。

### 阶段 8：Skills Trust 与能力闭环

验收：registry、git、url、local 安装都有 trust profile；高风险 skill capability 默认不能自动执行；未锁 sha256 的 update 在确认 UI 明确提示；skill 不能通过 `allowed-tools` 提权。

### 阶段 9：Sandbox Profile 分级治理

验收：每个高风险工具能说明运行在哪个 `SandboxProfile`；不存在“cert pinning/rendered fetch/OS sandbox 已实现”的误导表达；子进程工具有 timeout、输出限制、参数限制和审计。

### 阶段 10：前端 Agent 协作体验收敛

验收：用户能看懂 agent 为什么停下、等什么权限、缺什么证据；running/paused 任务状态持续刷新；写作和研究不再只显示普通聊天流；前端不暴露 raw checkpoint、用户笔记全文、API key、token、密码或其他敏感字段。

## 全局非目标

- 不做企业级多用户权限系统。
- 不做云端 agent 服务。
- 不做第三方通用插件 API、插件市场或应用内加载任意社区扩展包。
- 不做完整工作流 DAG 平台。
- 不做强制容器化运行时。
- 不把用户 `.md` 内容转成专有格式。
- 不让 AI 无确认修改笔记正文。
- 不为了架构漂亮新增无调用者抽象。

## 阶段 0 原则

- 如果审计报告与源码冲突，以当前源码和 contract test 为准。
- 阶段 0 只做定标，不混入阶段 1-10 的修复。
- 问题矩阵必须包含证据、状态、目标阶段和验收方式。
- 所有后续阶段继续遵守 AGENTS.md 的技术栈、许可、安全、TDD 和验证要求。
