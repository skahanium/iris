# Iris Agent Harness 建设文档

本目录是 Iris Agent Harness 的唯一现行建设入口，统一描述当前状态、长期不变量、目标架构、研究与工具合同、实施顺序和验收门槛。它不建立第二份产品路线图，也不把目标设计冒充为已部署事实。

## 文档权威边界

- 版本、里程碑和产品范围以 [`ROADMAP.md`](../ROADMAP.md) 为唯一来源。
- 已部署模块、数据流和兼容边界以 [`ARCHITECTURE.md`](../ARCHITECTURE.md) 与代码为准。
- 本目录负责 Harness 的差距、目标合同、技术债处置、实施顺序和验收证据，不独立承诺版本或日期。
- 历史方案已归档；除本页提供的历史入口外，归档内容不构成现行规范。

## 统一方向

1. 以模型驱动、`EvidenceGap` 驱动的多轮 Web 研究作为通用时效事实主路径。
2. 继续只暴露一个模型可见网络工具 `web_search`，通过现有 `query`、`gap`、`urls` 合同支持搜索与当前 Run 内的深入抓取。
3. 结构化领域能力保留为已配置时的精确事实快路径；11 个 operation 不再是近期完成门槛。
4. 路由按任务形态区分精确槽位事实和研究型问题，不再按领域统一阻断无 binding 请求。
5. Host 始终控制权限、URL 来源、预算、证据所有权、最终化与失败语义；模型只能在这些边界内调整研究重点。
6. 通过 Quick、Standard、Deep 三档预算、单轮最多 3 个并发抓取、提前停止和回归门禁控制性能。
7. 新抽象必须在同阶段替换旧分支；不得新增第二研究引擎、第二网络工具、第二 provider registry 或第二证据真相源。

## 当前基线摘要

审计基线为 `f19364d6` 及其后的本次工作树变更，日期为 2026-08-22。

- Run 幂等、durable finalization、工具表面冻结、Run-local evidence、隐私门禁和 `system_time_now` 已有实现与回归测试。
- `FreshResearchPlan`、`ResearchBudget`、`EvidenceGap` 和多次搜索骨架已经存在，但抓取预算、模型后续深抓取与按任务形态路由仍为部分实现。
- 五类领域工具表面与 11 个 operation 的 DTO、mapping、validator、Host renderer、production fixtures 和 migration 072 已存在；这不代表真实 Provider 已配置。
- 2026-08-19 的开发实例历史审计为 0/11 configured。该数字只作为带日期快照保留，实施真实 Provider 前必须重新只读审计。
- 2026-08-22 已修复 Windows 评测路径权限语义与 PowerShell MCP fixture 漂移，并重新通过 `npm run agent:eval:smoke` 的 24/24 deterministic matrix。

完整状态和证据见 [`02-current-state-and-debt.md`](02-current-state-and-debt.md) 与 [`appendices/A-status-and-test-traceability.md`](appendices/A-status-and-test-traceability.md)。

## 阅读顺序

1. [`01-authority-and-invariants.md`](01-authority-and-invariants.md)：先确认不可违反的边界。
2. [`02-current-state-and-debt.md`](02-current-state-and-debt.md)：理解代码已经做到什么、还欠什么。
3. [`03-target-architecture.md`](03-target-architecture.md)：理解统一后的 Harness 形态。
4. [`04-research-and-tool-contracts.md`](04-research-and-tool-contracts.md)：实现网络研究、结构化快路径或路由时使用。
5. [`05-implementation-roadmap.md`](05-implementation-roadmap.md)：选择下一阶段及其同步删除项。
6. [`06-evaluation-performance-and-acceptance.md`](06-evaluation-performance-and-acceptance.md)：在声称完成前核对质量、性能和安全门禁。

附录分别提供[状态与测试追踪](appendices/A-status-and-test-traceability.md)、[当前事实合同矩阵](appendices/B-current-fact-contract-matrix.md)和[决策与延期项](appendices/C-decisions-and-deferred.md)。

## 状态语言

每项工作必须同时记录实现状态和处置方式，禁止把两个维度混在一个“完成”词中。

| 维度     | 允许值       | 含义                                       |
| -------- | ------------ | ------------------------------------------ |
| 实现状态 | 已验证       | 当前工作树上的命名测试已经通过             |
| 实现状态 | 已实现待复验 | 代码存在，但本次尚未取得足够的当前测试证据 |
| 实现状态 | 部分实现     | 只覆盖部分路径、预算或生产数据流           |
| 实现状态 | 计划中       | 已有决策完整的施工项，尚未实现             |
| 实现状态 | 延期         | 明确不进入当前主干                         |
| 处置方式 | 保留         | 继续作为单一事实或稳定合同                 |
| 处置方式 | 重构         | 用新合同替换并在同阶段删除旧分支           |
| 处置方式 | 移除         | 无替代价值或会制造冲突                     |
| 处置方式 | 归档         | 仅供历史追溯，不再指导施工                 |

宽泛全量测试不能替代能力对应的命名测试；历史文档中的通过数量不得直接迁移到本目录。

## 归档

统一前的两套文档和根入口保存在[归档清单](archive/2026-08-pre-unification/MANIFEST.md)中。归档文件保持历史原貌，其中的相对链接、状态和版本判断可能已经失效。
