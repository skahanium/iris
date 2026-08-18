# Iris 文档索引

**v1.2.22** 为当前开发版本。版本排期唯一来源是 [ROADMAP.md](../ROADMAP.md)；已被实现替代的设计、审计和施工计划通过 Git 历史查阅，不在工作树中充当现行规范。

## 现行规范

| 文档                                                             | 用途                                 |
| ---------------------------------------------------------------- | ------------------------------------ |
| [README.md](../README.md)                                        | 产品边界、快速开始与开发入口         |
| [ROADMAP.md](../ROADMAP.md)                                      | 唯一版本排期与当前里程碑             |
| [ARCHITECTURE.md](../ARCHITECTURE.md)                            | 当前模块、数据流、单向兼容与安全边界 |
| [ipc-api-reference.md](./ipc-api-reference.md)                   | 当前 Tauri IPC 契约                  |
| [design-system.md](./design-system.md)                           | 界面 token、组件规范与人工验收       |
| [adaptive-workspace.md](./adaptive-workspace.md)                 | v1.2.19 自适应工作区状态与交互契约   |
| [rss-subscription-library.md](./rss-subscription-library.md)     | RSS 订阅资料库产品、数据与安全契约   |
| [markdown-export.md](./markdown-export.md)                       | 编辑器 Markdown 往返与保留节点语义   |
| [markdown-indexing-contract.md](./markdown-indexing-contract.md) | 编辑器与索引器的当前解析边界         |
| [llm-routing.md](./llm-routing.md)                               | LLM 配置、连通性与联网证据           |
| [testing/](./testing/)                                           | 可执行的手工验收清单                 |
| [ops/](./ops/)                                                   | 运维、性能与能力降级手册             |

## 当前施工资料

| 文档                                                                                                   | 用途                                   |
| ------------------------------------------------------------------------------------------------------ | -------------------------------------- |
| [Agent Harness 可靠性重构](../refactor/README.md)                                                      | 当前差距、可靠性契约、实施顺序与验收   |
| [MiniMax 协议与发布门禁加固计划](./superpowers/plans/2026-08-11-minimax-protocol-release-hardening.md) | 流式隐私、模型能力与发布门禁施工证据   |
| [RSS 订阅资料库实施计划](./superpowers/plans/2026-08-11-rss-subscription-library.md)                   | 全周期阶段、测试先行任务与发布门禁     |
| [RSS 订阅资料库人工清单](./testing/rss-subscription-library-manual-checklist.md)                       | 全尺寸、双平台、隐私、升级与回滚验收   |
| [v1.2.19 自适应工作区实施计划](./superpowers/plans/2026-07-31-v1.2.19-adaptive-workspace.md)           | 测试先行任务、文件边界与交付顺序       |
| [v1.2.19 自适应工作区人工清单](./testing/v1.2.19-adaptive-workspace-manual-checklist.md)               | 分辨率、主题、键盘、生命周期与真机验收 |

## 保留的评测资料

`docs/eval/` 中的 fixture、结果和评测定义可复现历史实验；其中的结论均为**历史结果，不代表当前架构状态**。

## 维护规则

1. 修改版本范围只更新 `ROADMAP.md`，并按需更新已完成事实的 CHANGELOG。
2. 修改 IPC 时同步 Rust command、`src/types/ipc.ts`、`src/lib/ipc.ts`、测试和 IPC 参考。
3. 修改 TipTap schema 或 Markdown 链路时同步 round-trip corpus 与 `markdown-export.md`。
4. 当前文档不得链接历史设计目录，也不得描述不存在的模块、命令或事件。
