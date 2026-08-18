# 附录 A：现状核对清单

本表是施工前的事实清单，不是长期设计承诺。状态含义：

- **Confirmed**：当前代码可直接确认缺口存在。
- **Partial**：机制已存在，但覆盖或生产接入不完整。
- **Resolved**：基线测试复现后已完成最小修复，并有回归测试通过。
- **Stale**：旧审查结论已被当前代码事实推翻。
- **Unverified**：尚无足够代码或测试证据，不进入主干施工。

| ID        | 优先级 | 状态       | 当前事实                                                                                                                      | 最小行动                                               |
| --------- | ------ | ---------- | ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| RUN-001   | P0     | Resolved   | `retry_with_sink_outcome` 现返回 `is_new`，仅首次接受时 emit accepted 并允许 spawn；重放/并发只产生一个执行权                 | 保留 `retry_replay_*` 与 `concurrent_retry_*` 回归测试 |
| RUN-002   | P0     | Resolved   | 基线探针曾复现 `AnswerComplete` 早于持久化；现已改为 Completed 后再发终端展示                                                 | 保留顺序回归测试并覆盖 sink 失败                       |
| RUN-003   | P1     | Partial    | Run 状态与事件已有基础，但 sink 失败后的恢复语义缺少统一契约                                                                  | 用快照/重放恢复，禁止重做副作用                        |
| ROUTE-001 | P1     | Resolved   | `ToolSurfacePlan.tool_names` 已由生产编排器填充，`NormalRunToolExecutor` 只消费该冻结列表，`capabilities_read` 同步报告该列表 | 保留 current-surface-only 与 executor surface 测试     |
| ROUTE-002 | P2     | Unverified | 没有证据表明新增 LLM Router 能改善当前可靠性                                                                                  | 不进入首阶段；仅保留评测后立项可能                     |
| TOOL-001  | P1     | Resolved   | `capabilities_read` 现通过 `ToolDispatchContext.available_tool_names` 只报告当前 Run 已冻结 surface                           | 保留 current-surface-only 测试                         |
| TOOL-002  | P0     | Resolved   | 两个 harness 工具曾错误映射到 `vault.search`；现已使用独立 `harness.*` 权限原子                                               | 保留权限映射回归测试                                   |
| TOOL-003  | P1     | Resolved   | 模型工具调用同时受 `AgentToolLoop.allowed_tools` 与 `NormalRunToolExecutor.allowed_tool_names` 表面门禁约束                   | 保留 unexposed tool 负例与 executor surface 负例       |
| TOOL-004  | P2     | Resolved   | 遗留 `conclude_reasoning` 保持内部不可见且不进入模型 surface；未消费参数不暴露给模型                                          | 保留 internal-only 不暴露测试与静态元数据负例          |
| EVID-001  | P0     | Resolved   | `SourceGroupFallback` 已存在且 ToolLoop 会生成；Direct 严格 Web 现已补齐 binding                                              | 保留 Direct/ToolLoop 双路径回归测试                    |
| EVID-002  | P0     | Resolved   | UI 对缺失/未知 binding 与有来源条目时统一按“本次检索来源/未逐段核验”展示；精确 binding 仍走精确样式                           | 保留 missing/unknown binding fail-safe 测试            |
| EVID-003  | P1     | Confirmed  | 严格结构化 VERIFIED 规则没有形成有效覆盖                                                                                      | 逐工具增加确定性规则；其余保持 uncalibrated            |
| EVID-004  | P2     | Resolved   | `list_current_run_web_citation_links` 只返回当前 Run、未 retired、HTTPS 可定位证据；foreign/retired 均被排除                  | 保留 foreign/retired 负例测试                          |
| SEC-001   | P0     | Resolved   | harness 工具已使用独立 `harness.*` 权限原子，不再继承 `vault.search`                                                          | 保留权限映射拒绝型测试                                 |
| SEC-002   | P0     | Resolved   | 本地检索内容通过 `record_web_query_taint_witness` 与 Web 查询门禁阻止进入查询/URL/日志；已有端到端隐私负例                    | 保留 taint/隐私负例回归测试                            |
| CTX-001   | P1     | Partial    | 运行时上下文构造逻辑存在，但生产调用链接入不足                                                                                | 接成只读 `RunSituation`，不新增状态表                  |
| CTX-002   | P1     | Confirmed  | 会话记忆兜底可能把第一条用户消息长期提升为目标                                                                                | 移除兜底，目标只来自当前请求/明确任务                  |
| CTX-003   | P1     | Partial    | `conversation_summaries` 已存在，可支持压缩，但需补覆盖范围和失效语义                                                         | 复用现表并增加失效/重建测试                            |
| MEM-001   | P2     | Partial    | `ai_memories` 已存在，但 key 冲突可能跨 scope 覆盖                                                                            | 调整为 `(scope, key)` 并提供 scope 清理                |
| MEM-002   | P2     | Confirmed  | 缺少“仅用户确认偏好可长期写入”的主干约束                                                                                      | 限制写入入口、来源和预算                               |
| UI-001    | P1     | Resolved   | `capability_degraded` 已接入 `UnifiedAssistantPanel` 生产事件投影，组件复用且不新增第二套体系                                 | 保留事件 reducer 与生产面板 contract 测试              |
| UI-002    | P2     | Confirmed  | 原始/无用工具参数会增加噪音与隐私风险                                                                                         | 仅显示脱敏摘要和稳定错误码                             |
| EVAL-001  | P1     | Stale      | “只有 24 个评测场景”的旧基线已过期；当前代码已有 48-case 契约                                                                 | 复用现有套件并维护稳定场景 ID                          |
| MEM-003   | —      | Stale      | “完全没有记忆基础设施”不准确：会话摘要和 `ai_memories` 均已存在                                                               | 只补最小安全语义，不重建记忆中心                       |

## 核对纪律

- 实施某项前再次搜索其定义、调用方和测试；若事实变化，先更新本表状态。
- `Stale` 项不转化为任务。
- `Unverified` 项必须先获得复现、调用链或评测证据，不能凭架构偏好升级优先级。
- 优先级描述风险，不代表版本承诺。

## 阶段 0 基线结果（branch-v1.3.0）

### 已钉住的测试

- `tool_surface` 聚焦测试：7 passed；时效请求、Web 授权、预取后工具隐藏等现有行为成立。
- `harness_tools_do_not_inherit_vault_search_permission`：先 RED 后 GREEN；当前使用独立 `harness.child_run` / `harness.conclude` 原子。
- `retry_replay_emits_accepted_event_only_once` / `concurrent_retry_starts_executor_once`：先 RED 后 GREEN；重放与并发 retry 均只产生一个 `is_new=true` 和一个 accepted 通知（RUN-001）。
- `direct_required_web_run_persists_source_group_binding_when_markers_are_missing`：先 RED 后 GREEN；Direct 严格 Web 现在保存 `source_group_fallback` binding（EVID-001）。
- `direct_streaming_does_not_emit_answer_complete_before_durable_finalization`：先 RED 后 GREEN；`AnswerComplete` 只在持久化完成后观察到（RUN-002）。
- `capabilities_read_reports_current_surface_only`：仍 RED；`capabilities_read` 当前仍返回完整目录，Web 关闭时仍列出 `web_search`（TOOL-001）。
- `missing_binding_renders_unverified_source_group` / `unknown_binding_version_fails_safe`：先 RED 后 GREEN；UI 对缺失/未知 binding 统一按“本次检索来源/未逐段核验”展示（EVID-002）。
- `first_user_message_is_not_permanent_goal`：仍 RED；会话记忆兜底仍可能把第一条用户消息写入长期目标（CTX-002）。
- `structured_verifier_requires_registered_rule`：GREEN；当前 `VERIFIED` 注册表为空，任意 provider/model 都不能晋升结构化 VERIFIED（EVID-003）。
- `capability_degraded_event_is_visible_and_recoverable`：GREEN；`AssistantRunCapabilityDegraded` 已接入生产面板事件投影（UI-001）。

这些 RED 是阶段 0 的基线证据，进入后续阶段后必须逐项转绿；不能用删除断言或标记 `ignore` 代替修复。

### 现有覆盖映射（阶段 0 复核）

- RUN-003：`presentation_delivery_failure_never_invalidates_the_durable_answer`、`completed_emit_failure_never_appends_a_second_terminal_event`、`startup_recovery_*`、`startup_recovery_completes_a_rejected_confirmation_as_not_modified`。
- ROUTE-001：`tool_surface` 聚焦测试、`web_evidence_broker::frozen_*`、`tool_executor` 的 Run capability 测试。
- TOOL-003：`tool_execution_pipeline::malformed_arguments_never_reach_dispatch`、`web_disabled_time_sensitive_does_not_expose_search_and_forbids_fabrication`、`tool_dispatch` 的 scope/权限负例。
- SEC-001：`harness_tools_do_not_inherit_vault_search_permission`、`permission_atom_strings_are_stable`。
- SEC-002：`tool_audit::tainted_web_query_is_witnessed_before_external_dispatch`、`tool_audit::web_query_and_body_are_not_persisted`、`agent_capacity_eval_tests::web_query_boundary_keeps_a_blocked_attempt_after_a_clean_retry`。
- CTX-001：`run_context_tests::assemble_reads_only_the_run_persisted_explicit_reference`、`normal_session_repository_tests::prompt_history_and_memory_projection_exclude_failed_modern_turns`。
- CTX-003：`conversation_memory::refresh_keeps_summary_and_recent_window_disjoint_at_twenty_five_messages`、`normal_session_repository_tests::retract_clears_conversation_memory_when_remaining_history_fits_the_recent_window`。
- UI-001：`tests/assistant-run-capability-degraded.test.tsx`、`tests/assistant-run-events.test.ts`。

### 48-case Agent capacity eval 基线

- 核心 48-case 确定性 full 结果固定为 `docs/eval/results/v1.2.15-agent-capacity.json`：48/48，`securityGate=true`，四个证据组各 12/12。
- 场景 ID 固定为 `1..=48`，由 `src-tauri/src/ai_runtime/agent_capacity_eval.rs` 的固定矩阵生成（24 个基础问题 Offline/Online 成对），不另造重复评测框架。
- 另有 24-case `agent:eval:smoke` 交互矩阵；`docs/eval/results/v1.2.18-agent-rag-stage0-baseline.json` 记录为 12/24 完成、case 25/36/37/48 失败，属于发布阻断，不是 48-case 替代。
- 后续阶段不得通过新建“另一个评测框架”绕过上述固定场景 ID；新增能力只在现有层级无法覆盖时才扩展场景集，并同步更新本附录与 B 表。

## 阶段 1 结果（branch-v1.3.0）

- 普通会话单航班：前端 `useUnifiedAssistantSend` 的 `accepts at most one Run when send is invoked twice in the same tick` 保持 GREEN；后端 `concurrent_retry_starts_executor_once` 同时钉住唯一执行权。
- RUN-001：`retry_replay_emits_accepted_event_only_once` 与 `concurrent_retry_starts_executor_once` 均 GREEN；`assistant_run_retry` 只在 `is_new=true` 时 spawn。
- RUN-002：`direct_streaming_does_not_emit_answer_complete_before_durable_finalization` 保持 GREEN；terminal presentation 投递失败保持 best-effort。
- EVID-002：`missing_binding_renders_unverified_source_group` / `unknown_binding_version_fails_safe` 均 GREEN；`AssistantCitationFooter` 对缺失/未知 binding 且有来源条目时统一 fail-safe。
- UI-001：`AssistantRunCapabilityDegraded` 已接入 `UnifiedAssistantPanel.impl.tsx` 的事件投影，生产面板 contract 测试 GREEN。

## 阶段 2 结果（branch-v1.3.0）

- ROUTE-001：`ToolSurfacePlan.tool_names` 已由生产编排器填充，`NormalRunToolExecutor` 与 `capabilities_read` 均消费该冻结列表。
- TOOL-001：`capabilities_read_reports_current_surface_only` 已 GREEN；`capabilities_read` 通过 `ToolDispatchContext.available_tool_names` 只报告当前 Run 冻结 surface。
- TOOL-003：`unexposed_tool_call_is_rejected_without_reaching_executor` 与 `executor_rejects_model_call_outside_frozen_surface` 均 GREEN；模型工具调用在 AgentToolLoop 与 NormalRunToolExecutor 两层都被 surface 门禁拒绝。
- TOOL-002：harness 工具独立权限映射保持 GREEN，`conclude_reasoning` 保持内部不可见。
- TOOL-004：遗留 `conclude_reasoning` 不进入模型 surface，`static_metadata_for_tool` 对无生产消费方的内部工具返回 `None`。
- 静态元数据：`ToolStaticMetadata` 已为 `web_search`、本地读取/搜索、运行时快照等关键工具补充 `cost_class` / `output_policy` / `evidence_policy`，并通过 `capabilities_read` 暴露。
- 工具输出尺寸：既有 `oversized_web_tool_results_fail_closed_with_valid_json`、`tool_payload_8001_truncated` 等测试继续覆盖确定性截断与安全失败，未引入新平行截断逻辑。

## 阶段 3 结果（branch-v1.3.0）

- EVID-004：`current_run_citation_links_exclude_foreign_and_retired_evidence` 已 GREEN；当前 Run 的 citation links 只包含本 Run、未 retired、HTTPS 可定位证据。
- 来源展示：`AssistantCitationFooter` 不把插入顺序包装成“排名/评分/质量排序”，新增 UI 负例 GREEN。
- SEC-001 / SEC-002：权限映射与本地检索→Web 查询隐私门禁保持既有负例 GREEN。
- EVID-001/EVID-002：Direct/ToolLoop 的 `SourceGroupFallback` 与 UI fail-safe 继续 GREEN。
- EVID-003：当前没有已支持的结构化 VERIFIED 工具，`VERIFIED` 注册表保持为空；`structured_verifier_requires_registered_rule` 保证无规则不能晋升。
