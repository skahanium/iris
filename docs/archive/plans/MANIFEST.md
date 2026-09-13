# 施工计划归档清单

本目录保存**已落地或已撤回**的施工计划，共 14 份。归档文件不再维护：其中的任务勾选、
测试名与文件路径都是开工当时的快照，**不得作为当前能力、阶段状态或文件布局的依据**。

当前事实的唯一来源分别是：[ROADMAP.md](../../../ROADMAP.md)（版本排期）、
[ARCHITECTURE.md](../../../ARCHITECTURE.md)（模块与数据流）、
[Agent Harness 建设文档](../../../agent-harness/README.md)（Harness 事实、目标合同与 HR 阶段状态）。
仍在施工的计划保留在 [`docs/superpowers/plans/`](../../superpowers/plans/)。

本目录的失效链接由 `scripts/docs-facts-check.mjs` 白名单豁免；`docs/README.md` 是本目录的唯一入口，
其余现行文档不得把归档材料当作依据引用。

| 归档文件                                                                                                                        | 归档原因                                                                                                      | 当前状态依据                                                                                             |
| ------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| [2026-07-29 Iris Agent Run 可靠性与受控能力演进方案](./2026-07-29-iris-agent-run-runtime-optimization.md)                       | 阶段 0 契约基线已落地；阶段 1–5 的后续方向已由 Harness HR 路线接管，六阶段冻结门禁保留在 ROADMAP 的验收矩阵中 | `tests/agent-run-phase0-contract.test.ts`；ROADMAP 六阶段受控演进验收矩阵                                |
| [2026-07-30 Agent 面板启动性能计划](./2026-07-30-agent-panel-startup-performance.md)                                            | 面板预热与 MCP 发现延后已落地；Task 3 的共享 Markdown Worker 后被反转，历史 Markdown 改为同步渲染 + 窗口分块  | ROADMAP 已交付节；`tests/assistant-stream-rendering-performance-contract.test.ts`                        |
| [2026-07-31 v1.2.19 自适应工作区计划](./2026-07-31-v1.2.19-adaptive-workspace.md)                                               | v1.2.19 已交付；人工矩阵并入通用 UI 清单                                                                      | ROADMAP 已交付节；[自适应工作区规范](../../adaptive-workspace.md)                                        |
| [2026-08-07 Task 2 Agent sqlite-vec 复查修复](./2026-08-07-task-2-agent-sqlite-vec-review-fixes.md)                             | 已落地：sqlite-vec v3 KNN、硬 scope 前置与发布烟测都在 CI 中持续运行                                          | `.github/workflows/ci.yml` 的 sqlite-vec 迁移/KNN 与 50k 规模门                                          |
| [2026-08-11 MiniMax 协议与发布门禁加固](./2026-08-11-minimax-protocol-release-hardening.md)                                     | 已落地：流式推理续轮、M2/M3 控制与自定义端点能力边界修正                                                      | CHANGELOG 1.3.0 的 Fixed 条目                                                                            |
| [2026-08-16 AI 对话渲染稳定性计划](./2026-08-16-ai-conversation-rendering-stability.md)                                         | 已落地：单一投影 Hook、阅读锚点与窗口分块；定稿 Markdown 走同步渲染，反转了 07-30 计划的 Worker 方向          | `src/components/ai/hooks/useConversationReadingAnchor.ts`；`src/components/ai/WindowedMarkdownBlock.tsx` |
| [2026-08-23 AH-2 自适应研究循环计划](./2026-08-23-agent-harness-ah2-adaptive-research-loop.md)                                  | **已撤回**：AH-2 方向作废，目标模块 `fresh_research_plan` 已删除                                              | `agent-harness/05` 开篇的撤回声明与 `agent-harness/README.md`                                            |
| [2026-08-27 Harness HR-1 回归基线计划](./2026-08-27-agent-harness-hr1-regression-baseline.md)                                   | 计划施工已落地；阶段状态为「已完成」                                                                          | `agent-harness/05` 阶段表                                                                                |
| [2026-08-27 Harness HR-2 渐进 Intake 计划](./2026-08-27-agent-harness-hr2-progressive-intake.md)                                | 计划施工已落地；阶段状态为「已完成，持续回归」                                                                | `agent-harness/05` 阶段表                                                                                |
| [2026-08-28 Harness HR-3 统一工具循环计划](./2026-08-28-agent-harness-hr3-unified-adaptive-tool-loop.md)                        | 计划施工已落地；阶段状态为「重新打开，施工中」，后续施工不由本文件跟踪                                        | `agent-harness/05` 阶段表                                                                                |
| [2026-08-30 Harness HR-4 回答与投影计划](./2026-08-30-agent-harness-hr4-answer-clarification-projection.md)                     | 计划施工已落地；阶段状态为「重新打开，施工中」，后续施工不由本文件跟踪                                        | `agent-harness/05` 阶段表                                                                                |
| [2026-08-30 Harness HR-5 冻结变更集计划](./2026-08-30-agent-harness-hr5-frozen-change-set-and-verification.md)                  | 计划施工已落地；阶段状态为「集成验收重新打开」                                                                | `agent-harness/05` 阶段表                                                                                |
| [2026-08-31 Harness HR-6 领域核心退役计划](./2026-08-31-agent-harness-hr6-domain-core-retirement.md)                            | 计划施工已落地；阶段状态为「已完成，持续回归」                                                                | `agent-harness/05` 阶段表                                                                                |
| [2026-08-31 Harness HR-7 质量与 Provider 矩阵计划](./2026-08-31-agent-harness-hr7-deterministic-quality-and-provider-matrix.md) | 计划施工的部分已落地；阶段状态为「已实现，实测未通过」                                                        | `agent-harness/05` 阶段表                                                                                |
