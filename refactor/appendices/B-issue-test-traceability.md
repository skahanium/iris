# 附录 B：问题到测试追踪

本表只列仓库中真实存在、能直接证明相应契约的测试。状态以测试执行结果为准；文档不以宽泛 E2E 或相近名称替代定向证据。上表中的第一轮 Resolved 测试是持续回归基线，不等于下方第二轮问题已经完成。

| 问题 ID  | 状态     | 实际测试                                                                         | 证明边界                                                                                  |
| -------- | -------- | -------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| RUN-001  | Resolved | `same_session_concurrent_start_admits_only_one_active_run`                       | 同会话并发首次启动只接受一个活动 Run，另一请求返回稳定错误码                              |
| RUN-001  | Resolved | `same_session_concurrent_retry_admits_only_one_active_run`                       | 两个不同 retry ID 不能并发创建两个执行权                                                  |
| RUN-001  | Resolved | `accepted_retry_does_not_spawn_again`                                            | 同一 `client_request_id` 与 intake 指纹重放返回原 Run，`is_new=false` 且不重复发 Accepted |
| RUN-001  | Resolved | `classified_request_replay_does_not_spawn_again`                                 | 涉密临时 Run 同 ID、同请求指纹重放复用身份且不重复启动                                    |
| RUN-002  | Resolved | `direct_streaming_does_not_emit_answer_complete_before_durable_finalization`     | AnswerComplete 只能在最终消息与 Completed 持久化后投影                                    |
| RUN-003  | Resolved | `terminal_sink_failure_recovers_without_reexecution`                             | 终态 sink 失败后从持久化事实恢复，副作用计数不增加                                        |
| RUN-003  | Resolved | `窗口重新获得焦点时重放仍显示为非终态的 Run`                                     | 前端仅在 focus 时调用 `assistant_run_get` 重放非终态 Run                                  |
| RUN-003  | Resolved | `rejected_confirmation_cancels_without_write`                                    | 拒绝确认后终态为 Cancelled，且无目标写入、最终消息或 AnswerComplete                       |
| RUN-003  | Resolved | `rejected_confirmation_recovery_stays_cancelled`                                 | 历史 confirmation rejected / Run active 不一致恢复为 Cancelled                            |
| TOOL-001 | Resolved | `capabilities_read_empty_surface_returns_no_tools`                               | 空表面返回空目录，不回退到完整 catalog                                                    |
| TOOL-001 | Resolved | `capabilities_read_reports_current_surface_only`                                 | 能力读取只报告当前 Run 允许表面                                                           |
| TOOL-002 | Resolved | `harness_tools_do_not_inherit_vault_search_permission`                           | harness 工具不继承不相关的 vault.search 权限                                              |
| TOOL-003 | Resolved | `empty_tool_surface_rejects_forged_tool_call`                                    | 伪造工具调用返回 `tool_not_in_run_surface` 且不到达 dispatch                              |
| TOOL-003 | Resolved | `internal_web_prefetch_allows_only_web_search`                                   | 严格 Web 预取表面只允许 `web_search`                                                      |
| TOOL-004 | Resolved | `catalog_owns_execution_metadata`                                                | 成本、输出和证据策略只来自 `ToolCatalogEntry`                                             |
| EVID-001 | Resolved | `direct_required_web_run_persists_source_group_binding_when_markers_are_missing` | Direct 严格 Web 无精确标记时仍持久化 `SourceGroupFallback`                                |
| EVID-002 | Resolved | `labels an uncalibrated source group as this-run retrieval sources`              | UI 标为“本次检索来源组”，不暗示精确引用或逐段核验                                         |
| EVID-003 | Deferred | `structured_verifier_requires_registered_rule`                                   | 仅证明无注册规则不能晋升 VERIFIED；通用语义校验不进入完成标准                             |
| EVID-004 | Resolved | `current_run_citation_links_exclude_foreign_and_retired_evidence`                | foreign/retired 证据不能成为当前 Run 引用                                                 |
| CTX-002  | Resolved | `first_user_message_is_not_permanent_goal`                                       | 首条历史用户消息不会被永久提升为当前目标                                                  |
| CTX-003  | Resolved | `stale_summary_is_revalidated_before_context_assembly`                           | 上下文组装前复核摘要；覆盖范围内变更会刷新或清除                                          |
| CTX-003  | Resolved | `messages_after_summary_range_do_not_invalidate_existing_summary`                | 覆盖范围后的新消息不使旧摘要失效，并保留在近期窗口                                        |
| MEM-001  | Resolved | `memory_scope_precedence_is_vault_then_global`                                   | 精确 key 与列表读取均按 vault 优先于 global，且同名去重                                   |
| MEM-001  | Resolved | `confirmed_memory_clear_is_scope_local`                                          | clear_scope 只清理明确指定作用域并返回 `affectedCount`                                    |
| MEM-002  | Resolved | `unconfirmed_memory_mutation_is_not_persisted`                                   | 未确认的 upsert/delete/clear 均不会 dispatch 或落库                                       |
| UI-002   | Resolved | `tool_diagnostics_never_expose_raw_arguments`                                    | 工具事件和审计不含原始参数、笔记正文哨兵或凭证哨兵                                        |
| UI-003   | Resolved | `new_run_never_projects_previous_reveal_answer`                                 | 新 Run 首帧和处理期间不投影上一 Run reveal                                                |
| UI-003   | Resolved | `queued_previous_run_frame_cannot_patch_new_run`                                | 上一 Run 排队 frame/event 不能修改新 Run 行                                               |
| UI-003   | Resolved | `terminal_recovery_uses_only_its_own_persisted_answer`                          | 终态恢复只使用同 Run 持久化正文                                                           |

| ROUTE-003 | Partial  | `today_date_question_uses_trusted_runtime_without_web`           | 单测证明 runtime 规则；生产 intake/domain 路由仍待补充 |
| ROUTE-003 | Partial  | `recent_movie_question_freezes_date_and_location`                | 计划 04 需证明地点真实传入生产研究计划 |
| WEB-001   | Partial  | `insufficient_first_search_triggers_bounded_refinement`          | 已证明预搜索计数、gap 预算和 resume state 恢复；未证明 Provider attempt/winner 生产夹具 |
| WEB-001   | Partial  | `sufficient_first_search_stops_without_extra_tool_turn`          | 已证明单测提前停止；未证明 provider failover 不扩大轮次 |
| EVID-005  | Partial  | `strict_current_fact_rejects_unsupported_free_text`              | validator 单测通过；生产终局接线待完成 |
| EVID-005  | Partial  | `source_group_fallback_cannot_complete_strict_current_fact`      | 来源组不能替代当前事实支持 |
| EVID-005  | Partial  | `unsupported_finalization_protocol_never_falls_back_to_guessing` | 协议不支持时失败关闭 |
| EVAL-002  | Partial  | `current_fact_movie_follow_up_scenario`                          | 固定场景存在；需重新接入生产结构化/Web 路径 |
| EVAL-002  | Partial  | `agent_does_not_deny_web_after_current_run_search`               | 已搜索能力诚实性仍需生产路径复验 |
| CAP-001   | Partial  | `domain_tool_output_requires_source_and_observed_time`            | DTO 单测保留；需改为真实 evidence ledger 证据 |
| CAP-001   | Resolved | `weather_without_confirmed_city_requests_location`               | 天气缺城市时询问，不推断位置                                              |
| CAP-001   | Resolved | `location_scope_widens_city_then_province_then_country`          | 允许放宽的领域遵守固定地域顺序                                            |
| CAP-001   | Resolved | `stale_weather_and_market_data_fail_closed`                      | 陈旧天气/行情不产生当前结论                                              |
| CAP-001   | Resolved | `movie_availability_requires_region_channel_and_date`            | 影视可用性必须包含地域、渠道和日期                                        |
| CAP-001   | Resolved | `finance_analysis_cannot_introduce_unsupported_numbers`          | 描述性金融分析不引入证据外数值                                            |
| CAP-001   | Resolved | `domain_tool_diagnostics_never_expose_raw_output`                | provider 原始 JSON 哨兵不进入 Run event、tool audit、UI error、eval report |

| INPUT-001 | Partial | `input_submission_resumes_the_same_run_and_replay_is_noop` | 控制层证明同一 Run 恢复与重复提交幂等；生产执行路径待补 |
| WEB-002 | Partial | `supplement_without_gap_is_rejected_after_initial_prefetch`; `duplicate_normalized_query_is_rejected_even_when_gap_changes`; `fresh_research_resume_state_restores_budget_and_query_deduplication` | 已证明补搜缺 gap、规范化重复查询拒绝及 resume state 恢复；真实 Provider 尝试待补 |
| CAP-002 | Partial | `structured_weather_without_provider_fails_closed_instead_of_using_web_fallback` | 证明天气无结构化 Provider 不伪装通用 Web；11 operation 与 Host 渲染待补 |
| EVID-006 | Partial | `current_fact_finalization` production gate; `domain_tool_output_requires_source_and_observed_time` | 终局 gate 与 DTO 规则存在；Provider evidence 身份哨兵和完整登记夹具待补 |

## 追踪规则

- 测试改名或迁移文件时必须同步本表，并以 `rg` 和测试运行结果重新核实。
- 一个问题可以由多个层级测试覆盖，但不得把不存在的目标测试写成已通过。
- 只有对应测试通过后，附录 A 才能把 RUN-003、MEM-001、UI-002 等状态标记为 Resolved。
- EVID-003 保持 Deferred：本轮只验 fail-closed 门；未来结构化工具规则和自由文本实验见附录 C。
