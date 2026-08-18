# 附录 B：问题到测试追踪

本表为目标测试清单。测试文件名可按现有模块组织调整，但测试语义和问题 ID 应保留。只有代码中仍可确认的问题才进入必做列。

| 问题 ID   | 目标测试                                                                         | 层级           | 通过条件                                                                                                                |
| --------- | -------------------------------------------------------------------------------- | -------------- | ----------------------------------------------------------------------------------------------------------------------- |
| RUN-001   | `concurrent_retry_starts_executor_once`                                          | 仓储/服务集成  | 两个并发接受只返回一个 `is_new=true`，副作用计数为 1（已新增，GREEN）                                                   |
| RUN-001   | `accepted_retry_does_not_spawn_again`                                            | 服务单元       | 已接受 retry 返回已有 Run 且不调用 spawn                                                                                |
| RUN-001   | `retry_replay_emits_accepted_event_only_once`                                    | intake 回归    | 同一 retry 重放只发出一个 accepted 事件（已新增，GREEN）                                                                |
| RUN-002   | `answer_complete_follows_persisted_terminal_state`                               | 最终化集成     | 事件顺序晚于消息和 Completed 持久化                                                                                     |
| RUN-002   | `message_persist_failure_never_emits_complete`                                   | 故障注入       | 写入失败时无成功事件或成功 UI 状态                                                                                      |
| RUN-002   | `direct_streaming_does_not_emit_answer_complete_before_durable_finalization`     | 服务回归       | AnswerComplete 观察到消息与 Completed 均已持久化                                                                        |
| RUN-003   | `sink_disconnect_recovers_without_reexecution`                                   | 恢复集成       | 重连恢复同一终态且工具调用计数不增加                                                                                    |
| RUN-003   | `rejected_confirmation_cancels_without_write`                                    | 服务集成       | reason 为 `user_rejected_change` 且无笔记写入                                                                           |
| ROUTE-001 | `executor_uses_frozen_tool_surface`                                              | 服务单元       | Run 中途设置变化不产生第二套路由结果（`ToolSurfacePlan.tool_names` 已接入，GREEN）                                      |
| TOOL-001  | `capabilities_read_reports_current_surface_only`                                 | 工具集成       | Planned、未授权和当前模型不支持的工具均不出现（已新增，GREEN）                                                          |
| TOOL-002  | `reasoning_tools_have_no_vault_search_permission_alias`                          | 权限单元       | 遗留工具不再继承不相关权限                                                                                              |
| TOOL-003  | `all_dispatch_paths_apply_same_gate`                                             | 工具集成       | 正常、恢复、旁路调用均执行同一权限/确认检查（`unexposed_tool_call_*` 与 `executor_rejects_model_call_*` 已新增，GREEN） |
| TOOL-003  | `web_disabled_blocks_every_external_path`                                        | 安全集成       | 外部请求计数为 0                                                                                                        |
| TOOL-004  | `unknown_or_unused_parameters_are_rejected`                                      | schema 单元    | 未声明/无语义字段产生稳定错误码；内部无消费方工具不暴露（`static_metadata_defines_*` 已新增，GREEN）                    |
| EVID-001  | `direct_strict_web_emits_source_group_fallback`                                  | Run 集成       | 无精确绑定时 binding 明确为 fallback                                                                                    |
| EVID-001  | `tool_loop_uncalibrated_emits_source_group_fallback`                             | Run 集成       | 保持现有正确行为                                                                                                        |
| EVID-001  | `direct_required_web_run_persists_source_group_binding_when_markers_are_missing` | 服务回归       | Direct 严格 Web 缺少标记时仍保存 fallback                                                                               |
| EVID-002  | `missing_binding_renders_unverified_source_group`                                | 前端组件       | 标题和说明不暗示精确引用（已新增，GREEN）                                                                               |
| EVID-002  | `unknown_binding_version_fails_safe`                                             | 前端组件       | 未知版本仍按未逐段核验展示（已新增，GREEN）                                                                             |
| EVID-003  | `structured_verifier_requires_registered_rule`                                   | 校验单元       | 无规则不能晋升 VERIFIED（已新增，GREEN）                                                                                |
| EVID-003  | `structured_verifier_checks_units_time_and_source`                               | 校验单元       | 任一必要字段不一致即保持 uncalibrated/失败                                                                              |
| EVID-004  | `exact_binding_rejects_stale_or_foreign_run_evidence`                            | 证据集成       | 旧 Run/失效证据不能成为 Exact                                                                                           |
| SEC-001   | `permission_mapping_matches_catalog_capability`                                  | 目录单元       | 每个可执行工具映射与目录声明一致                                                                                        |
| TOOL-002  | `harness_tools_do_not_inherit_vault_search_permission`                           | 权限单元       | harness 工具不再返回 `VaultSearch`                                                                                      |
| SEC-002   | `local_retrieval_content_never_enters_web_query`                                 | 隐私集成       | 捕获的 URL/query 不含夹具中的敏感标记                                                                                   |
| SEC-002   | `untrusted_web_text_cannot_expand_permissions`                                   | 安全集成       | 注入文本无法启用工具或跳过确认                                                                                          |
| CTX-001   | `run_situation_uses_committed_projection`                                        | 上下文单元     | 草稿、旧临时结果和旧权限不进入投影                                                                                      |
| CTX-002   | `first_user_message_is_not_permanent_goal`                                       | 回归单元       | 后续无关请求不继承首条消息为目标（已新增，当前 RED）                                                                    |
| CTX-003   | `summary_invalidates_when_covered_messages_change`                               | 摘要集成       | 删除/修改范围内消息后旧摘要不再使用                                                                                     |
| CTX-003   | `short_conversation_does_not_require_summary`                                    | 上下文单元     | 预算内直接使用原消息                                                                                                    |
| MEM-001   | `same_memory_key_can_exist_in_different_scopes`                                  | migration/仓储 | 两个 scope 的同 key 互不覆盖                                                                                            |
| MEM-001   | `clear_scope_preserves_other_scopes`                                             | 仓储单元       | 清理精确且可重复                                                                                                        |
| MEM-002   | `unconfirmed_inference_is_not_persisted`                                         | 记忆服务       | 模型猜测和 Web 内容不会写入                                                                                             |
| UI-001    | `capability_degraded_event_is_visible_and_recoverable`                           | 前后端契约     | 实时与刷新后的展示一致（已新增生产面板 contract，GREEN）                                                                |
| UI-002    | `tool_diagnostics_are_redacted`                                                  | 前端/事件单元  | 不出现密钥、正文或原始参数                                                                                              |

## 追踪规则

- 测试名可变，问题 ID 不变；测试注释或用例描述中引用 ID。
- 一个问题可以由多个层级测试覆盖，但不得只有宽泛 E2E 而没有可定位的契约测试。
- 问题修复后在附录 A 中保留事实并标记为 Resolved（附提交或 PR），不要删除历史 ID。
- 若问题被证明不存在，标记 Stale 并记录证据，不写“占位修复”。
- 自由文本语义校验不进入本表的默认必过项；实验验收见附录 C。
