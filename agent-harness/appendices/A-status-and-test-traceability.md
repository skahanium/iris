# 附录 A：状态、代码与测试追踪

> **文档状态**：现行
> **文档类型**：证据账本
> **事实基线**：2026-08-27，审计提交 `6c5dbd40`

本表把代码存在、命名测试、生产复现和目标处置分开。测试名称表示当前已有证据，不表示目标重构已经完成。

## 1. 当前能力与债务

“代码证据”只证明实现入口存在；“命名测试”只证明测试名所覆盖的合同。生产事故列引用下表 ID，任何事故都可以把测试结论降级。

| 能力/问题                | 当前状态 | 目标处置           | 代码证据                                                                                                                                                                                                                      | 命名测试                                                                                                                                                        | 生产事故       | 阶段   |
| ------------------------ | -------- | ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- | ------ |
| Run 幂等、终态事务与恢复 | 已验证   | 保留               | [`agent_run_repository.rs`](../../src-tauri/src/ai_runtime/agent_run_repository.rs)、[`run_engine/mod.rs`](../../src-tauri/src/ai_runtime/run_engine/mod.rs)                                                                  | `accept_is_idempotent_for_client_request_id_without_duplicate_message_or_event`、`terminal_sink_failure_recovers_without_reexecution`                           | 无已知相反复现 | 基础   |
| 工具结果进入下一模型轮次 | 已验证   | 保留               | [`agent_tool_loop.rs`](../../src-tauri/src/ai_runtime/agent_tool_loop.rs)                                                                                                                                                     | `tool_loop_returns_tool_results_to_the_next_model_turn_before_finalizing`                                                                                       | 无已知相反复现 | HR-3   |
| 通用 8/24 ToolLoop       | 部分实现 | 重构为唯一循环     | [`run_contract.rs`](../../src-tauri/src/ai_runtime/run_contract.rs)、[`agent_tool_loop.rs`](../../src-tauri/src/ai_runtime/agent_tool_loop.rs)                                                                                | `parent_turn_reuses_one_frozen_budget_for_every_provider_call`、`child_policy_executes_six_tools_and_rejects_the_seventh`                                       | Web 平行预算   | HR-3   |
| 重复成功与失败重试       | 部分实现 | 保留并泛化         | [`agent_tool_loop.rs`](../../src-tauri/src/ai_runtime/agent_tool_loop.rs) 的 fingerprint 与 `MAX_REPEAT_CALLS`                                                                                                                | `successful_equivalent_tool_call_is_not_executed_twice`；尚缺相同失败调用上限的独立回归                                                                         | 无             | HR-1/3 |
| Web 专用无进展停止       | 部分实现 | 泛化后删除         | [`agent_tool_loop.rs`](../../src-tauri/src/ai_runtime/agent_tool_loop.rs)、[`fresh_research_plan.rs`](../../src-tauri/src/ai_runtime/fresh_research_plan.rs)                                                                  | `two_research_rounds_without_new_evidence_stop_before_a_third_turn`                                                                                             | 直接错误终态   | HR-1/3 |
| 渐进联网                 | 已知缺陷 | 重构               | [`run_intake.rs`](../../src-tauri/src/ai_runtime/run_intake.rs)                                                                                                                                                               | `web_preferred_and_reason_use_stable_wire_values` 只证明 wire；`temporal_verification_matrix_requires_current_run_web_evidence_in_all_24_variants` 固化严格路线 | INC-HR-004/005 | HR-1/2 |
| 本地多轮检索             | 部分实现 | 接入统一循环       | [`run_context.rs`](../../src-tauri/src/ai_runtime/run_context.rs)、[`agent_tool_loop.rs`](../../src-tauri/src/ai_runtime/agent_tool_loop.rs)                                                                                  | `retrieval_scope_without_full_material_forces_a_local_tool_loop`；尚未证明差结果自适应                                                                          | 无             | HR-1/3 |
| 当前 Run 来源所有权      | 已验证   | 保留               | [`agent_evidence_repository.rs`](../../src-tauri/src/ai_runtime/agent_evidence_repository.rs)、[`provenance.rs`](../../src-tauri/src/ai_runtime/provenance.rs)                                                                | `web_provenance_ordinals_restart_at_w1_for_each_run_with_high_ledger_ids`                                                                                       | INC-HR-003     | HR-1/4 |
| 普通回答最终化           | 已知缺陷 | 简化               | [`final_answer_submission.rs`](../../src-tauri/src/ai_runtime/final_answer_submission.rs)、[`normal_run_service.rs`](../../src-tauri/src/ai_runtime/normal_run_service.rs)                                                    | `broad_movie_research_completes_with_run_local_web_sources_and_no_city_input` 被生产反例证明只覆盖理想脚本                                                      | INC-HR-004/005 | HR-1/4 |
| 普通缺参                 | 已知缺陷 | 自然对话           | [`normal_run_service.rs`](../../src-tauri/src/ai_runtime/normal_run_service.rs)                                                                                                                                               | `production_missing_city_waits_for_input_and_resumes_the_same_run` 只证明事务恢复                                                                               | INC-HR-001     | HR-1/4 |
| Run-local UI 投影        | 部分实现 | 收口单一 owner     | [`useAssistantRun.ts`](../../src/hooks/useAssistantRun.ts)、[`assistant-transcript.ts`](../../src/lib/assistant-transcript.ts)                                                                                                | `rebuilds the missing assistant slot for a recovered user-only Run`、`ignores a late Run event when no transcript slot is bound to it`                          | INC-HR-002     | HR-1/4 |
| 单操作写入确认           | 已验证   | 扩展变更集         | [`frozen_change_plan.rs`](../../src-tauri/src/ai_runtime/frozen_change_plan.rs)                                                                                                                                               | `frozen_confirmation_is_bound_to_its_run_hash_and_single_consumption`                                                                                           | 无             | HR-5   |
| 确认后验证               | 未实现   | 新增有界只读验证   | [`run_contract.rs`](../../src-tauri/src/ai_runtime/run_contract.rs) 当前 `post_confirmation_max_model_turns = 0`                                                                                                              | `production_resume_command_completes_without_model_and_repeated_resume_does_not_dispatch`                                                                       | 无             | HR-5   |
| 11 个领域 operation      | 已实现   | 退出核心、兼容读取 | [`run_contract.rs`](../../src-tauri/src/ai_runtime/run_contract.rs)、[`fresh_domains/`](../../src-tauri/src/ai_runtime/fresh_domains)、[`migration 072`](../../src-tauri/migrations/072_agent_domain_capability_mappings.sql) | `fresh_domain_tools_are_unique_closed_schema_dispatchable`、11 个 `*_minimal_record_passes_and_preserves_origin` 测试                                           | 无真实试点     | HR-6   |
| Provider 中立            | 部分实现 | Gateway 收口       | [`model_gateway.rs`](../../src-tauri/src/ai_runtime/model_gateway.rs)、[`agent_tool_loop.rs`](../../src-tauri/src/ai_runtime/agent_tool_loop.rs)                                                                              | `failover_selects_next_model_pool_candidate_for_provider_level_failure`；chat-only 降级尚未形成完整矩阵                                                         | 无真实试点     | HR-3/7 |
| 回答语义质量             | 已知缺口 | 建立通用评测       | [`agent_capacity_eval.rs`](../../src-tauri/src/ai_runtime/agent_capacity_eval.rs) 主要记录闭环和安全观察                                                                                                                      | 无可证明通用语义质量的命名测试                                                                                                                                  | INC-HR-005     | HR-1/7 |

## 2. 生产事故登记

| ID         | 日期       | 现象                                     | 已确认根因/盲区                             | 状态                         |
| ---------- | ---------- | ---------------------------------------- | ------------------------------------------- | ---------------------------- |
| INC-HR-001 | 2026-08-24 | 提交城市后长期停在等待补充信息           | `InputProvided` 状态语义错误                | 已修状态机，产品语义待 HR-4  |
| INC-HR-002 | 2026-08-24 | 后端完成但会话无 assistant 回答          | 历史只有 user 行时缺少同 Run assistant 投影 | 已补投影回归，整体待 HR-4    |
| INC-HR-003 | 2026-08-25 | 有工具和正文却提示证据无法关联           | `W1`、`[C1]`、ledger ID 多解释器及空库偶合  | ID 合同已修，普通终局待 HR-4 |
| INC-HR-004 | 2026-08-26 | 回答显示后仍标记来源协议失败             | 普通回答承担严格结构化终局                  | 未关闭，HR-1/4               |
| INC-HR-005 | 2026-08-26 | 近期影片回答混合年度片单、预告和上映状态 | fixture 只验调用和引用，不验任务完成        | 未关闭，HR-1/7               |

## 3. HR 阶段最小证据

| 阶段 | 必需失败回归                                | 必需通过证据                           |
| ---- | ------------------------------------------- | -------------------------------------- |
| HR-0 | 新文件缺失、旧文件存在、状态头/ROADMAP 冲突 | docs facts、format、diff check         |
| HR-1 | 五个生产事故与通用自适应负例                | 独立 fixture 能真实触发失败            |
| HR-2 | 普通事实被错误 WebRequired                  | task/intent/freshness 表驱动测试       |
| HR-3 | Web 专用与本地循环行为不一致                | 同一 ToolLoop 的分类预算和进展测试     |
| HR-4 | 正文被终局覆盖、普通澄清暂停                | Rust + Vitest 生命周期回归             |
| HR-5 | 多操作无法一次确认、写后不可验证            | durable apply/recovery/permission 测试 |
| HR-6 | 新 Run 仍依赖 DomainOperation               | 静态扫描、兼容读取和删除证据           |
| HR-7 | 理想脚本通过但任务答案错误                  | 通用质量矩阵和多协议 mock              |

## 4. 更新规则

- 当前命名测试失败时立即降级对应状态。
- 生产复现与测试结论冲突时记录事故并将能力标为部分实现或已知缺陷。
- 旧测试若只固化被撤回方向，必须随替代阶段删除，不能改名后继续保留。
- 真实 Provider 状态必须带日期、模型、配置 hash 和授权；fixture 永远不能升级为生产质量证据。
- 每个 HR 阶段完成时只更新本阶段相关行，不重写历史事故。
