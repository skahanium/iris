# D05 十任务确定性战役

**证据层级**：`headless_deterministic`（V03／V04）+ 既有确认卡片 V07。  
**基线**：治理闭合提交 `7842ddaf` 之上的工作树；题面固定，禁止中途换题。  
**不是**：D05／N04／Q14／C24／C25 验收；不是 V05 live；不扩 52 题问答矩阵；不写 `product-gate.json`。

权威机械入口：`src-tauri/src/ai_runtime/d05_ten_task_campaign_tests.rs`。  
T08 前端折叠／懒加载**只引用**既有 `tests/assistant-run-confirmation-diff.test.tsx`，本战役不重写 UI 测。

## 命令与分母

| 层                     | 命令                                                                          |                         分母 |
| ---------------------- | ----------------------------------------------------------------------------- | ---------------------------: |
| 生产入口 V03／V04      | `cargo test --manifest-path src-tauri/Cargo.toml --lib d05_ten_task_campaign` | 10 题（T01–T10，T08 后端半） |
| 确认卡片 V07（T08 UI） | `npx vitest run tests/assistant-run-confirmation-diff.test.tsx`               |        6（既有套件，不新增） |

空分母不得记满分。本文件的通过只表示上表命令在本机复现；不把 `verification.state` 标 `passed`，不关工作包。

## 固定题面

| ID  | 用户任务                                                                     | 期望轨迹／错误码                                                                                                                                                | 落点             |
| --- | ---------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- |
| T01 | 纯润色「将这段话润色成正式通知。」且 `web_enabled=true`                      | 信封 `freshness=Offline`，`web_reason=LocalTransformation`，capabilities 不含 `web.search`                                                                      | Intake／Q14      |
| T02 | 「请核实这篇」且 `web_enabled=true`                                          | 仍授予 `web.search`，`web_reason≠LocalTransformation`                                                                                                           | Intake／Q14 对照 |
| T03 | 「润色：请核实这篇笔记的出处」且 `web_enabled=true`                          | 冒号剥离后仍用**全文**判定核实，授予 `web.search`                                                                                                               | 第一波不变量     |
| T04 | 闲聊「你好」且 `web_enabled=true`                                            | `freshness=WebPreferred`，`web_reason=DefaultOnline`，不是 LocalTransformation                                                                                  | 分类器边界       |
| T05 | 「请格式整理这段笔记」，候选只改标题级别／列表标记，正文／顺序／链接目标保持 | `check_format_preservation` 通过；生产入口仍冻结确认（`confirmation_pending`），不写盘                                                                          | C23／N04 机械面  |
| T06 | 同上请求，候选改掉散文或链接目标                                             | 工具结果 `format_preservation_unproven`，确认数 0，磁盘未改                                                                                                     | 同上负例         |
| T07 | 选区替换润色 → `prepare_edit_candidate`／生产 `replace_selection`            | 确认事件仅 `summary`+`targets`，无笔记正文；未批准磁盘不变                                                                                                      | C24              |
| T08 | 确认仍 pending、磁盘未漂，调用 `preview_pending_confirmation_diff`           | `previewable=true`，有界 unified diff；预览前后持久化事件 JSON 不变（瞬时 IPC 不进事件）                                                                        | R14／C24         |
| T09 | 冻结后改磁盘再预览                                                           | 该文件 `previewable=false`；执行已确认变更不得把候选写上漂移磁盘                                                                                                | R14              |
| T10 | Host 限制结束                                                                | Run `status=completed`（不是 failed）+ `taskOutcome=blocked`；`classify_task_outcome` 只读 Host 事实，没有 `capability_degraded` 输入，限制优先于「变更已完成」 | K15／C25         |

## 每题结果

本机复现（工作树在 `7842ddaf` 之上）：`cargo test --manifest-path src-tauri/Cargo.toml --lib d05_ten_task_campaign` → 11 passed（T01–T10 + T08 引用契约）；`npx vitest run tests/assistant-run-confirmation-diff.test.tsx` → 6 passed。

| ID  | 结果 | 缺口                                                                                                                               |
| --- | ---- | ---------------------------------------------------------------------------------------------------------------------------------- |
| T01 | pass |                                                                                                                                    |
| T02 | pass |                                                                                                                                    |
| T03 | pass |                                                                                                                                    |
| T04 | pass |                                                                                                                                    |
| T05 | pass |                                                                                                                                    |
| T06 | pass |                                                                                                                                    |
| T07 | pass |                                                                                                                                    |
| T08 | pass | UI 半：既有 V07 6/6 pass，未重写                                                                                                   |
| T09 | pass | 写盘拒绝码为 `frozen_change_base_hash_drift`（Approved 后核验 base hash；非 Dispatching 的 `frozen_change_write_receipt_unknown`） |
| T10 | pass |                                                                                                                                    |

T08 UI 半引用：`AssistantConfirmationDiff` 默认折叠、展开才调用 `assistantRunConfirmationDiff`、失败静默回退。命令见上表 V07 行。

## 明确不做

- 不关 Q14／N04／D05／D04／C24／C25
- 不解封 CHG-83／CHG-84，不绑 V02／D05#file
- 不改 D05 合同正文
- 不加 SQLite 列、不跑 `agent:eval:live`
- 不把本战役写成 D05 第六波产品施工
