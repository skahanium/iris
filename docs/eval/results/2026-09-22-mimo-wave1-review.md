# 独立复核：治理剂量校正第一波（`CHG-2026-09-22-92`）

本文件是 `REV-2026-09-22-MIMO-50` 的可追溯评审记录。它**不是**复核权威存储；权威在 `registry-history.json` 的 `reviews[]`（第二波拆表后检查器读合并视图）。

- **复核者**：`mimo`（未参与 `CHG-2026-09-22-92` 编写）
- **变更作者**：`cursor-grok-4.6`
- **对象**：方案「Harness dose correction」第一波 + 提交 `1f2ae2cf` + `CHG-2026-09-22-92`
- **结论**：`synchronized`（细化；有条件，条件已写入 notes，不阻断 refinement 定性）
- **不解封**：`CHG-2026-09-21-83`、`CHG-2026-09-21-84`（现存缺口是 `REV-2026-09-21-84` → `V02`／`D05#file`）；不授权第二波拆表；不追认 D04／D05／C25／K15／N04 已验收

## 核验摘要

对照方案「做 / 明确不做」逐项通过：阅读面与 catalog 一致；closed 项残留 `blocks` 已撤，现存 8 条 open 阻断边；日记迁出 `D05#file` 并改两栏口径；K15／ROADMAP 归档引用已收口；`agent-harness-show` 可用；D04／D05 仍 `active` 且 `verification=none`。

质量门（第一波提交时）：`agent-harness:check` PASSED（96 文件／195 对象）；`agent-harness:test` 39 pass；`docs:check` PASSED。`format:check` 在 15 个品牌／布局文件上红，**不是本波文件集、不是本波引入**；不得把四门写成全绿。

日记各波是压缩执行事实，不是合同原文逐字搬迁；「不把 `capability_degraded` 映射为 `partial`」在第四波跨波锁定。catalog 撤 `blocks` 未进入 `CHG-2026-09-22-92.objects[]`（reconcile 只记指纹变化），`reason` 已写明；不改写该变更历史 `objects[]`。

## 字段语义

入库时 `reviews[].author = mimo`（复核者），`reviews[].reviewer = cursor-grok-4.6`（变更编写方）。两者同填 `mimo` 会使检查器判定非独立，不得原样照抄聊天稿。

`REV-2026-09-22-MIMO-50.objects` **不**绑定 `V02`／`D05#file`：`openStaleBindings` 按对象 ID 跨变更密封，这两项的当前指纹若被独立复核写入，会按 `P03` §5.2 规则 8 清掉 `REV-2026-09-21-84` 缺口。对它们的阅读面核验留在本文与 `reason`，不解封 `CHG-83`／`CHG-84`。本条不把密封规则改成按变更隔离——那是治理口径，不并入本收口。施工方不得再改写本条 `objects[]`；若绑定面必须收缩，由复核者重签。
