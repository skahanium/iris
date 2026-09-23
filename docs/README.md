# Iris 文档索引

**v1.3.0** 为当前开发版本。版本排期唯一来源是 [ROADMAP.md](../ROADMAP.md)。

文档按五类组织：**现行规范**描述当前实现与契约，**设计与讨论**区分讨论依据和尚未全部落地的目标架构，
**评测与验收**规定门禁怎么跑、人工怎么验，**施工计划**只在飞，**归档**保存已被取代的材料。已被实现替代的设计、审计与计划一律进入归档，
不留在现行目录；归档文件不再维护，也不作为规范依据。

## 现行规范

| 文档                                                                     | 用途                                  |
| ------------------------------------------------------------------------ | ------------------------------------- |
| [README.md](../README.md)                                                | 产品边界、快速开始与开发入口          |
| [agent-harness/README.md](../agent-harness/README.md)                    | Agent／Harness 详细规范与施工依据入口 |
| [ROADMAP.md](../ROADMAP.md)                                              | 唯一版本排期与当前里程碑              |
| [ARCHITECTURE.md](../ARCHITECTURE.md)                                    | 当前模块、数据流、单向兼容与安全边界  |
| [ipc-api-reference.md](./ipc-api-reference.md)                           | 当前 Tauri IPC 契约                   |
| [design-system.md](./design-system.md)                                   | 界面 token、组件规范与人工验收        |
| [design-system/brand.md](./design-system/brand.md)                       | 品牌色、印记与视觉资产                |
| [adaptive-workspace.md](./adaptive-workspace.md)                         | v1.2.19 自适应工作区状态与交互契约    |
| [rss-subscription-library.md](./rss-subscription-library.md)             | RSS 订阅资料库产品、数据与安全契约    |
| [markdown-export.md](./markdown-export.md)                               | 编辑器 Markdown 往返与保留节点语义    |
| [markdown-indexing-contract.md](./markdown-indexing-contract.md)         | 编辑器与索引器的当前解析边界          |
| [llm-routing.md](./llm-routing.md)                                       | LLM 配置、连通性与联网证据            |
| [ops/performance-guide.md](./ops/performance-guide.md)                   | 性能预算、测量方法与调优入口          |
| [ops/web-capability-degradation.md](./ops/web-capability-degradation.md) | 联网能力降级的用户可见行为与诊断      |

## 设计与讨论

目标设计不等于当前实现，讨论中的建议也不自动成为设计决定；以各文档的日期、状态和职责范围为准。

| 文档                                                           | 用途                                                                 |
| -------------------------------------------------------------- | -------------------------------------------------------------------- |
| [Agent 革新讨论纪要](../AGENT-REFORM-DISCUSSION-2026-09-14.md) | 用户需求、历史审查与技术取舍的讨论依据                               |
| [Agent 架构定义](./agent-architecture.md)                      | 目标模块、组件、工具、状态所有权与交接合同；不声明实现完成或版本排期 |
| [审计报告](./audit-report-2026-09.md)                          | 前端 / Markdown / Agent 衔接审计发现与复核修正后的排期建议           |
| [产品愿景](./product-vision.md)                                | 对话即笔记、无侧栏、落盘单调与呈现多态的形态原则                     |

## 评测与验收

| 文档                                                                                                           | 用途                                                 |
| -------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| [eval/agent-answer-capacity.md](./eval/agent-answer-capacity.md)                                               | Agent 门禁执行口径：证据层级、核心矩阵、压力与安全轨 |
| [eval/semantic-search.md](./eval/semantic-search.md)                                                           | 语义检索评测口径与夹具边界                           |
| [eval/rag-v2-broker-evaluation.md](./eval/rag-v2-broker-evaluation.md)                                         | RAG v2 broker 分层检索评测                           |
| [eval/results/](./eval/results/)                                                                               | 版本化确定性评测结果                                 |
| [eval/fixtures/](./eval/fixtures/)                                                                             | 哈希校验的评测夹具（测试数据，不是文档）             |
| [testing/iris-ui-manual-checklist.md](./testing/iris-ui-manual-checklist.md)                                   | 窗口外壳、工作区、Agent 与 Overlay 人工验收          |
| [testing/desktop-release-runbook.md](./testing/desktop-release-runbook.md)                                     | 桌面发版手册                                         |
| [testing/assistant-streaming-acceptance.md](./testing/assistant-streaming-acceptance.md)                       | 流式显示验收：合成回放、回归证据与双平台边界         |
| [testing/harness-timeliness-acceptance.md](./testing/harness-timeliness-acceptance.md)                         | 时效性检索验收：实际检索、调用恢复与答案质量边界     |
| [testing/document-open-runtime-manual-checklist.md](./testing/document-open-runtime-manual-checklist.md)       | 冷/热打开、加载面与索引竞争验收                      |
| [testing/document-persistence-embedding-acceptance.md](./testing/document-persistence-embedding-acceptance.md) | 文档持久化与嵌入索引验收                             |
| [testing/app-close-manual-checklist.md](./testing/app-close-manual-checklist.md)                               | 应用关闭与 macOS 首次升级手工迁移验收                |
| [testing/rss-subscription-library-manual-checklist.md](./testing/rss-subscription-library-manual-checklist.md) | RSS 全尺寸、双平台、隐私与回滚验收                   |

## 施工计划

在飞计划只描述任务分解与当时的核验记录，**不自行声明完成度**。版本状态以 [ROADMAP.md](../ROADMAP.md) 为准；Agent／Harness 的阶段状态与工作包就绪由 [agent-harness/](../agent-harness/README.md) 承载（D01–D06），旧体系见[归档清单](../agent-harness/archive/2026-09-15-pre-reform/MANIFEST.md)。

| 文档                                                                                                   | 用途                                                    |
| ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------- |
| [Agent / RAG 可靠性修复计划](./superpowers/plans/2026-08-07-iris-agent-rag-reliability-remediation.md) | 检索与 Agent 可靠性的在飞修复契约与阻断项               |
| [Agent 正确性与联网回答质量修复方案](./superpowers/plans/2026-09-14-agent-correctness-remediation.md)  | 2026-09-14 复审后的读取、终态、检索、联网质量与文档修正 |
| [RSS 订阅资料库实施计划](./superpowers/plans/2026-08-11-rss-subscription-library.md)                   | RSS 资料库的在飞任务分解与发布门禁                      |

## 归档

归档只作对照，其中的模块、测试与结论可能已不存在。除本索引外，现行文档不得把归档材料作为依据引用。

| 文档                                                                               | 用途                                   |
| ---------------------------------------------------------------------------------- | -------------------------------------- |
| [archive/plans/MANIFEST.md](./archive/plans/MANIFEST.md)                           | 已落地／已撤回施工计划的清单与归档原因 |
| [Harness 统一前归档](../agent-harness/archive/2026-08-pre-unification/MANIFEST.md) | 统一前 Harness 设计材料的受控归档      |
| [Harness 重建前归档](../agent-harness/archive/2026-09-15-pre-reform/MANIFEST.md)   | 重建前施工文档的受控归档（2026-09-15） |

## 维护规则

1. 修改版本范围只更新 `ROADMAP.md`，并按需更新已完成事实的 CHANGELOG。
2. 修改 IPC 时同步 Rust command、`src/types/ipc.ts`、`src/lib/ipc.ts`、测试和 IPC 参考。
3. 修改 TipTap schema 或 Markdown 链路时同步 round-trip corpus 与 `markdown-export.md`。
4. 当前文档不得把归档目录作为规范性依据，也不得描述不存在的模块、命令或事件；归档只能经本索引进入，且归档目录下每个文件都必须在对应 `MANIFEST.md` 登记。
5. 新增、改名或删除文档时同步更新本索引；`scripts/docs-facts-check.mjs` 会校验索引完整性与链接可达性。
