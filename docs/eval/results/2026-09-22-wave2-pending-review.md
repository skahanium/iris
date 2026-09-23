# 第二波核对清单（已入库）

**状态**：`REV-2026-09-22-MIMO-51`／`REV-2026-09-22-MIMO-52` 已入库。原施工提交在 `backup-wave2-local` 的 `06ab01a4`。本文件是复核前核对清单的留底，不是权威存储。权威在 `registry-history.json` 的 `reviews[]`。

## 并表改号（远程 `000393d5` 为主）

`origin/branch-v1.3.1`（`000393d5`）已占用 `CHG/REV-2026-09-22-93`／`94`／`95`（第五波日记与 R14）。第二波拆表在并表时改号；**不要把远程 94／95 读成第二波**。

| 原编号（`06ab01a4`）                     | 并表后现行编号                                      | 远程同号实际是 |
| ---------------------------------------- | --------------------------------------------------- | -------------- |
| `CHG-2026-09-22-94`／`REV-2026-09-22-94` | `CHG-2026-09-23-96`／`REV-2026-09-23-96`／`MIMO-51` | R14 架构变更   |
| `CHG-2026-09-22-95`／`REV-2026-09-22-95` | `CHG-2026-09-23-97`／`REV-2026-09-23-97`／`MIMO-52` | R14 独立复核   |

`MIMO-51`／`52` 的 `change` 指针已改号；`objects[]` 指纹未改。不解封 `CHG-83`／`84`。

- **变更作者**：`cursor-grok-4.6`（与第一波 catalog／日记迁移同一实体，不能自签）
- **分类**：`governance`
- **两条治理变更**（`isIndependentReview` 要求 `review.change === change.id`，所以要两条复核记录，不能一条绑完）：
  - `CHG-2026-09-23-96` / `REV-2026-09-23-96`：快照与流水拆表、closed-issue 禁残留 blocks
  - `CHG-2026-09-23-97` / `REV-2026-09-23-97`：规则文档 Prettier 折行后的当前指纹（仅 `P01#file`、`X01`）
- **入库结论**：`synchronized`（mimo；`reviews.author=mimo`，`reviews.reviewer=cursor-grok-4.6`）
- **不解封**：`CHG-2026-09-21-83`、`CHG-2026-09-21-84`（`REV-2026-09-21-84` → `V02`／`D05#file` 必须仍在 gaps）
- **不授权**：不得把本波当成 D04／D05／C25／K15／N04 已验收；不得预批「其后」内核减法或 V05 live

`--reconcile` 写入的 `needs-reverification`（`REV-2026-09-23-96`／`97`）仍留在流水里，不能当作完成依据。解封靠 MIMO-51／52。

## 当前指纹

| 对象     | 当前 revision | 当前 fingerprint                   | 出现在           |
| -------- | ------------- | ---------------------------------- | ---------------- |
| P01#file | 0             | `4e7f3a5f33d46314fbd1d0ebcb1d0aee` | 96 已 stale → 97 |
| P03#file | 0             | `eada0bde491a87e5708d68a6f50f63d9` | 96               |
| V08      | 12            | `de14538f1439db6162db1aafca887c43` | 96               |
| X01      | 16            | `b7b4038f55ff507e6502460f4cefc758` | 96 已 stale → 97 |
| X02      | 14            | `0ed8e97c3f4f4c619a60121beae0f131` | 96               |
| X03      | 11            | `e3d49ff5d70a04bdae59098c211d1868` | 96               |

## 核对清单（复核时已核）

1. `registry.json` 只留当前快照；`changes`／`reviews`／obsolete verify 在 `registry-history.json`。
2. 流水文件不进 `catalog.files`，reconcile 不因此打漂容器指纹。
3. obsolete verify 只保留 object／kind／fingerprint／applicability／一行原因，指纹未改写。
4. 建立期 `CHG-2026-09-15-*` 整段在流水文件；`notes` 有指针。
5. `issues.state=closed` 残留 catalog `blocks` 会被 `X01` 检出（负例在 `agent-harness-registry.test.mjs`）。
6. 拆开的快照缺少 history 文件时，检查器按基础设施失败，不装成通过。
7. 独立复核分别覆盖 96 与 97；当前指纹以上表为准。`objects` **不要**绑定 `V02`／`D05#file`。
