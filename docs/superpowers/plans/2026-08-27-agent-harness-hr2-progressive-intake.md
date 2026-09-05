# Agent Harness HR-2：渐进联网与去领域化 Intake 实施计划

> 状态：已完成，待提交
> 基线日期：2026-08-27
> 依据：`agent-harness/03-target-architecture.md`、`agent-harness/05-implementation-roadmap.md`、`agent-harness/appendices/B-task-capability-and-risk-matrix.md`

## 范围与边界

本阶段只收敛新 Normal Run 的入口合同与由其直接驱动的工具面。它不实现 HR-3 的自适应循环和分类预算、不实现 HR-4 的自然澄清或普通回答终局、不实现 HR-5 的冻结变更集，也不删除旧版领域持久化/执行代码（该清理由 HR-6 完成）。不新增数据库表、迁移、IPC、Provider 或依赖。

现存 `FreshFactPolicy`、`FreshFactDomain`、`DomainOperation` 和 `CurrentRunDomain` 必须继续可反序列化并支持既有 Run 恢复；但新的 `ExecutionEnvelope` 不得再写入领域、操作、时间窗口或地点要求，也不得冻结 `web.domain.read` 授权。

## 先行验证（Red）

在改实现前，于 `src-tauri/src/ai_runtime/run_intake_tests.rs` 建立以下失败回归：

1. 表驱动覆盖 Chat、AskNotes、Research、CitationCheck、Draft、Apply：分别断言 `Offline`、`WebPreferred` 或 `WebRequired`，并断言 Effort、验证义务与 Web 能力符合任务矩阵。
2. 普通推荐、比较和公开资料梳理在 Web 开启时为 `WebPreferred + None`，关闭时降为 `Offline + None`；不应以 `StrictExternalFact` 终局拒绝。
3. 显式 URL/联网核实、强时效事实、高风险当前建议在开启时为 `WebRequired + CurrentRunWeb`，关闭时仅保留证据义务而绝不获得 Web 能力。
4. 新 Run 对天气、新闻、金融、娱乐、体育、泛当前事实和运行时问题都持久化空 `FreshFactPolicy`，且不生成 `web.domain.read` 或领域冻结操作；测试以 `RunIntake::start` 后的持久化 envelope 为准。
5. Web 开关是唯一的 Web 授权来源：相同任务切换开关只能改变能力集合，不得由分类、模型文本、历史会话或外部授予隐式加权。
6. 旧 envelope 与旧领域策略仍能反序列化；该兼容测试保留在 `run_contract_tests.rs`，不将旧值当作新 Run 行为。

先运行精确的 `run_intake_tests`，确认上述新增断言在旧实现中失败，再开始最小实现。

## 实施顺序

1. `run_intake.rs`
   - 移除 `classify_fresh_fact`、时间敏感领域投影和领域操作对新 Run 的输入依赖。
   - 保留硬/软离线排除；运行时问题改由窄文本识别，不依赖领域分类。
   - 将 Web 决策改为：显式外部只读授予为 `Offline + CurrentRunExternal`；URL、明确联网/核实、强时效和高风险当前事实为 `WebRequired + CurrentRunWeb`；其余非排除 Normal 请求在 Web 开启时为 `WebPreferred + None`，关闭时为 `Offline + None`。
   - `WebRequired` 和 `WebPreferred` 均进入既有 `ToolLoop`；Web 能力只由开关、Normal 域和非 local-only 决定。新的 envelope 使用 `FreshFactPolicy::default()`，并向仓库传入空领域操作集。
2. `tool_surface.rs` 与 `run_context.rs`
   - 用 `VerificationRequirement`（而非 `FreshFactDomain`）表示“当前回答必须获取 Web 证据”的工具提示和不可编造约束。
   - 保持已有工具面和持久化协议，不创建第二套分类器。
3. `normal_run_service.rs`
   - 新 Run 由空领域策略自然走通用 `web_search`/工具循环；不为它暴露结构化领域工具、地点输入或基于领域的研究预算/结构化最终化。
   - 只在读取到旧 envelope 的非空领域策略时保留原有兼容分支。HR-6 再删除旧实现。
4. 更新仅受事实影响的 Harness 追踪与路线文档，准确记载“新入口已去领域化、旧领域读取兼容仍存在”，不把 HR-3/HR-4 写成已完成。

## 退出与验收

- 表驱动任务矩阵、普通渐进联网、严格边界、开关唯一授权、新 Run 空领域持久化和旧值反序列化测试全部通过。
- 新 Run 不含 `web.domain.read`，不会由 `FreshFactDomain` 触发输入、工具面、研究预算或终局策略。
- 运行针对性 Rust 测试、`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`npm run docs:check`、`npm run format:check`、`git diff --check`；不连接真实 Provider，也不运行无关全量测试。
- 复读完整 diff，检查无领域关键词特判、无新持久化格式/实体、文档未将后续 HR 阶段冒充为已实现；随后中文 Conventional Commit 并推送 `branch-v1.3.0`。

## 实施证据

- 先行回归曾在旧实现下失败：普通问题仍为 `WebRequired`，新 Run 冻结 `FreshFactPolicy` 和 `web.domain.read`，任务矩阵的 Chat 也被强制为严格联网。
- 实现后，`run_intake_tests` 80/80、`normal_run_service_tests` 31/31、`tool_surface` 相关测试 8/8 和 `run_context::timeliness_tests` 1/1 通过。
- 严格 Web 路径保留既有结构化最终化：无工具模型明确终态为 `NoCapableModel`；具备工具模型的 mock 路径继续验证当前 Run 证据和最终来源绑定。普通 `WebPreferred` 的自然正文最终化仍是 HR-4，不在本阶段伪造完成。
