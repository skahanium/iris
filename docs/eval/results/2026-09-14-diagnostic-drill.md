# 已知故障诊断演练：目录外工具提议触发两轮拒绝收束（`D01` 证据 2）

本文件是 [D01 建立可信基线与最小诊断](../../../agent-harness/implementation/D01-trustworthy-baseline.md) §三回归证据第 2 条要求的**一次已知故障的诊断演练记录**：能指出发现位置、直接失败、恢复结果与待证根因。

它只记录**可在当前基线上核对的动作与结论**，不声明付费实网或语义质量已通过，也不把历史故障压缩成一个未经验证的唯一根因。

---

## 一、被演练的故障

| 项         | 值                                                                                          |
| ---------- | ------------------------------------------------------------------------------------------- |
| 现象       | 初始搜索与抓取均成功后，模型连续两轮提出目录外工具，最终进入 `recovery_exhausted`           |
| 历史登记   | [AGENT-REFORM-DISCUSSION-2026-09-14.md](../../../AGENT-REFORM-DISCUSSION-2026-09-14.md):170 |
| 已登记事实 | 该次记录为**三轮模型调用、两次工具调用**后退出（同上 :174）                                 |
| 影响边界   | `C11` 调用身份、`C16` 工具面版本、`C17` 拒绝原因（`current-baseline.md` `Q01`）             |

**已核对的实际 Run（供复盘引用，不能当作该故障）**：本机 dev 库 `.iris-dev/app-data/iris.db` 中，2026-09-14 09:40（本地 +08:00，UTC `2026-09-14T01:40:42Z`）记录的 `run_id` 为 `ada3d10e-7221-44b3-aea5-2ddd9b014f2a`，`effort=tool_loop`。其 `tool_audit` 为 `web_search`（成功，5454 ms）与 `web_fetch`（成功，10039 ms），与「初始搜索和抓取成功」一致。

**口径更正（必须记录）**：该 Run 的 `agent_run_events` 到 `completed` 为止，`content_delta` 产出的是正常答复，**没有** `capability_degraded`，也没有未派发提议的痕迹。因此「连续两轮目录外工具 + `recovery_exhausted`」的那一次运行**不能**用上述 `run_id` 指认；它出现在同一 db 的相近时段，但当时库早于 migration `075`，没有 `audit_boundary_events`，无法用 `assistant_run_diagnose` 回放。`Q01` 的调查结论是：历史那一次永久无法唯一归因。

同类故障的可定位性用**当前代码**复现，见第三节。

## 二、触发机制（可直接核对的源码位置）

两轮拒绝收束是一条**独立规则**，与网络额度是两条规则：

| 观察点                       | 位置                                                                             |
| ---------------------------- | -------------------------------------------------------------------------------- |
| 本轮提议全部未派发           | `src-tauri/src/ai_runtime/agent_tool_loop.rs:1410-1414`                          |
| 未派发计数 `rejected_rounds` | `agent_tool_loop.rs:1428`                                                        |
| 写入修复事件                 | `agent_tool_loop.rs:1429`（`{"event":"repair","round":N}`）                      |
| 达阈值即要求收束             | `agent_tool_loop.rs:1430-1434`                                                   |
| 退出原因                     | `agent_tool_loop.rs:1641`（`rejected_rounds >= 2` → `recovery_exhausted`）       |
| 退出诊断载荷                 | `agent_tool_loop.rs:1618-1626`（含 `modelTurns`／`toolCalls`／`rejectedRounds`） |

**未派发的提议不按执行路径增加网络工具计数**：`agent_tool_loop.rs:1437` 只在有派发时清零，未派发轮次走 `continue`，不经过派发记账。

## 三、诊断如何定位（`C26` + `C27`，含当前代码端到端）

事件先由 `C26` 追加写入 `audit_boundary_events`（migration `075_audit_boundary_events.sql`），再由 `C27` 的 `diagnose_run` 解释为可查询报告。IPC `assistant_run_diagnose` 调用的就是该函数。

| 演练要求     | 报告字段                                                                                          | 由何产生                                                            |
| ------------ | ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| **发现位置** | `path[]`、`findings[].discovery`（module／component／toolInstance／callId／attemptId／modelTurn） | `diagnostic_query.rs` 的 `discovery_of`                             |
| **直接失败** | `directFailures[]`                                                                                | `tool_error` / `result.success=false`                               |
| **恢复结果** | `recoveryResults[]`                                                                               | `recovery_exhausted` **只**写成「恢复耗尽，这是恢复结果而不是根因」 |
| **待证根因** | `pendingRootCauses[]`                                                                             | 有 C11 解析 hop 且来源不是 `Unattributed` 时                        |
| **证据缺口** | `evidenceGaps[]`                                                                                  | 缺解析 hop：来源不可归属，进缺口而**不**进待证根因                  |
| **归因状态** | `attributionStatus`、`recordCompleteness`                                                         | `DiagnosticReport`                                                  |
| **影响**     | `auditHealth.persistFailed` 为独立健康信号                                                        | 不得把写入失败伪装成空成功                                          |

### 待证根因的判据（本故障的关键）

工具名来源由 `tool_name_origin.rs` 的 `infer_origin` 判定，缺解析 hop 直接落到 `Unattributed`：

| 来源                   | 报告结论                                                                   |
| ---------------------- | -------------------------------------------------------------------------- |
| 缺 `C11` 解析 hop      | 进 `evidenceGaps`，「工具名来源链缺少网关解析 hop，不能归因于模型」        |
| `ModelGenerated`       | 进 `pendingRootCauses`，「来源为模型生成（**疑似**），不能证实为模型故障」 |
| `ProtocolParsed`       | 「可能来自协议解析改写，不能归因于模型」                                   |
| `NameMapped`（被拒绝） | 「工具名经过名称映射，不能归因于模型」                                     |
| `PromptConvention`     | 「目录内名称未出现在本次工具面，属提示约定或表面错位，不能归因于模型」     |

### 当前代码端到端（2026-09-16）

测试 `diagnose_run_locates_unknown_tool_two_round_exhaustion_without_attributing_the_model` 在迁移后的内存库中：

1. `AgentRunRepository::accept` 建立普通域 Run（`run_id` 固定为 `d01-unknown-tool-e2e`，供报告对照）。
2. 经生产写入 API `record_loop_event` 落入 `audit_boundary_events`：成功的 `web_search`／`web_fetch` 结果 → 两轮目录外 `unknown_tool` 提议（无 C11 hop）→ 两次 `repair` → `exit.reason=recovery_exhausted`。
3. 调用 `diagnose_run`（与 `assistant_run_diagnose` 同一查询，绑定会话与 `run_id`）。

2026-09-16 本机（Windows 10）`cargo test --lib diagnostic_query`：**29 passed / 0 failed**，含上述用例。锁定的报告合同：

| 字段                        | 断言                                              |
| --------------------------- | ------------------------------------------------- |
| `runId`                     | `d01-unknown-tool-e2e`                            |
| `path[]`                    | 非空，且含 `component=C14`                        |
| `recoveryResults[]`         | 含「恢复耗尽」与「修复尝试」                      |
| `evidenceGaps[]`            | 含 `issueClass=diagnostic-gap` 且陈述含「来源链」 |
| `pendingRootCauses[]`       | 不含 `issueClass=model-behavior`                  |
| `directFailures[]`          | 空（未派发的未知工具不是执行失败）                |
| `attributionStatus`         | 不是 `confirmed`                                  |
| `auditHealth.persistFailed` | `false`                                           |
| 标题                        | 不得含「未发现可证实的故障」                      |

**结论**：诊断能给出发现位置与恢复结果，并把「为何产生未知工具名」标为**证据缺口**（缺 hop 时），而**不**写成已证实的模型故障。这是 `Q01`／`Q12` 要求的可定位性。历史 09-14 那次仍不能事后唯一归因。

## 四、证据与执行

| 项         | 值                                                                                                                                                                                                      |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 机械证据   | `cargo test --manifest-path src-tauri/Cargo.toml --lib diagnostic_query`                                                                                                                                |
| 结果       | **29 passed / 0 failed**（2026-09-16 本机，Windows 10）                                                                                                                                                 |
| 直接对位   | `diagnose_run_locates_unknown_tool_two_round_exhaustion_without_attributing_the_model`、`recovery_exhausted_is_recovery_not_confirmed_root_cause`、`unknown_tool_without_parse_hop_is_unattributed_gap` |
| 来源链     | `cargo test --lib tool_name_origin` → **19 passed / 0 failed**                                                                                                                                          |
| 记录路径   | `cargo test --lib boundary_events` → **11 passed / 0 failed**                                                                                                                                           |
| 界面投影   | `npx vitest run tests/assistant-run-diagnostic-v07.test.tsx` → **8 passed / 0 failed**（`V07`）                                                                                                         |
| 检查器负例 | `npm run agent-harness:test` → **22 passed / 0 failed**（`X02`）                                                                                                                                        |
| 评分口径   | `cargo test --lib headless_smoke_summary_exposes_only_the_closed_contract` → **1 passed**                                                                                                               |

## 五、本轮明确不声称的范围

1. **不对 2026-09-14 历史 Run 做端到端重放**：该库没有 `audit_boundary_events`。这是 `Q01` 的调查结论，不是本包还缺的实现。
2. **未运行付费实网评测**，未生成新的 live 结果。
3. **界面投影证据由 `V07` 的组件级测试提供**，不等于完整诊断工作台已验收。
4. **本演练的落库路径是生产 `record_loop_event` + 生产 `diagnose_run`**，不是桌面 UI 里点一次诊断按钮；IPC 命令只做会话绑定后转调同一函数。
