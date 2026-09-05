# HR-7：通用质量评测与 Provider 能力矩阵实施计划

> **状态：确定性部分已完成；真实 Provider 试点待单独授权**
> **基线：2026-08-31，提交 `c414894b`**
> **范围：`agent-harness/05-implementation-roadmap.md` 的 HR-7；不发送真实模型请求。**

## 目标与边界

HR-7 的交付是可重复的通用质量与能力基线，而不是把 fixture 成功冒充真实模型质量。评测必须覆盖普通对话、差检索结果后的调整、本地多跳、混合材料、严格当前事实、无工具模型和受确认文档修改；所有案例走生产 Run/ToolLoop/终态路径或可证明等价的 Gateway 归一化路径。

真实 Provider 的回答质量、P50/P95、成本和盲评仍是外部副作用：只有用户另行确认模型、最大成本、场景与数据边界后才可运行。没有这项授权时，本阶段只可声明“确定性合同与 mock 协议基线通过”，不可声明生产质量已经验证。

## 完成记录（2026-08-31）

- 24 个通用 deterministic 场景已 24/24 完成、24/24 通过；WebRequired fixture 已改为真实路径的“模型工具调用 → 当前 Run 结果 → 综合”。
- 八项硬边界各重复五次；总工具上限使用真实 catalog 的 12 local + 6 network + 6 external-read 组合验证，第 25 次不执行、最终综合仍可完成。
- OpenAI-compatible 与 Anthropic Messages 的本地 mock 各自验证工具调用和结果续接；chat-only 保持能力降级。`npm run agent:eval:smoke` 同时验证本地加密凭据隔离与禁止继承云密钥。
- 未运行 `agent:eval:live`，未连接真实 Provider；因此计划中涉及真实质量、P50/P95、成本与盲评的项目仍是明确授权边界，不是已完成声明。

## 实施步骤

### 1. 先让质量门禁能失败

**文件：** `agent_capacity_eval.rs`、`agent_capacity_eval_tests.rs`、`normal_run_service_tests.rs`、`agent_tool_loop_tests.rs`。

- [x] 已将现有 capacity fixture 收敛为表驱动的 24 案 `CoreScenario` 矩阵；每案声明任务类、授权能力、预算、脚本化 Provider 行为、期望工具轨迹、来源/写入/终态断言和质量原子。
- [x] 已覆盖差的首轮 Web 结果后的改写、重复成功调用、连续两轮无新资源、无工具 Provider、严格事实缺少当轮证据和写入越过确认等负向路径。
- [x] 每个案例都有会在放松对应 Host 约束时失败的断言；不依赖数据库 ID、领域词或回答字符串偶合。

### 2. 建立通用任务矩阵与评测产物

**文件：** `agent_capacity_eval.rs`、`agent_capacity_eval_tests.rs`、`scripts/agent-eval.mjs`、`scripts/agent-eval.test.mjs`。

- [x] 复用 `EvaluationTelemetryTap` 和既有 JSON schema，输出匿名化聚合：case id、通过/失败、工具/模型轮次、预算结果、来源覆盖、终态、安全代码、token/延迟统计；不输出正文、查询词、URL、笔记或密钥。
- [x] 评分拆为管线正确性、回答质量原子、来源归属、安全/副作用与性能，安全失败为硬失败。
- [x] 每个任务类已有成功与失败脚本，覆盖 Direct、Standard、DurableApply 及 chat-only 降级。
- [x] `agent:eval:smoke` 运行快速 deterministic 子集；`agent:eval` 运行完整 deterministic 子集；`agent:eval:live` 保持严格授权前置且不得被其他脚本隐式调用。

### 3. 验证 Provider 中立性

**文件：** `model_gateway/body.rs`、`model_gateway/streaming.rs`、`llm/provider_contract.rs`、Gateway 测试与能力评测测试。

- [x] OpenAI-compatible chat-completions、Anthropic messages 经各自请求/流式工具调用归一化后，产生同一 ToolLoop 语义与评测结论。
- [x] chat-only 或未探测自定义端点只能安全降级为无工具 Direct；评测明确记录受限能力，不把工具合同假设为可用。
- [x] 核心不按供应商、模型显示名或 MiniMax 名称改变策略；差异只位于 Gateway/provider contract 适配层。

### 4. 真实试点门与文档

- [x] 已保留并验证 live-pilot command 的严格前置：会话授权、配置 hash、场景、最大成本、过期时间、结果输出目录和匿名盲评缺一不可；拒绝缺项早于凭据/Provider 访问。
- [x] 文档已区分确定性通过、mock 协议通过、真实试点执行和生产质量结论；无授权时不写“已验证”。
- [x] 已更新 `agent-harness/06-evaluation-performance-and-acceptance.md`、附录 A/C、`ROADMAP.md` 与 `docs/README.md` 的事实状态。

## 验收证据

1. 七类任务各至少一个正向与一个反向确定性案例，均使用通用能力/风险字段而非领域 operation。
2. OpenAI-compatible 与 Anthropic mock 都通过同一 ToolLoop 语义断言；chat-only 表现为受控降级。
3. 评测产物拒绝泄漏正文、查询、URL 与密钥，并可通过自身 schema 校验。
4. P50/P95/token 仅来自 mock 运行时 telemetry；文档明确它们不是生产性能数据。
5. `agent:eval:live` 没有显式授权即拒绝，且本轮不执行它。
