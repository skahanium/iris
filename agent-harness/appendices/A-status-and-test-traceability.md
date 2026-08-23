# 附录 A：状态与测试追踪

本表只把当前仓库中真实存在的命名测试作为能力证据。执行结果以当前工作树实际运行记录为准；“全量测试通过”不能替代对应能力的命名测试。

## 1. 当前追踪表

| ID        | 能力                               | 实现状态 | 处置 | 命名测试或验证命令                                                                                                                                                                   | 当前证据                                                    |
| --------- | ---------------------------------- | -------- | ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------- |
| RUN-001   | Run 幂等与单航班                   | 已验证   | 保留 | `same_session_concurrent_start_admits_only_one_active_run`; `accepted_retry_does_not_spawn_again`                                                                                    | 2026-08-22 全量 Rust 通过                                   |
| RUN-002   | durable finalization 与 sink 恢复  | 已验证   | 保留 | `direct_streaming_does_not_emit_answer_complete_before_durable_finalization`; `terminal_sink_failure_recovers_without_reexecution`                                                   | 2026-08-22 全量 Rust 通过                                   |
| UI-001    | 跨 Run 正文、frame 与恢复隔离      | 已验证   | 保留 | `new_run_never_projects_previous_reveal_answer`; `queued_previous_run_frame_cannot_patch_new_run`; `terminal_recovery_uses_only_its_own_persisted_answer`                            | 2026-08-22 Vitest 2460/2460                                 |
| TOOL-001  | 冻结工具表面与执行门禁             | 已验证   | 保留 | `capabilities_read_reports_current_surface_only`; `empty_tool_surface_rejects_forged_tool_call`                                                                                      | 2026-08-22 全量 Rust 通过                                   |
| SEC-001   | Web 查询污染和诊断脱敏             | 已验证   | 保留 | `tool_diagnostics_never_expose_raw_arguments`; local-to-Web taint 负例                                                                                                               | 2026-08-22 全量 Rust 通过                                   |
| EVID-001  | 当前 Run evidence 与来源组语义     | 已验证   | 保留 | `current_run_citation_links_exclude_foreign_and_retired_evidence`; `direct_required_web_run_persists_source_group_binding_when_markers_are_missing`                                  | 2026-08-22 全量 Rust 通过                                   |
| TIME-001  | runtime 日期不联网                 | 已验证   | 保留 | `today_date_question_uses_trusted_runtime_without_web`                                                                                                                               | 2026-08-22 全量 Rust 通过                                   |
| WEB-001   | 首轮搜索、gap 与 query 去重        | 部分实现 | 重构 | `insufficient_first_search_triggers_bounded_refinement`; `supplement_without_gap_is_rejected_after_initial_prefetch`; `duplicate_normalized_query_is_rejected_even_when_gap_changes` | 搜索侧已覆盖，深抓取预算未接通                              |
| WEB-002   | 模型后续抓取与自适应 profile       | 部分实现 | 重构 | AH-2 新增表驱动测试                                                                                                                                                                  | `max_fetches: 0` 仍阻断目标合同                             |
| ROUTE-001 | 精确事实与研究型任务路由           | 部分实现 | 重构 | AH-3 task-shape matrix                                                                                                                                                               | 当前仍有领域级前置失败和 News 特例                          |
| DOM-001   | 11 operation DTO/mapping/validator | 已验证   | 保留 | `production_domain_operations_freeze_authorize_dispatch_and_recover_table_driven`; `stale_weather_and_market_data_fail_closed`                                                       | 2026-08-22 全量 Rust 通过；fixture 不证明实例配置           |
| DOM-002   | 真实结构化 Provider                | 延期     | 保留 | 每个真实试点单独 PDR 与 instance evidence                                                                                                                                            | 2026-08-19 历史快照 0/11 configured                         |
| EVAL-001  | Windows 路径安全与 fixture parity  | 已验证   | 保留 | `node --test scripts/agent-eval.test.mjs`; real stdio search；single Web case                                                                                                        | 2026-08-22：8/8，两个定向 Rust 测试通过                     |
| EVAL-002  | deterministic smoke                | 已验证   | 保留 | `npm run agent:eval:smoke`                                                                                                                                                           | 2026-08-22：24/24                                           |
| EVAL-003  | deterministic full                 | 已验证   | 保留 | `npm run agent:eval`                                                                                                                                                                 | 2026-08-22：48-case、压力阶梯、硬边界、安全轨和组合终端通过 |
| DEBT-001  | 单一研究循环与 dead-code 清理      | 计划中   | 移除 | 静态唯一性扫描、clippy、全量测试                                                                                                                                                     | AH-2 至 AH-4 同步完成                                       |

## 2. 初始删除追踪

| 目标                                                      | 当前问题                            | 替代合同                                 | 删除时机                        |
| --------------------------------------------------------- | ----------------------------------- | ---------------------------------------- | ------------------------------- |
| `domain_operation_is_executable` 的缺 binding 前置失败    | 研究型领域问题无法进入模型/Web loop | task-shape routing + evidence contract   | AH-3 替代测试变绿后同阶段删除   |
| `constrain_domain_tool_surface` 的 News/Web 特例          | 通用研究能力被领域路径隐藏          | 单一 `web_search` surface                | AH-3 同阶段删除特例和旧断言     |
| Direct 首次预取后无充分性判断的提前返回                   | 单次搜索被误当成研究完成            | deterministic sufficiency + bounded loop | AH-2 同阶段收窄                 |
| `max_fetches: 0`                                          | 模型无法按证据缺口深抓取            | profile remaining fetch budget           | AH-2 预算测试变绿后删除常量行为 |
| 未消费的 `max_fetches`/`max_repairs` 字段                 | 产生虚假预算安全感                  | 接通或删除                               | AH-2 完成前处理                 |
| 非 News 无 binding 必须模型前失败的测试                   | 固化旧架构方向                      | exact/research task matrix               | AH-3 同阶段替换                 |
| `fresh_domains` 模块级 `allow(dead_code)`                 | 隐藏不可达分支                      | 可达性审计与精确局部许可                 | AH-4 删除                       |
| 11/11 readiness、管理中心、通用 REST 和 failover 近期目标 | 扩大维护面且无真实 Provider         | 单 Provider PDR 试点                     | 现行文档已移除，代码不预建      |

## 3. 更新规则

- 测试改名、移动或删除时同步本表。
- 当前测试失败时，`已验证` 必须立即降为 `已实现待复验` 或 `部分实现`。
- 实例状态必须带审计日期、数据目录和脱敏查询口径；fixture 永远不能把实例状态升级为 Operational。
- 一个替代能力未删除对应旧分支时，只能标为部分实现。
