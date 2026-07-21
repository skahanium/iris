# Iris 文档索引

**v1.2.13** 为当前开发版本。版本排期只由 [ROADMAP.md](../ROADMAP.md) 维护；历史计划、过期审计和已替代规格请通过 git 历史查阅。

## 核心文档

| 文档                                  | 用途                             |
| ------------------------------------- | -------------------------------- |
| [README.md](../README.md)             | 产品边界、快速开始与开发入口     |
| [ROADMAP.md](../ROADMAP.md)           | 唯一版本排期与当前里程碑         |
| [ARCHITECTURE.md](../ARCHITECTURE.md) | 当前模块、数据流、迁移与安全边界 |
| [CHANGELOG.md](../CHANGELOG.md)       | 已交付的版本变更                 |
| [SECURITY.md](../SECURITY.md)         | 凭据安全与漏洞报告               |
| [AGENTS.md](../AGENTS.md)             | 施工约束、测试和提交规范         |

## 专题文档

| 主题                                       | 文档                                                   |
| ------------------------------------------ | ------------------------------------------------------ |
| UI token、组件与人工验收                   | [design-system.md](./design-system.md)                 |
| IPC 契约与变更流程                         | [ipc-api-reference.md](./ipc-api-reference.md)         |
| LLM 配置、连通性和联网证据                 | [llm-routing.md](./llm-routing.md)                     |
| Agent Harness 目标规格与施工计划           | [agent-harness-refactor/](./agent-harness-refactor/)   |
| Markdown 导出                              | [markdown-export.md](./markdown-export.md)             |
| 语义/混合检索评测                          | [eval/semantic-search.md](./eval/semantic-search.md)   |
| 关闭、文档打开、保存/嵌入与 Iris Rail 验收 | [testing/](./testing/)                                 |
| 性能排查                                   | [ops/performance-guide.md](./ops/performance-guide.md) |
| 品牌规范                                   | [design-system/brand.md](./design-system/brand.md)     |

历史 TaskPlan 设计材料已从当前工作树移出，必要时通过 git 历史查阅：
`2026-06-21-agent-harness-taskplan-blueprint-design.md`、
`2026-06-21-agent-harness-taskplan-blueprint.md`。

## 维护规则

1. 新增或调整版本范围：只改 `ROADMAP.md`，并按需同步 CHANGELOG 的已完成事实。
2. 修改 IPC：同步 Rust command、`src/types/ipc.ts`、`src/lib/ipc.ts` 与本索引指向的 IPC 文档。
3. 修改 UI：先更新设计系统与 ROADMAP，再更新样式和组件。
4. 修改安全、凭据或外部网络边界：同步 `SECURITY.md`、`ARCHITECTURE.md` 和相关专题文档。
5. 修改 Markdown 写入、关闭/更新协议或嵌入调度：同步 [文档持久化与嵌入验收](./testing/document-persistence-embedding-acceptance.md)、相关手工清单与 IPC 参考；不得把派生索引或嵌入失败表述为 Markdown 保存失败。
6. 旧计划、临时审计、代理状态文件不作为工作树文档体系的一部分。

Agent Harness 重构文档集是经用户确认保留的当前目标规格，不属于历史临时计划；在实施完成前不得将其中的目标结构写入 `ARCHITECTURE.md` 作为当前事实。
