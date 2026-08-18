# Iris Agent Harness 可靠性重构

本目录定义 Iris 现有 Agent Harness 的可靠性重构。它不是第二份产品路线图，也不把 Iris 扩张成通用 Agent 平台。

- 版本排期与产品边界以 [`ROADMAP.md`](../ROADMAP.md) 为唯一来源。
- 已实现架构事实以 [`ARCHITECTURE.md`](../ARCHITECTURE.md) 和代码为准。
- 本目录只记录尚未交付的约束、差距、实施顺序和验收方法。

## 重构目标

1. 让同一用户请求只产生一次有效执行，并且终态、持久化与前端展示一致。
2. 让模型看到的工具、权限与运行时真实可执行能力一致。
3. 让 Web 来源展示如实区分“精确证据绑定”和“本次检索来源组”。
4. 将授权、安全、证据和上下文处理收敛到少量可测试契约，而不是新增并行状态机。
5. 用问题 ID 到测试用例的追踪关系约束实施和验收。

## 文档结构

### 核心文档

1. [`01-invariants-and-non-goals.md`](01-invariants-and-non-goals.md)：不可破坏的系统约束与明确非目标。
2. [`02-current-to-target-delta.md`](02-current-to-target-delta.md)：基于当前代码的最小目标形态。
3. [`03-reliability-contracts.md`](03-reliability-contracts.md)：Run、工具、授权、证据、上下文和 UI 契约。
4. [`04-implementation-roadmap.md`](04-implementation-roadmap.md)：按风险排序的实施阶段与停线条件。
5. [`05-evaluation-and-acceptance.md`](05-evaluation-and-acceptance.md)：测试分层、场景矩阵和完成标准。

### 附录

- [`appendices/A-current-state-audit.md`](appendices/A-current-state-audit.md)：现状核对清单，只记录可由代码复核的事实。
- [`appendices/B-issue-test-traceability.md`](appendices/B-issue-test-traceability.md)：问题 ID 到测试用例和验收证据的追踪表。
- [`appendices/C-deferred-capabilities.md`](appendices/C-deferred-capabilities.md)：不进入本次主干的未来能力提案。

## 使用方式

实施前先在现状核对清单中确认问题仍存在，再按实施路线图选择一个阶段；实现时遵守可靠性契约；合并前用追踪表确认每个已处理问题都有相应测试。代码事实变化时，优先更新附录 A 和 B，不在多份叙述文档中重复维护同一事实。

## 完成定义

本次重构只有在以下条件同时成立时才算完成：

- 已确认的 P0/P1 问题均已修复，或被明确移入 deferred 并说明理由；
- Run、工具授权、证据绑定、上下文压缩的核心契约均有自动化测试；
- 没有引入第二套 Run 状态机、工具目录、证据存储或会话真相源；
- UI 不会把来源组暗示为逐段精确引用；
- 自由文本语义判断、跨会话语义记忆等实验能力未被当作默认完成条件。
