# 独立复核：治理剂量校正第二波（现行 `CHG-2026-09-23-96`／`CHG-2026-09-23-97`）

本文件是 `REV-2026-09-22-MIMO-51`／`REV-2026-09-22-MIMO-52` 的可追溯评审记录。它**不是**复核权威存储；权威在 `registry-history.json` 的 `reviews[]`（检查器读合并视图）。

并表改号：远程 `000393d5` 已占用 `CHG/REV-2026-09-22-93`／`94`／`95`（第五波日记与 R14）。下文若仍写「94／95」，均指第二波原编号，现行为 `CHG-2026-09-23-96`／`97`。`change` 指针已改号；`objects[]` 指纹未改。

- **复核者**：`mimo`（未参与 96／97 编写；原施工编号 94／95）
- **变更作者**：`cursor-grok-4.6`
- **对象**：方案「Harness dose correction」第二波；施工提交 `06ab01a4`（基线 `9374d367`）
- **结论**：两条均为 `synchronized`（治理）
- **不解封**：`CHG-2026-09-21-83`、`CHG-2026-09-21-84`（缺口仍是 `REV-2026-09-21-84` → `V02`／`D05#file`）；不授权 D04／D05／C25／K15／N04 已验收；不预批「其后」

## 核验摘要

核对清单 1–7 通过：快照／流水分离；流水不进 `catalog.files`；obsolete 压缩不改写指纹；建立期 `CHG-2026-09-15-*` 在流水且 notes 有指针；closed-issue 残留 `blocks` 由 X01 检出；缺 history 按基础设施失败。分类 `governance` 恰当。自签禁止执行到位。

质量门（入库后）：`agent-harness:check` PASSED（97 文件／195 对象），缺口只剩 CHG-84 两项；`agent-harness:test` 含身份负例；`docs:check` PASSED；`size:check` PASSED。

## 字段语义

入库时 `reviews[].author = mimo`（复核者），`reviews[].reviewer = cursor-grok-4.6`（变更编写方）。P03 §5.2 写明：`reviews[].reviewer` 是参与该变更编写的身份，用来核对独立性。检查器要求 `review.author ≠ change.author` **且** `review.author ≠ review.reviewer`。两者同填 `mimo` 会使检查器判定非独立，不得原样照抄聊天稿。负例见 `scripts/agent-harness-registry.test.mjs`。

`objects` **不**绑定 `V02`／`D05#file`：`openStaleBindings` 按对象 ID 跨变更密封。96 的 `P01#file`／`X01` 标 `stale: true`（当时 To），当前覆盖在 97。

施工方不得改写已入库独立复核的 `objects[]`。若绑定面必须收缩，由复核者重签，不由施工方裁剪。`REV-2026-09-22-MIMO-50` 当时为挡住规则 8 未绑 `V02`／`D05#file`，本波不回改那条绑定面。

## 当前指纹（入库时复算一致）

| 对象     | revision | fingerprint                        | 覆盖    |
| -------- | -------- | ---------------------------------- | ------- |
| P01#file | 0        | `4e7f3a5f33d46314fbd1d0ebcb1d0aee` | MIMO-52 |
| P03#file | 0        | `eada0bde491a87e5708d68a6f50f63d9` | MIMO-51 |
| V08      | 12       | `de14538f1439db6162db1aafca887c43` | MIMO-51 |
| X01      | 16       | `b7b4038f55ff507e6502460f4cefc758` | MIMO-52 |
| X02      | 14       | `0ed8e97c3f4f4c619a60121beae0f131` | MIMO-51 |
| X03      | 11       | `e3d49ff5d70a04bdae59098c211d1868` | MIMO-51 |
