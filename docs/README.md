# Iris 文档索引

阅读顺序建议：**路线图（排期）→ 设计系统（界面）→ 架构（实现）→ 贡献/Agent（施工）**。

---

## 文档层级

| 层级             | 文档                                   | 职责                         |
| ---------------- | -------------------------------------- | ---------------------------- |
| **门面**         | [README.md](../README.md)              | 项目介绍、快速开始、设计哲学 |
| **排期（唯一）** | [ROADMAP.md](../ROADMAP.md)            | 版本里程碑                   |
| **界面**         | [design-system.md](./design-system.md) | Notion N token、组件、C 原则 |
| **实现**         | [ARCHITECTURE.md](../ARCHITECTURE.md)  | 分层、数据流、IPC、安全      |
| **变更**         | [CHANGELOG.md](../CHANGELOG.md)        | 版本变更记录                 |
| **施工**         | [AGENTS.md](../AGENTS.md)              | AI/人协作硬约束、命令速查    |
| **协作**         | [CONTRIBUTING.md](../CONTRIBUTING.md)  | 环境、PR、测试、Commit       |
| **安全**         | [SECURITY.md](../SECURITY.md)          | 漏洞报告                     |

**原则**：不在多份文档里重复维护「版本排期」；细节以 ROADMAP 为准，其他文档只引用并补充本域内容。

---

## 当前版本

**v1.1.0**。功能建设已超越原 v0.x 规划，剩余未完成项见 [ROADMAP § v1.1.0](../ROADMAP.md#v110--稳定发布无限期延后)。

---

## 专题

| 主题                   | 文档                                                                                       |
| ---------------------- | ------------------------------------------------------------------------------------------ |
| 使用指南（用户向）     | Notion 官方文档站（无限期延后，URL 待发布）                                                |
| IPC API 参考           | [ipc-api-reference.md](./ipc-api-reference.md)                                             |
| 设计系统 · Notion N    | [design-system.md](./design-system.md)                                                     |
| Notion 参考摘要        | [design-system/notion-master.md](./design-system/notion-master.md)                         |
| 品牌图标               | [design-system/brand.md](./design-system/brand.md)                                         |
| 编辑器 Markdown 导出   | [markdown-export.md](./markdown-export.md)                                                 |
| LLM 路由与连通性       | [llm-routing.md](./llm-routing.md)                                                         |
| 语义搜索与 Recall@5    | [eval/semantic-search.md](./eval/semantic-search.md)                                       |
| 语义评测 fixture vault | [eval/fixtures/semantic-vault/](./eval/fixtures/semantic-vault/)                           |
| 审计与清理记录         | [audits/2026-06-11-project-review-v1.1.0.md](./audits/2026-06-11-project-review-v1.1.0.md) |
| 关闭回归手工验收       | [testing/app-close-manual-checklist.md](./testing/app-close-manual-checklist.md)           |
| 品牌图标               | [../scripts/assets/README.md](../scripts/assets/README.md)                                 |

---

## 历史文档

v0.1.0–v0.5.2 期间的 Epic、施工计划、设计 spec、品牌规划已归档至 [docs/history/](./history/)。这些文件保留决策背景，可能与当前实现不一致，不作为当前事实来源；界面以 [design-system.md](./design-system.md) 为准。

`docs/plans/`、`docs/superpowers/plans/` 与 `docs/superpowers/specs/` 是施工过程资料。引用其中内容前必须先核对 README、ROADMAP、ARCHITECTURE 与当前代码。

---

## 维护约定

1. **新增版本里程碑**：只改 `ROADMAP.md`，并在本表「当前版本」更新状态。
2. **架构/IPC 变更**：`ARCHITECTURE.md` + `src/types/ipc.ts` + `AGENTS.md` §4.2。
3. **发布**：`CHANGELOG.md` + ROADMAP 版本状态 + git tag。
4. **设计系统变更**：`design-system.md` + `ROADMAP.md` 版本 checklist。
