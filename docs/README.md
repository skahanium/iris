# Iris 文档索引

阅读顺序建议：**路线图（排期）→ 设计系统（界面）→ 架构（实现）→ 贡献/Agent（施工）**。

---

## 文档层级

| 层级         | 文档                                   | 职责                                          |
| ------------ | -------------------------------------- | --------------------------------------------- |
| 门面         | [README.md](../README.md)              | 项目介绍、快速开始、设计哲学                  |
| 排期（唯一） | [ROADMAP.md](../ROADMAP.md)            | 版本里程碑                                    |
| 界面         | [design-system.md](./design-system.md) | Iris Rail、Notion N token、组件与 Chrome 规则 |
| 实现         | [ARCHITECTURE.md](../ARCHITECTURE.md)  | 分层、数据流、IPC、安全                       |
| 变更         | [CHANGELOG.md](../CHANGELOG.md)        | 版本变更记录                                  |
| 施工         | [AGENTS.md](../AGENTS.md)              | AI/人协作硬约束、命令速查                     |
| 协作         | [CONTRIBUTING.md](../CONTRIBUTING.md)  | 环境、PR、测试、Commit                        |
| 安全         | [SECURITY.md](../SECURITY.md)          | 漏洞报告                                      |

**原则**：不在多份文档里重复维护版本排期；细节以 ROADMAP 为准，其他文档只引用并补充本域内容。

---

## 当前版本

**v1.2.4**。旧 v0.x / v1.1.x 规划与施工资料已完成归并，不再作为当前事实来源；必要时通过 git 历史查阅。

---

## 专题

| 主题                      | 文档                                                                                                                                                           |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 使用指南（用户向）        | Notion 官方文档站（无限期延后，URL 待发布）                                                                                                                    |
| IPC API 参考              | [ipc-api-reference.md](./ipc-api-reference.md)                                                                                                                 |
| 设计系统 / Iris Rail      | [design-system.md](./design-system.md)                                                                                                                         |
| 品牌图标                  | [design-system/brand.md](./design-system/brand.md)                                                                                                             |
| 编辑器 Markdown 导出      | [markdown-export.md](./markdown-export.md)                                                                                                                     |
| LLM 路由与连通性          | [llm-routing.md](./llm-routing.md)                                                                                                                             |
| AI Agent 当前状态         | [audits/2026-06-20-ai-agent-current-readiness.md](./audits/2026-06-20-ai-agent-current-readiness.md)                                                           |
| AI Agent 问题矩阵         | [audits/2026-06-20-ai-agent-issue-matrix.md](./audits/2026-06-20-ai-agent-issue-matrix.md)                                                                     |
| AI Harness TaskPlan 蓝图  | [spec](./superpowers/specs/2026-06-21-agent-harness-taskplan-blueprint-design.md) · [plan](./superpowers/plans/2026-06-21-agent-harness-taskplan-blueprint.md) |
| AI Harness 架构收口       | [spec](./superpowers/specs/2026-07-01-iris-ai-harness-architecture-design.md) · [plan](./superpowers/plans/2026-07-01-iris-ai-harness-architecture.md)         |
| Agent 搜索并行与证据链    | [spec](./superpowers/specs/2026-07-03-agent-search-concurrency-design.md) · [plan](./superpowers/plans/2026-07-03-agent-search-concurrency.md)                 |
| RAG 系统深度优化 (v1.2.6) | [specs/v1.2.6-rag-deep-optimization.md](./specs/v1.2.6-rag-deep-optimization.md)                                                                               |
| 语义搜索 Recall@5         | [eval/semantic-search.md](./eval/semantic-search.md)                                                                                                           |
| 语义评测 fixture vault    | [eval/fixtures/semantic-vault/](./eval/fixtures/semantic-vault/)                                                                                               |
| 关闭回归手工验收          | [testing/app-close-manual-checklist.md](./testing/app-close-manual-checklist.md)                                                                               |
| Iris Rail 手工验收        | [testing/iris-rail-refresh-manual-checklist.md](./testing/iris-rail-refresh-manual-checklist.md)                                                               |
| 性能排查                  | [ops/performance-guide.md](./ops/performance-guide.md)                                                                                                         |
| 品牌素材                  | [../scripts/assets/README.md](../scripts/assets/README.md)                                                                                                     |

---

## 维护约定

1. **新增版本里程碑**：只改 `ROADMAP.md`，并在本表「当前版本」更新状态。
2. **架构 / IPC 变更**：同步 `ARCHITECTURE.md`、`src/types/ipc.ts`、`src/lib/ipc.ts` 与 `AGENTS.md` §4.2。
3. **发布**：同步 `CHANGELOG.md`、ROADMAP 版本状态与 git tag。
4. **设计系统变更**：先改 `design-system.md` 与 ROADMAP 对应节，再改 `src/styles/globals.css` 与组件。
5. **历史施工资料**：6 月 15 日以前的规划、设计、施工文档不再留在工作树；需要追溯时查 git 历史。
