# Iris 结构化工具生产化

本目录是一套独立于 `refactor/` 的专项文档体系，聚焦时间、天气、新闻、金融、娱乐和体育等当前事实能力是否真实可用。

它解决的不是“代码里有没有工具名”，而是以下问题：

- 当前实例是否配置了对应领域的真实数据服务；
- 模型看到的能力是否与 Run 实际能够执行的 operation 一致；
- Provider 是否经过审核、真实结果预览、健康检查和冻结；
- 成功结果是否经过 DTO 验证、证据登记和最终消息恢复；
- 自动化 fixture、软件生产链和用户实例配置是否分别完成验收。

版本排期仍以根目录 `ROADMAP.md` 为唯一来源；已实现架构事实仍以代码和 `ARCHITECTURE.md` 为准。本目录不得用计划语气覆盖上述事实源。

## 当前结论

截至 2026-08-19，`branch-v1.3.0` 的真实状态是：

- `system_time_now` 是可用的本机确定性工具。
- AnySearch、Tavily 已配置为普通 `web_search/web_fetch` Provider。
- 五个外部领域工具及 11 个 operation 已进入工具目录，并具备 DTO、mapping 和验证框架。
- 开发实例的 11 个 `web.domain.read` operation binding 全部为 0。
- `news.search` 可以使用普通 Web evidence；天气、金融、娱乐和体育在没有结构化 Provider 时失败关闭。
- 当前只能宣称 **Framework Ready / Instance Unconfigured**，不能宣称结构化垂直工具已经可用。

## 文档结构

1. [`01-current-state-and-evidence.md`](01-current-state-and-evidence.md)：代码、数据库和运行行为的实况证据。
2. [`02-gap-register.md`](02-gap-register.md)：问题 ID、优先级、影响和最小弥补边界。
3. [`03-target-architecture.md`](03-target-architecture.md)：operation-specific readiness、Provider 准入、冻结与降级架构。
4. [`04-implementation-roadmap.md`](04-implementation-roadmap.md)：按依赖和风险排序的施工阶段。
5. [`05-evaluation-and-acceptance.md`](05-evaluation-and-acceptance.md)：软件门禁、实例门禁和问题—测试追踪。
6. [`06-instance-readiness-record.md`](06-instance-readiness-record.md)：当前实例每个 operation 的实际配置与验收记录。
7. [`07-provider-landing-and-decision-process.md`](07-provider-landing-and-decision-process.md)：Provider 落地、覆盖矩阵和“施工前必须讨论”的决策流程。
8. [`plans/01-live-provider-enablement.md`](plans/01-live-provider-enablement.md)：可直接执行的测试先行施工计划。

## 状态词

| 状态            | 含义                                                               |
| --------------- | ------------------------------------------------------------------ |
| Contract Only   | 只有 schema、DTO 或 validator，不存在可执行生产路由                |
| Framework Ready | 目录、mapping、dispatch 和安全合同存在，但实例未配置 Provider      |
| Configured      | 当前实例存在受信、hash 一致、字段映射完整的 operation binding      |
| Healthy         | Configured 且真实探测成功，未处于持续失败或熔断状态                |
| Operational     | Healthy 且正式 Run 的 snapshot、evidence、finalization、恢复均通过 |
| Degraded        | 有安全 fallback 或冻结备用 Provider，且 UI 明确说明降级            |
| WebFallback     | 仅 News：无结构化 binding，但普通 Web 可用且通过确定性校验         |
| Unavailable     | 没有合规路由；必须失败关闭，不能从模型记忆猜测                     |

## 使用方式

1. 先读文档 01，确认代码环境和实际运行实例没有混淆。
2. 从文档 02 选择一个 Confirmed 问题，不跨阶段同时修改多个事实源。
3. 按文档 03 的 operation 粒度设计，不使用粗粒度 `web.domain.read` 代替真实可用性。
4. 按文档 04 的顺序施工，并执行 `plans/01` 中对应任务；每个阶段先关闭对应决策门。
5. 进入阶段 5 前必须读文档 07，为每个 operation 完成并确认 Provider Decision Record。
6. 只有文档 05 的软件门禁和实例门禁都通过，才能更新 Operational 状态。

## 非目标

- 不建设第二套 Provider registry、证据仓库或工具目录。
- 不硬编码商业服务商、Endpoint、API Key 或凭证。
- 不把普通网页搜索包装成天气、行情、排片或比分结构化接口。
- 不为了达到 11/11 数量而放宽时效、地域、单位和来源校验。
- 不用 live API 测试替代可重复的本地 contract fixture，也不用 fixture 替代真实实例验收。
