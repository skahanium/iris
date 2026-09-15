# 重建前 Agent Harness 施工文档归档清单

**归档日期：2026-09-15。替代入口：待重建，尚未确定。**

本目录保存本轮文档体系重建之前的 Agent Harness 施工文档（README + 01–06 + 附录 A/B/C）。
文件通过 Git 重命名迁入，正文保持归档时原貌；其中的相对链接、状态、提交号、实例配置和版本判断可能失效。

## 归档原因

该文档体系此前被声明为全仓库 Agent Harness 事实与 HR 阶段状态的唯一权威
（见归档时 `ROADMAP.md` 第 62 行与 `docs/README.md` 的现行规范表）。按
`AGENT-REFORM-DISCUSSION-2026-09-14.md` 重建文档体系，需要先撤下这套施工文档及其权威声明，
避免旧体系与新体系并存形成第二份规范。

## 来源

| 原路径 | 归档路径 | 归档原因 |
| --- | --- | --- |
| `/README.md` | `README.md` | 施工文档体系整体退役，待重建 |
| `/01-authority-and-invariants.md` | `01-authority-and-invariants.md` | 同上 |
| `/02-current-state-and-debt.md` | `02-current-state-and-debt.md` | 同上 |
| `/03-target-architecture.md` | `03-target-architecture.md` | 同上 |
| `/04-adaptive-agent-loop-and-tool-contracts.md` | `04-adaptive-agent-loop-and-tool-contracts.md` | 同上 |
| `/05-implementation-roadmap.md` | `05-implementation-roadmap.md` | 同上 |
| `/06-evaluation-performance-and-acceptance.md` | `06-evaluation-performance-and-acceptance.md` | 同上 |
| `/appendices/A-status-and-test-traceability.md` | `appendices/A-status-and-test-traceability.md` | 同上 |
| `/appendices/B-task-capability-and-risk-matrix.md` | `appendices/B-task-capability-and-risk-matrix.md` | 同上 |
| `/appendices/C-decisions-and-deferred.md` | `appendices/C-decisions-and-deferred.md` | 同上 |

## 有效内容去向

**尚未确定。** 待重建的文档体系按 `AGENT-REFORM-DISCUSSION-2026-09-14.md` 讨论定稿后回填本表。
在此之前，本目录不承担任何现行权威职责。

## 与本次归档配套的变更

- `scripts/docs-facts-check.mjs` 中针对本套施工文档的断言（现行文件清单、状态头、双向链接、
  撤回主张扫描）已随体系退役一并停用；归档本身的登记与完整性仍受校验。
- `docs/README.md`、`ROADMAP.md`、`ARCHITECTURE.md`、`docs/eval/`、`docs/superpowers/plans/`
  中原先指向现行入口的链接已改指本清单。

归档文件不得被 `ROADMAP.md`、`ARCHITECTURE.md` 或当前施工文档引用为现行事实。
