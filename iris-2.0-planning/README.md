# Iris 2.0 规划文档体系

本目录记录 Iris 2.0 的产品与架构设想，用作未来正式 ROADMAP / ARCHITECTURE / design-system 改版前的依据。

这些文档不是当前版本承诺，不替代根目录 `ROADMAP.md`。当前实现事实仍以 `ROADMAP.md`、`ARCHITECTURE.md`、`docs/design-system.md` 和代码为准。

## 核心方向

Iris 2.0 的候选北极星：

> 在 Markdown 主权不变的前提下，增加本地优先的知识对象层，让笔记、任务、项目、人物、资料、网页与附件能够被结构化组织、快速计算，并由 Agent 以 proposal + confirmation 的方式安全操作。

## 文档阅读顺序

1. [00-north-star.md](./00-north-star.md) - 产品北极星与非目标
2. [01-concepts.md](./01-concepts.md) - Object / Collection / View 概念
3. [02-architecture.md](./02-architecture.md) - 总体架构分层
4. [03-data-model.md](./03-data-model.md) - 数据模型与 SQLite 边界
5. [04-agent-protocol-permissions.md](./04-agent-protocol-permissions.md) - Agent 对接协议与权限矩阵
6. [05-formula-compute.md](./05-formula-compute.md) - Iris DSL、统计、公式与计算引擎
7. [06-markdown-integration.md](./06-markdown-integration.md) - Markdown / 文档体系融合
8. [07-ui-views.md](./07-ui-views.md) - Grid / Board / List 视图体系
9. [08-performance-security.md](./08-performance-security.md) - 性能、安全、审计与回滚
10. [09-roadmap-open-questions.md](./09-roadmap-open-questions.md) - 分阶段路线图草案与待讨论问题

## 已锁定的设计判断

- 不做 WPS、Notion、Anytype 或 AFFiNE 克隆。
- Markdown 继续是正文权威源。
- SQLite 是 Object / Collection / View / 多维表数据权威源。
- 多维表不是平行系统，而是 Collection 的 Grid View + Property + Formula / Aggregate 能力。
- Object 层只覆盖用户可见知识对象，不覆盖 AI runtime 内部实体。
- Object 类型以内置类型为主。
- Collection 采用 Type Collection + Topic Collection 双层。
- Field / Property 采用 Core Properties + Custom Properties。
- View 收敛为 Grid / Board / List 三种核心类型。
- 公式采用 Iris DSL，Rust 执行，强类型、无副作用、可依赖分析、可增量计算。
- Agent 内部对接走 Iris 原生 Tool / Capability / Proposal 协议，不走 MCP。
- MCP 只作为外部 web.search / web.fetch 证据 provider 边界。

## 维护规则

- 未明确讨论的内容必须标注为“待讨论”。
- 不在本目录直接承诺具体版本日期。
- 若未来纳入正式计划，需要同步根目录 `ROADMAP.md`。
- 若涉及 IPC / schema / Markdown round-trip，必须同步正式架构文档和测试计划。
