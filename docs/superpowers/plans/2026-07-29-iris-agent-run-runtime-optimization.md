# Iris Agent Run 可靠性与受控能力演进方案

## 总结

将 Iris 定位为“确定性治理的统一 Run 控制平面”，保持最小权限、显式确认、证据账本和有界执行不变。优化拆成六个可独立验收阶段：先补齐真实恢复能力和契约一致性，再治理预算、事件与代码结构，最后逐步开放只读并行子代理、白名单 MCP 和可信 Skills 语义激活。

各阶段独立提交、独立评审，不做一次性大重构。

## 架构原则

- 权限与工具授权继续由显式 UI 输入、冻结快照和确定性规则决定；LLM 不参与授权决策。
- `agent_runs` 是事务状态投影，`agent_run_events` 是追加式安全回放日志；文档不再宣称可仅靠事件重建全部 Run。
- 普通主循环继续保持 8 个模型轮次、24 个工具调用、同指纹最多 2 次；任何扩容须先通过承压评测。
- `Durable` 继续表示可确认、可检查点的写入，不承担“深度研究”含义。
- Skills 永远是 prompt-only，不能增权、安装依赖或强制模型调用工具。
- 通用 MCP 第一版只支持用户逐 Run 显式选择的只读工具，不开放写入、日历修改、消息发送等外部副作用。
- 涉密 Run、严格 Web 证据和现有 Provider failover 边界保持不变。

## 分阶段实施

### 阶段 0：契约校准与回归基线

- 先补充契约测试，固定现有 Direct、ToolLoop、严格 Web、确认写入、断流回放和涉密隔离行为。
- 更新 `ARCHITECTURE.md`、`ROADMAP.md`、IPC 和路由文档：明确事件日志不包含工具参数/原始输出，Direct/ToolLoop 不支持进程级续跑，MCP 当前只承载 Web capability mapping。
- 建立六阶段验收矩阵；任何后续阶段不得改变未授权工具面、Web 授权源、涉密持久化策略或 Markdown 写入确认流程。

### 阶段 1：补齐 Durable 恢复闭环

主要修改 `run_contract.rs`、`agent_run_repository.rs`、`run_engine.rs`、`frozen_change_plan.rs` 和确认执行入口。

- 在冻结笔记补丁时，根据基准正文、范围和 replacement 预计算并写入 hash-bound 的 `expectedPostContentHashes`。
- 将 `append_checkpoint_step` 开放给生产路径，并增加最新检查点读取；检查点只保存 confirmation ID、plan hash、阶段、前后内容 hash 和证据 ID，不保存正文、路径、工具参数或凭据。
- 确认消费后的执行阶段固定为 `approved → dispatching → applied → completed`。确认在有效期内被消费后，恢复时不再重复检查原确认 TTL，但必须重新校验策略、目标范围和内容 hash。
- 启动恢复规则：
  - Direct、ToolLoop 或无已消费确认的 Durable 中断：继续安全失败。
  - 待确认 Run：保持 `awaiting_confirmation`。
  - 已拒绝确认但未终态化：自动完成为“未修改”。
  - 当前 hash 等于 base hash：暂停并标记 `resume_available`。
  - 当前 hash 等于 expected post hash：判定写入已发生，直接补齐检查点和 Completed，禁止重复写入。
  - hash 为其他值或多目标状态不一致：标记 `manual_review_required`，禁止自动重放。
- 实现现有 `RunControlAction::Resume`：仅接受 `paused + resume_available + Durable Apply`，以 state version 做乐观并发校验，随后复用无模型的 frozen-plan executor。
- 将 Snapshot 的恢复信息改为 `RunRecoveryKind = resume_available | manual_review_required`；Rust、`src/types/ai.ts`、`src/lib/ipc.ts` 和前端恢复提示同步更新。

### 阶段 2：预算、事件代码和 Intake 治理

- 使用现有 `budget_policy_json` 持久化冻结的 `RunBudgetPolicy`：
  - `direct`：1/0，无 ChildRun。
  - `standard`：8/24，无隐式 ChildRun。
  - `delegated`：主循环仍为 8/24，最多 3 个 ChildRun。
  - `durable_apply`：确认前 8/24，确认后 0 次模型调用。
- 用 `AgentToolLoop::from_policy` 取代散落的 `default/with_limits`；ChildRun 固定为 2 个模型轮次、6 个工具调用、每轮最多 2,000 输入 token 和 1,024 输出 token，并通过 GatewayRequest 真正执行预算，而不是只把数值放在 `SubAgentTaskSpec`。
- 给 `stage_changed` 增加可选 `stageCode`，首批代码为 `preparing`、`preparing_tools`、`recovering`、`model_and_tools`、`generating_answer`、`classified_preparing`、`classified_analyzing`。保留 `stage` 文本兼容历史事件；前端优先按 code 映射文案，旧事件回退到原文本。
- Intake 继续使用确定性排除分类器，但所有约束都只解析引号外的 directive text；增加中英文否定、引用原话、显式 Apply 冲突、local-only 和高风险事实的表驱动测试，不引入 LLM 分类授权。
- 行为稳定后把 `run_engine.rs` 按职责拆为 `providers.rs`、`observer.rs`、`finalization.rs`、`recovery.rs`，`mod.rs` 保留现有入口和 re-export；拆分提交不包含功能变化。

### 阶段 3：显式、只读、确定性并行子代理

- `spawn_subagent` 兼容现有单个 `task`，并增加 `tasks` 数组；两者互斥，每批最多 3 个任务、并发上限 3，只有 Intake 已授予 `harness.child_run` 时才暴露。
- ChildRun 保持 depth-1、只读或继承的 `web_search`、禁止写锁、禁止确认、禁止再次委派。
- 在模型一次返回多个 ChildRun 时，先按请求顺序持久化全部 ToolStarted，再并发执行，最后按请求顺序持久化 ToolCompleted 和报告，避免完成时序影响事件回放。
- 返回 `SubagentBatchReport`，每项包含 `summary`、`findings`、`evidenceIds`、`confidence`、`openQuestions`、`errors` 和预算使用；自由文本只进入 `summary`，其余字段由 Harness 校验生成。
- 单个子任务失败只产生结构化错误并允许父 Run 继续；全部失败时顶层工具结果 `success=false`，仍由父模型决定收敛或给出受限回答。
- 修正当前工具说明，不再在串行实现下宣称“并行”；资源锁规划在只读阶段仅用于拒绝 write 请求，不宣称已支持写入协调。

### 阶段 4：白名单、逐 Run 授权的通用 MCP 只读工具

新增 migration `059_agent_mcp_capability_bindings.sql` 及 down 脚本。

- 新增 `mcp_capability_bindings`：保存 provider、稳定暴露名、真实 MCP tool 名、输入 Schema、参数映射、输出策略、`external.read` capability、只读风险和配置 hash。
- 新增 `agent_run_mcp_tool_snapshots`：在 Accept 事务中冻结本 Run 的 binding、provider config hash、Schema、映射和输出策略；运行中禁止重新 discovery 或接受配置漂移。
- 公共请求增加：
  - `ExternalToolGrantRef { bindingId, bindingConfigHash }`
  - `AssistantRunStartRequest.externalToolGrants?: ExternalToolGrantRef[]`
- 授权必须同时满足：provider 已启用、binding 为只读、配置 hash 匹配、用户在 Composer 中逐 Run 显式选择、normal domain。分类域、隐式关键词和 Skills 均不能增加该能力。
- `ToolRegistry` 由“内置工具 + 冻结 MCP snapshot”组成；动态名称使用创建 binding 时生成的稳定安全名，不注入 MCP 服务端原始描述。
- MCP 输出只接受受限文本/JSON，模型可见结果上限 8,000 字符；证据账本登记 provider、获取时间、内容 hash 和最多 2,000 字符摘录，事件与工具审计只保存安全摘要。
- 第一版明确拒绝 write、send、delete、calendar mutation、process 和 secret 类 binding；未来写能力必须另走确认计划和外部副作用幂等设计。
- 管理中心负责 discovery、只读 mapping 与诊断；Composer 负责逐 Run 授权，两者不得合并为“启用后自动给所有 Run”。

### 阶段 5：可信且可解释的 Skills 激活

新增 migration `060_skill_activation_embeddings.sql` 及 down 脚本，为激活索引增加 `embedding_source_hash`、`embedding_model` 和 `embedding_dimensions`。

- 将全量 DELETE/INSERT 改为增量 upsert：未变化条目保留向量，变更条目清空旧向量，已删除条目定向删除。
- Vault 激活先立即完成词法索引和缓存替换，再后台生成缺失向量；Run 本身不访问文件系统、不等待 embedding。
- 显式技能名、trigger hint 和关键词匹配始终优先；向量只对已有词法候选重排，不能让零关联技能凭相似度直接激活。
- 继续最多激活 1 个 primary + 1 个 auxiliary；向量失败、模型变化或维度不匹配时确定性回退词法排序。
- 建立中文短查询、混合语言、显式提及、同义表达和误激活评测集；只有相对词法基线提高召回且不增加高风险误激活时才默认启用向量重排。
- 不采纳“Skills 强制 tool_choice”或技能链自动增权；工具调用仍受 Envelope 和授权快照控制。

## 测试与验收

- 所有功能严格测试先行，每个阶段先证明测试失败，再实现，再独立提交。
- Durable 恢复覆盖：写前崩溃、写后终态前崩溃、确认拒绝后崩溃、内容被第三方修改、重复 Resume、过期但已消费确认、普通 ToolLoop 中断。
- 预算覆盖：标准 8/24 不变、ChildRun 2/6、输入输出 token 上限、配置变化不改变已接受 Run。
- 并行覆盖：3 个真实并发子任务、稳定报告顺序、部分/全部失败、无递归、无写入、Web 不越权、事件序列连续。
- MCP 覆盖：未授权不暴露、配置漂移失败、Schema 拒绝、只读门禁、输出超限、证据归属、日志/事件/checkpoint 零敏感内容。
- Skills 覆盖：增量更新、向量生成失败回退、显式匹配优先、最多两个、不能增加工具 capability。
- 前端覆盖历史 `stage` 与新 `stageCode`、恢复操作、外部工具授权 chip 和断流回放。
- 每阶段通过：
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
  - `npm run lint`
  - `npm run format:check`
  - `npm run typecheck`
  - `npm run test`
  - `npm run audit:rust`
  - `npm audit`
- 全部阶段完成后运行 `npm run test:e2e` 和 Agent capacity eval；安全轨任何越权、伪引用、重复写入或敏感数据泄漏均为零容忍失败。

## 默认假设

- 采用分阶段完整演进，而非一次性能力扩张。
- 子代理只在用户明确要求委派、并行或交叉验证时启用。
- 通用 MCP 采用 capability mapping 白名单，不直接暴露 discovery 得到的任意工具。
- 不新增运行时依赖；并发使用现有 Tokio/futures 能力。
- 不改变技术栈、许可证、安全红线和 Markdown 权威来源原则。
- 本方案不承诺具体发布版本；每阶段只有在自动化门禁和对应文档同步完成后，才能写入 ROADMAP 的已交付事实。

## Task 1：阶段 0 契约校准与回归基线

落实阶段 0 的契约测试、架构和路由文档、六阶段验收矩阵。测试先行，且不能改变授权面、Web 授权源、涉密持久化策略或 Markdown 写入确认流程。

## Task 2：阶段 1 Durable 恢复闭环

落实阶段 1 的 `expectedPostContentHashes`、生产 checkpoint、恢复判定、Resume 乐观并发与前后端恢复类型。严格以阶段 1 的全部条目和测试矩阵为验收要求。

## Task 3：阶段 2 预算、事件代码与 Intake 治理

落实阶段 2 的冻结预算策略、Gateway token 限制、`stageCode` 兼容事件、引号外 deterministic Intake 测试，以及行为不变的 `run_engine.rs` 职责拆分。严格以阶段 2 的全部条目和测试矩阵为验收要求。

## Task 4：阶段 3 显式、只读、确定性并行子代理

落实阶段 3 的批量子代理契约、授权与只读限制、确定性事件序列、并发执行、结构化报告和失败语义。严格以阶段 3 的全部条目和测试矩阵为验收要求。

## Task 5：阶段 4 白名单、逐 Run 授权的通用 MCP 只读工具

落实阶段 4 的 059 迁移、绑定与快照、公共 IPC 请求、normal-domain 授权、ToolRegistry、安全输出与证据审计，以及管理中心与 Composer 的边界。严格以阶段 4 的全部条目和测试矩阵为验收要求。

## Task 6：阶段 5 可信且可解释的 Skills 激活

落实阶段 5 的 060 迁移、增量索引更新、后台向量生成、词法优先的受限重排、确定性回退和激活评测。严格以阶段 5 的全部条目和测试矩阵为验收要求。
