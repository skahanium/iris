# 附录 A：现状核对清单

本表是施工前的事实清单，不是长期设计承诺。状态含义：

- **Confirmed**：当前代码可直接确认缺口存在。
- **Partial**：机制已存在，但覆盖或生产接入不完整。
- **Resolved**：基线测试复现后已完成最小修复，并有回归测试通过。
- **Stale**：旧审查结论已被当前代码事实推翻。
- **Unverified**：尚无足够代码或测试证据，不进入主干施工。

| ID        | 优先级 | 状态       | 当前事实                                                                                                                        | 最小行动                                       |
| --------- | ------ | ---------- | ------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| RUN-001   | P0     | Confirmed  | retry 仓储层能识别已接受请求，但 `retry_with_sink`/上层执行入口未保留 `is_new`，重放会重复发 accepted 事件并存在重复 spawn 风险 | 传递 `is_new`，并发测试钉住唯一执行权          |
| RUN-002   | P0     | Resolved   | 基线探针曾复现 `AnswerComplete` 早于持久化；现已改为 Completed 后再发终端展示                                                   | 保留顺序回归测试并覆盖 sink 失败               |
| RUN-003   | P1     | Partial    | Run 状态与事件已有基础，但 sink 失败后的恢复语义缺少统一契约                                                                    | 用快照/重放恢复，禁止重做副作用                |
| ROUTE-001 | P1     | Partial    | `ToolSurfacePlan` 已开始收敛时效/Web 判断，但能力读取和执行仍未完全消费同一冻结结果                                             | 完成现有 planner 接入，Executor 只消费冻结结果 |
| ROUTE-002 | P2     | Unverified | 没有证据表明新增 LLM Router 能改善当前可靠性                                                                                    | 不进入首阶段；仅保留评测后立项可能             |
| TOOL-001  | P1     | Partial    | `ToolImplementationStatus` 已排除 Planned，但 `capabilities_read` 仍读取完整目录                                                | 改为读取 `ToolSurfacePlan` 的已解析工具列表    |
| TOOL-002  | P0     | Resolved   | 两个 harness 工具曾错误映射到 `vault.search`；现已使用独立 `harness.*` 权限原子                                                 | 保留权限映射回归测试                           |
| TOOL-003  | P1     | Partial    | 目录、权限和执行校验均存在，但尚未形成单一不可绕过门禁                                                                          | 收敛门禁并覆盖旁路负例                         |
| TOOL-004  | P2     | Confirmed  | 部分工具参数没有生产消费方                                                                                                      | 删除死参数或实现真实语义                       |
| EVID-001  | P0     | Resolved   | `SourceGroupFallback` 已存在且 ToolLoop 会生成；Direct 严格 Web 现已补齐 binding                                                | 保留 Direct/ToolLoop 双路径回归测试            |
| EVID-002  | P0     | Partial    | UI 只有识别到 fallback 才显示“未逐段核验”，binding 缺失时降级不够诚实                                                           | 缺失/未知 binding 统一 fail-safe               |
| EVID-003  | P1     | Confirmed  | 严格结构化 VERIFIED 规则没有形成有效覆盖                                                                                        | 逐工具增加确定性规则；其余保持 uncalibrated    |
| EVID-004  | P2     | Partial    | `session_evidence` 已具时间、原 Run、失效和安全摘录字段                                                                         | 在绑定校验中完整消费现有字段，不新增证据表     |
| SEC-001   | P0     | Confirmed  | 错误工具权限映射可能使授权语义失真                                                                                              | 与 TOOL-002 一并修复并加入拒绝型测试           |
| SEC-002   | P0     | Partial    | 已有 Web 权限与内容隔离机制，但本地检索到 Web 查询的数据流需端到端负例                                                          | 建立统一数据流门禁和隐私回归测试               |
| CTX-001   | P1     | Partial    | 运行时上下文构造逻辑存在，但生产调用链接入不足                                                                                  | 接成只读 `RunSituation`，不新增状态表          |
| CTX-002   | P1     | Confirmed  | 会话记忆兜底可能把第一条用户消息长期提升为目标                                                                                  | 移除兜底，目标只来自当前请求/明确任务          |
| CTX-003   | P1     | Partial    | `conversation_summaries` 已存在，可支持压缩，但需补覆盖范围和失效语义                                                           | 复用现表并增加失效/重建测试                    |
| MEM-001   | P2     | Partial    | `ai_memories` 已存在，但 key 冲突可能跨 scope 覆盖                                                                              | 调整为 `(scope, key)` 并提供 scope 清理        |
| MEM-002   | P2     | Confirmed  | 缺少“仅用户确认偏好可长期写入”的主干约束                                                                                        | 限制写入入口、来源和预算                       |
| UI-001    | P1     | Partial    | `capability_degraded` 组件与测试存在，但生产面板接入不完整                                                                      | 接入既有事件投影，不新增组件体系               |
| UI-002    | P2     | Confirmed  | 原始/无用工具参数会增加噪音与隐私风险                                                                                           | 仅显示脱敏摘要和稳定错误码                     |
| EVAL-001  | P1     | Stale      | “只有 24 个评测场景”的旧基线已过期；当前代码已有 48-case 契约                                                                   | 复用现有套件并维护稳定场景 ID                  |
| MEM-003   | —      | Stale      | “完全没有记忆基础设施”不准确：会话摘要和 `ai_memories` 均已存在                                                                 | 只补最小安全语义，不重建记忆中心               |

## 核对纪律

- 实施某项前再次搜索其定义、调用方和测试；若事实变化，先更新本表状态。
- `Stale` 项不转化为任务。
- `Unverified` 项必须先获得复现、调用链或评测证据，不能凭架构偏好升级优先级。
- 优先级描述风险，不代表版本承诺。

## 阶段 0 基线结果（branch-v1.3.0）

### 已钉住的测试

- `tool_surface` 聚焦测试：7 passed；时效请求、Web 授权、预取后工具隐藏等现有行为成立。
- `harness_tools_do_not_inherit_vault_search_permission`：先 RED 后 GREEN；当前使用独立 `harness.child_run` / `harness.conclude` 原子。
- `retry_replay_emits_accepted_event_only_once`：仍 RED，重放收到 2 个 accepted 事件，目标为 1 个（RUN-001）。
- `direct_required_web_run_persists_source_group_binding_when_markers_are_missing`：先 RED 后 GREEN；Direct 严格 Web 现在保存 `source_group_fallback` binding（EVID-001）。
- `direct_streaming_does_not_emit_answer_complete_before_durable_finalization`：先 RED 后 GREEN；`AnswerComplete` 只在持久化完成后观察到（RUN-002）。
- `capabilities_read_reports_current_surface_only`：仍 RED；`capabilities_read` 当前仍返回完整目录，Web 关闭时仍列出 `web_search`（TOOL-001）。
- `missing_binding_renders_unverified_source_group` / `unknown_binding_version_fails_safe`：仍 RED；UI 只在显式 fallback 时降级，缺失/未知 binding 仍按普通“来源”展示（EVID-002）。
- `first_user_message_is_not_permanent_goal`：仍 RED；会话记忆兜底仍可能把第一条用户消息写入长期目标（CTX-002）。
- `structured_verifier_requires_registered_rule`：GREEN；当前 `VERIFIED` 注册表为空，任意 provider/model 都不能晋升结构化 VERIFIED（EVID-003）。

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
