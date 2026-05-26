# Iris 文档索引

阅读顺序建议：**路线图（排期）→ 设计系统（界面）→ 架构（实现）→ 贡献/Agent（施工）**。

---

## 文档层级

| 层级 | 文档 | 职责 |
|------|------|------|
| **门面** | [README.md](../README.md) | 项目介绍、快速开始、设计哲学摘要 |
| **排期（唯一）** | [ROADMAP.md](../ROADMAP.md) | 版本里程碑：**功能 + 体验** 同一表；产品/体验非目标；待定特色 |
| **界面** | [design-system.md](./design-system.md) | 纸墨（B）/ 命令（C）；token；与版本对照的阶段 0～3 |
| **实现** | [ARCHITECTURE.md](../ARCHITECTURE.md) | 分层、数据流、IPC、安全、AI 线框；配色摘要指向 design-system |
| **变更** | [CHANGELOG.md](../CHANGELOG.md) | 已发布版本与 `[Unreleased]` |
| **施工** | [AGENTS.md](../AGENTS.md) | AI/人协作硬约束、命令速查 |
| **协作** | [CONTRIBUTING.md](../CONTRIBUTING.md) | 环境、PR、测试、Commit |
| **安全** | [SECURITY.md](../SECURITY.md) | 漏洞报告 |

**原则**：不在多份文档里重复维护「版本排期」；细节以 ROADMAP 为准，其他文档只引用并补充本域内容。

---

## 按版本查阅

| 版本 | 功能 / 体验说明 | 施工 / 审计 |
|------|-----------------|-------------|
| **v0.1.0**（已发布） | [ROADMAP § v0.1.0](../ROADMAP.md#v010--ai-原生-mvp) | [v0.1.0-epic.md](./v0.1.0-epic.md)、[v0.1.0-completion-prs.md](./v0.1.0-completion-prs.md) |
| **v0.1.1**（已发布） | [ROADMAP § v0.1.1](../ROADMAP.md#v011--体验定稿与质量补齐) | [v0.1.1-epic.md](./v0.1.1-epic.md) |
| **v0.2.0**（已发布） | [ROADMAP § v0.2.0](../ROADMAP.md#v020--知识网络) | 知识网络 + 纸墨阶段 1 + sqlite-vec |
| **v0.3.0**（已实现） | [ROADMAP § v0.3.0](../ROADMAP.md#v030--安全与版本) | 版本系统、冲突解决、模板、导出 |
| **v0.3.1-ui**（体验） | [ROADMAP § v0.3.1-ui](../ROADMAP.md#v031-ui--命令浮层与纸墨抛光) | [UI 实现计划](./plans/2026-05-26-ui-overlay-refresh.md) |
| **v1.0.0** | [ROADMAP § v1.0.0](../ROADMAP.md#v100--完整发布) | 国际化、无障碍、体验收尾 |

---

## 专题

| 主题 | 文档 |
|------|------|
| 语义搜索与 Recall@5 | [eval/semantic-search.md](./eval/semantic-search.md) |
| 语义评测 fixture vault | [eval/fixtures/semantic-vault/](./eval/fixtures/semantic-vault/) |
| 文档版本 / 历史（B+ 双层保存） | [plans/2026-05-26-document-version-design.md](./plans/2026-05-26-document-version-design.md) |
| 命令浮层与纸墨抛光（v0.3.1-ui） | [plans/2026-05-26-ui-overlay-refresh.md](./plans/2026-05-26-ui-overlay-refresh.md) |
| 品牌图标 | [../scripts/assets/README.md](../scripts/assets/README.md) |

---

## 体验（纸墨 B + 命令 C）

- **方向与 token**：[design-system.md](./design-system.md)
- **路线图绑定**：[ROADMAP § 体验方向](../ROADMAP.md#体验方向与路线图绑定)
- **线框（引用卡、AI 面板）**：[ARCHITECTURE § 上下文块](../ARCHITECTURE.md)

修改界面时：先更新 `design-system.md` 与 ROADMAP 对应版本 checklist，再改 `src/styles/globals.css` 与组件。

---

## 维护约定

1. **新增版本里程碑**：只改 `ROADMAP.md`，并在此表「按版本查阅」增加一行；必要时新增 `docs/v0.x.x-epic.md`。
2. **合并 v0.1.0 类补齐 PR 后**：更新 completion-prs 状态表，并勾选 ROADMAP 复选框。
3. **架构/IPC 变更**：`ARCHITECTURE.md` + `src/types/ipc.ts` + `AGENTS.md` §4.2。
4. **发布**：`CHANGELOG.md` + ROADMAP 版本状态一句 + git tag。
