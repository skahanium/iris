# 附录 B：问题到测试追踪

本表只列仓库中真实存在、能直接证明相应契约的测试。状态以测试执行结果为准；文档不以宽泛 E2E 或相近名称替代定向证据。

| 问题 ID | 状态 | 实际测试 | 证明边界 |
| --- | --- | --- | --- |
| RUN-001 | Resolved | `same_session_concurrent_start_admits_only_one_active_run` | 同会话并发首次启动只接受一个活动 Run，另一请求返回稳定错误码 |
| RUN-001 | Resolved | `same_session_concurrent_retry_admits_only_one_active_run` | 两个不同 retry ID 不能并发创建两个执行权 |
| RUN-001 | Resolved | `accepted_retry_does_not_spawn_again` | 同一 retry 重放返回原 Run，`is_new=false` 且不重复发 Accepted |
| RUN-001 | Resolved | `classified_request_replay_does_not_spawn_again` | 涉密临时 Run 同请求重放复用身份且不重复启动 |
| RUN-002 | Resolved | `direct_streaming_does_not_emit_answer_complete_before_durable_finalization` | AnswerComplete 只能在最终消息与 Completed 持久化后投影 |
| RUN-003 | Resolved | `terminal_sink_failure_recovers_without_reexecution` | 终态 sink 失败后从持久化事实恢复，副作用计数不增加 |
| RUN-003 | Resolved | `窗口重新获得焦点时重放仍显示为非终态的 Run` | 前端仅在 focus 时调用 `assistant_run_get` 重放非终态 Run |
| RUN-003 | Resolved | `rejected_confirmation_cancels_without_write` | 拒绝确认后终态为 Cancelled，且无目标写入、最终消息或 AnswerComplete |
| RUN-003 | Resolved | `rejected_confirmation_recovery_stays_cancelled` | 历史 confirmation rejected / Run active 不一致恢复为 Cancelled |
| TOOL-001 | Resolved | `capabilities_read_empty_surface_returns_no_tools` | 空表面返回空目录，不回退到完整 catalog |
| TOOL-001 | Resolved | `capabilities_read_reports_current_surface_only` | 能力读取只报告当前 Run 允许表面 |
| TOOL-002 | Resolved | `harness_tools_do_not_inherit_vault_search_permission` | harness 工具不继承不相关的 vault.search 权限 |
| TOOL-003 | Resolved | `empty_tool_surface_rejects_forged_tool_call` | 伪造工具调用返回 `tool_not_in_run_surface` 且不到达 dispatch |
| TOOL-003 | Resolved | `internal_web_prefetch_allows_only_web_search` | 严格 Web 预取表面只允许 `web_search` |
| TOOL-004 | Resolved | `catalog_owns_execution_metadata` | 成本、输出和证据策略只来自 `ToolCatalogEntry` |
| EVID-001 | Resolved | `direct_required_web_run_persists_source_group_binding_when_markers_are_missing` | Direct 严格 Web 无精确标记时仍持久化 `SourceGroupFallback` |
| EVID-002 | Resolved | `labels an uncalibrated source group as this-run retrieval sources` | UI 标为“本次检索来源组”，不暗示精确引用或逐段核验 |
| EVID-003 | Deferred | `structured_verifier_requires_registered_rule` | 仅证明无注册规则不能晋升 VERIFIED；通用语义校验不进入完成标准 |
| EVID-004 | Resolved | `current_run_citation_links_exclude_foreign_and_retired_evidence` | foreign/retired 证据不能成为当前 Run 引用 |
| CTX-002 | Resolved | `first_user_message_is_not_permanent_goal` | 首条历史用户消息不会被永久提升为当前目标 |
| CTX-003 | Resolved | `stale_summary_is_revalidated_before_context_assembly` | 上下文组装前复核摘要；覆盖范围内变更会刷新或清除 |
| CTX-003 | Resolved | `messages_after_summary_range_do_not_invalidate_existing_summary` | 覆盖范围后的新消息不使旧摘要失效，并保留在近期窗口 |
| MEM-001 | Resolved | `memory_scope_precedence_is_vault_then_global` | 精确 key 与列表读取均按 vault 优先于 global，且同名去重 |
| MEM-001 | Resolved | `confirmed_memory_clear_is_scope_local` | clear_scope 只清理明确指定作用域并返回 `affectedCount` |
| MEM-002 | Resolved | `unconfirmed_memory_mutation_is_not_persisted` | 未确认的 upsert/delete/clear 均不会 dispatch 或落库 |
| UI-002 | Resolved | `tool_diagnostics_never_expose_raw_arguments` | 工具事件和审计不含原始参数、笔记正文哨兵或凭证哨兵 |

## 追踪规则

- 测试改名或迁移文件时必须同步本表，并以 `rg` 和测试运行结果重新核实。
- 一个问题可以由多个层级测试覆盖，但不得把不存在的目标测试写成已通过。
- 只有对应测试通过后，附录 A 才能把 RUN-003、MEM-001、UI-002 等状态标记为 Resolved。
- EVID-003 保持 Deferred：本轮只验 fail-closed 门；未来结构化工具规则和自由文本实验见附录 C。
