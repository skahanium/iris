# Iris AI Agent 体系全阶段完善总指引

## 总目标

Iris 的 AI agent 体系要从“AI 面板 + 工具调用 + 局部安全策略”升级为“面向个人高价值知识工作的可信 agent runtime”。核心场景是长对话、重要文稿写作、行业研究分析、复杂问题拆解、可恢复执行、可审计工具协作和本地优先安全边界。

最终系统必须回答清楚八件事：当前目标是什么、已经知道什么、还缺什么证据、为什么采取这个步骤、哪些结论可追溯、哪些动作需要授权、任务如何恢复、agent 如何安全写入用户笔记。

## 总体施工路线

### 阶段 0：基线审计与架构定标

目标：把当前系统真实状态固化成可验证基线，避免后续边修边猜。

工作内容：

- 建立 AI agent 体系审计矩阵，覆盖长对话、复杂推理、subagent、工具、skills、检索、文件权限、agent 权限、沙箱、前端协作状态。
- 把 DeepSeek、MIMO、人工复核的问题归并为统一问题库，标注“成立、部分成立、不成立、需实验验证”。
- 为 `run.rs`、`tool_policy.rs`、`agent_permissions.rs`、`harness_confirm.rs`、`model_gateway`、`skills`、`retrieval_broker` 建立架构 contract tests。
- 明确哪些能力属于个人项目必要实体，哪些属于平台级非目标。

产物：

- `docs/superpowers/specs/ai-agent-system-remediation.md`
- 问题矩阵
- 基线 contract tests
- 后续阶段的验收清单

### 阶段 1：止血修复与协议正确性

目标：先让现有系统不撒谎、不低估、不误导、不乱码、不静默失败。

工作内容：

- 修复 token fallback 严重低估。
- 修复 Anthropic agent tool use 和 streaming 主链路。
- 修复工具确认失败乱码。
- 写入 `session_messages.content_hash`。
- 禁止 `retract_messages(from_seq <= 0)`。
- 删除或废弃 `ToolRegistry::execute_tool()` 存根。
- 修正 `rendered_fetch` 名实不符。
- Git skill install 禁 hooks、禁 smudge filters。
- 检索层区分“索引未准备好”和“真实错误”。

覆盖维度：

- 长对话基础可靠性
- 工具调度正确性
- 模型兼容性
- skills 安装安全
- 网络检索可观测性

验收：

- 不返回 usage 的模型不会绕过预算。
- Anthropic 可完成基本 agent tool use。
- 用户看不到乱码确认错误。
- 会话消息 hash 可追踪。
- 不存在明显误导性 sandbox/cert/rendered 声明。

### 阶段 2：统一 Tool + Permission + Audit 主链路

目标：解决当前最大结构问题，即工具策略、权限原子、确认流、审计、dispatch 分裂。

工作内容：

- 新增 `ToolExecutionPipeline`，成为所有 agent 工具执行唯一入口。
- 新增 `PermissionDecisionEngine`，统一解释 tool access level、permission atom、skill capability、用户确认、resume preflight 和 audit。
- `run.rs` 与 `harness_confirm.rs` 不再直接散落执行 policy、dispatch、audit。
- 所有工具执行固定经过：catalog → schema validate → tool policy → permission decision → confirmation → sandbox profile → dispatch → retry/timeout → audit → evidence ingest。
- `permissionEffects` 不再只是 UI 展示，而是执行决策摘要。
- approve 默认只授权本次 tool call；高风险 capability 不因 skill 激活自动放开。

覆盖维度：

- 工具调度能力
- agent 权限安全性
- 文件读写权限
- skills capability 控制
- 审计一致性

验收：

- 任意工具执行都能证明经过同一 pipeline。
- `supported: false` permission 永远无法执行。
- reject/approve/deny/timeout 都有审计。
- resume 和主执行使用同一权限语义。

### 阶段 3：Conversation Memory 与长对话能力

目标：把“SQLite 存消息”升级为真正的长对话工作记忆。

工作内容：

- 新增 `ConversationMemory`。
- 新增 `conversation_summaries` migration。
- 摘要记录 `session_id`、`from_seq`、`to_seq`、`summary`、`facts_json`、`decisions_json`、`open_threads_json`、`content_hash`。
- prompt assembly 改为组合 persona、用户规则、conversation summary、active task state、recent messages、selected evidence。
- 先做确定性结构摘要，后续允许接入 LLM semantic summary。
- 长会话清理不再简单丢弃上下文，而是先 compaction 再保留可追溯摘要。

覆盖维度：

- 长对话能力
- 用户偏好延续
- 历史决策追踪
- 长期写作协作

验收：

- 50+ 轮对话后仍能复述目标、偏好、决策、未解决问题。
- summary 可追溯到原始 seq 区间。
- 删除旧消息不破坏当前任务继续能力。

### 阶段 4：复杂任务 Deliberation 与 Verification

目标：把复杂问题处理从“多轮工具循环”升级为“显式任务推理状态”。

工作内容：

- 新增 `DeliberationState`。
- checkpoint 中持久化 current goal、plan outline、assumptions、open questions、evidence gaps、verification items、failure recovery。
- `reflection.rs` 改为验证 gate，不只是让模型再想一轮。
- final answer 前必须通过 verification gate，或说明未通过原因。
- 工具失败进入 failure recovery，而不是只作为普通错误文本丢给模型。

覆盖维度：

- 长难任务深度思考解决能力
- 任务恢复能力
- 错误恢复能力
- 最终回答质量控制

验收：

- paused/resume 后不丢 plan、假设、证据缺口、验证项。
- 复杂研究能显示未验证结论和证据不足点。
- 工具失败有恢复策略，而不是直接降级为随口回答。

### 阶段 5：Writing State 与重要文稿协作

目标：专门增强重要文稿写作，而不是把写作当普通 chat。

工作内容：

- 新增 `WritingState`。
- 维护文稿目标、受众、体裁、结构大纲、核心论点、素材清单、引用来源、风格约束、修改记录、当前草稿版本。
- 写入 `.md` 前必须说明写入范围、修改理由、风险和回滚方式。
- 支持草稿版本比较、段落级修改理由、证据支撑标注。
- 写作工具输出必须区分事实补充、结构调整、表达润色、引用插入。

覆盖维度：

- 重要文稿写作
- Markdown 安全写入
- 版本与修改意图
- 用户确认体验

验收：

- agent 能连续协作一篇长文，不丢结构和风格目标。
- 每次写入都能解释为什么改。
- 能比较两个草稿版本并给出可执行修改建议。
- 不经确认不修改 `.md`。

### 阶段 6：Research State 与行业研究分析

目标：让行业研究从“搜索 + 拼接”升级为“证据驱动分析”。

工作内容：

- 新增 `ResearchState`。
- 维护研究问题、子问题、来源清单、可信度、新鲜度、证据冲突、反方观点、证据缺口、初步结论。
- 新增或强化 `EvidencePipeline`。
- 所有本地检索、网页搜索、网页抓取、法规资料、skill resource、工具结果统一成 `EvidenceItem`。
- 最终报告必须区分本地证据、网络证据、模型推断。
- citation validation 读取同一 evidence pipeline。

覆盖维度：

- 网络检索能力
- 行业研究分析
- 证据链
- 引用可靠性
- 检索错误透明度

验收：

- 行业研究输出包含问题分解、证据链、可信度、新鲜度、冲突和结论边界。
- 没有证据的判断必须标为推断。
- 每个关键结论能追踪到 EvidenceItem。

### 阶段 7：SubAgent Coordinator

目标：把 subagent 从递归 harness 调用升级为可控协作单元。

工作内容：

- 新增 `SubAgentCoordinator`。
- subagent 必须有 role、task、allowed tools、input evidence、output schema、resource locks、budget、failure behavior。
- 输出统一为 `SubagentReport`，包含 summary、findings、evidence、confidence、open questions、errors。
- 父 agent 必须合并、去重、冲突处理和质检子结果。
- 子 agent 继承父 evidence state。
- 父 request abort 级联子 request。
- 同一 note 写入需要资源锁，冲突返回 `resource_conflict`。

覆盖维度：

- subagent 调度能力
- 并发协作
- 写冲突保护
- 父子上下文隔离
- 复杂任务分工

验收：

- 多 subagent 可并发读。
- 多 subagent 写同一 note 会被阻止。
- 父 agent 能看到每个子 agent 的 confidence、errors、open questions。
- 子 agent 失败不会污染最终结论。

### 阶段 8：Skills Trust 与能力闭环

目标：让 skill 成为受信任策略约束的能力扩展，而不是 prompt 注入片段。

工作内容：

- 新增 `SkillTrustPolicy`。
- 安装时生成 `SkillTrustProfile`，记录 source、integrity hash、declared capabilities、requested tools、risk level。
- skill 的 `allowed-tools` 只能缩小工具面，不能扩大工具面。
- 高风险 capability 必须通过 PermissionDecisionEngine。
- 未锁 sha256 的 update 必须在确认 UI 中明确提示。
- skill prompt 注入内容需要长度限制、能力边界和风险标记。

覆盖维度：

- skills 安装
- skills 调度
- skill capability 安全
- skill workspace 风险
- 供应链可信度

验收：

- 高风险 skill capability 默认不能自动执行。
- registry/git/url/local 安装都有 trust profile。
- update 不会绕过完整性提示。
- skill 不能通过 allowed-tools 提权。

### 阶段 9：Sandbox Profile 分级治理

目标：诚实建立 agent 沙箱能力，而不是把应用层校验包装成强沙箱。

工作内容：

- 新增 `SandboxProfile`。
- 定义 L0 应用层边界、L1 子进程边界、L2 OS 级边界。
- process、git、skill install、未来 script capability 都必须记录 sandbox profile。
- L1 包括 cwd 固定、env 清理、timeout、stdout/stderr 限制、参数白名单、git hook/filter 禁用。
- L2 只定义接口和 unsupported 状态，不虚假声明已实现。
- 文档明确当前沙箱真实能力和限制。

覆盖维度：

- 沙箱设置及运行情况
- process 工具安全
- git 工具安全
- skill 安装安全
- 安全透明度

验收：

- 每个高风险工具都能说明运行在哪个 sandbox profile。
- 不存在“cert pinning/rendered fetch/OS sandbox 已实现”的误导表达。
- 子进程工具有 timeout、输出限制、参数限制和审计。

### 阶段 10：前端 Agent 协作体验收敛

目标：让 UI 能真实表达 agent 状态、权限、证据、任务进展，而不是状态碎片化。

工作内容：

- 拆分 `useAssistantTasks`，减少参数爆炸。
- `useAgentTaskStatus` running/paused 时轮询，terminal 状态停止。
- `useAssistantRunPlan` 必须真实渲染或删除。
- `ToolConfirmDialog` 展示 permission decision、sandbox profile、pending confirmation index、skill trust warning。
- 复杂任务面板展示 deliberation、verification、evidence gaps、blocked reason。
- 写作任务面板展示 writing state。
- 研究任务面板展示 research state 和 evidence diagnostics。

覆盖维度：

- agent 内部整体协作能力
- 用户确认体验
- 长任务可见性
- 写作/研究工作流可用性
- 前端状态一致性

验收：

- 用户能看懂 agent 为什么停下、等什么权限、缺什么证据。
- 长任务状态不是一次性读取。
- 写作和研究不再只显示普通聊天流。
- 前端不暴露 raw checkpoint、用户笔记全文、敏感字段。

## 全局验收标准

最终系统必须满足：

- 长对话：50+ 轮后仍保持目标、偏好、决策和 open threads。
- 重要文稿：能维护结构、风格、版本、修改理由和写入确认。
- 行业研究：能输出证据链、可信度、新鲜度、冲突和结论边界。
- 工具执行：所有工具经过统一 pipeline。
- 权限安全：所有权限经过统一 decision engine。
- SubAgent：有 contract、有报告、有合并、有冲突处理。
- Skills：有 trust profile，不能提权。
- 沙箱：能力分级诚实、可审计。
- 恢复：paused/resume 后不丢 deliberation、memory、evidence。
- UI：真实展示 agent 状态，而不是只显示“正在思考”。

## 全局非目标

- 不做企业级多用户权限系统。
- 不做云端 agent 服务。
- 不做完整工作流 DAG 平台。
- 不做强制容器化运行时。
- 不把用户 `.md` 内容转成专有格式。
- 不让 AI 无确认修改笔记正文。
- 不为了架构漂亮新增无调用者抽象。

## 后续施工方式

后续每个阶段都应单独形成实施计划，包含：

- 涉及文件
- 新增/修改类型
- migration
- TDD 测试
- 验证命令
- 回归风险
- 是否影响 IPC
- 是否影响 AGENTS.md 约束

所有阶段都必须服从本总指引：不是堆功能，而是让 agent 在长期协作、复杂研究、重要写作和安全执行上变得可靠。
