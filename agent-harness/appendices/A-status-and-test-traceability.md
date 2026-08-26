# 附录 A：状态、代码与测试追踪

> **文档状态**：现行
> **文档类型**：证据账本
> **事实基线**：2026-08-27，审计提交 `6c5dbd40`

本表把代码存在、命名测试、生产复现和目标处置分开。测试名称表示当前已有证据，不表示目标重构已经完成。

## 1. 当前能力与债务

| 能力/问题                | 当前状态 | 目标处置           | 代码或命名测试证据                                                                                                                                              | 生产证据                       | 阶段   |
| ------------------------ | -------- | ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ | ------ |
| Run 幂等、终态事务与恢复 | 已验证   | 保留               | `agent_run_repository_tests`、`run_engine_tests`                                                                                                                | 未发现相反复现                 | 基础   |
| 工具结果进入下一模型轮次 | 已验证   | 保留               | `tool_loop_returns_tool_results_to_the_next_model_turn_before_finalizing`                                                                                       | 多轮工具过程可见               | HR-3   |
| 通用 8/24 ToolLoop       | 部分实现 | 重构为唯一循环     | `RunBudgetPolicy::Standard`、`agent_tool_loop_tests`                                                                                                            | Web 仍有第二预算语义           | HR-3   |
| 重复成功与失败重试       | 已验证   | 保留并泛化         | tool fingerprint tests                                                                                                                                          | 未覆盖不同查询同内容           | HR-1/3 |
| Web 专用无进展停止       | 部分实现 | 泛化后删除         | `two_research_rounds_without_new_evidence_stop_before_a_third_turn`                                                                                             | 当前行为直接错误终态           | HR-1/3 |
| 渐进联网                 | 已知缺陷 | 重构               | `web_preferred_and_reason_use_stable_wire_values` 只证明 wire；`temporal_verification_matrix_requires_current_run_web_evidence_in_all_24_variants` 固化严格路线 | 普通推荐被过度门禁             | HR-1/2 |
| 本地多轮检索             | 部分实现 | 接入统一循环       | local tools 和 `retrieval_scope_without_full_material_forces_a_local_tool_loop`                                                                                 | 未证明差结果自适应             | HR-1/3 |
| 当前 Run 来源所有权      | 已验证   | 保留               | `web_provenance_ordinals_restart_at_w1_for_each_run_with_high_ledger_ids`                                                                                       | 曾因 ID 偶合误拒绝             | HR-1/4 |
| 普通回答最终化           | 已知缺陷 | 简化               | strict provenance/run engine tests                                                                                                                              | 正文显示后出现来源协议失败     | HR-1/4 |
| 普通缺参                 | 已知缺陷 | 自然对话           | `production_missing_city_waits_for_input_and_resumes_the_same_run` 只证明事务恢复                                                                               | 顶部/轮内补充交互不协调        | HR-1/4 |
| Run-local UI 投影        | 部分实现 | 收口单一 owner     | run projection Vitest、恢复测试                                                                                                                                 | 地点提交后正文曾不可见         | HR-1/4 |
| 单操作写入确认           | 已验证   | 扩展变更集         | `frozen_confirmation_is_bound_to_its_run_hash_and_single_consumption`                                                                                           | 尚无多操作能力                 | HR-5   |
| 确认后验证               | 未实现   | 新增有界只读验证   | `post_confirmation_max_model_turns == 0`                                                                                                                        | 无                             | HR-5   |
| 11 个领域 operation      | 已实现   | 退出核心、兼容读取 | fresh_domains、tool dispatch、migration 072                                                                                                                     | 无真实 Provider 质量证据       | HR-6   |
| Provider 中立            | 部分实现 | Gateway 收口       | ToolLoop trait、failover tests                                                                                                                                  | 自定义 endpoint 仍是 chat-only | HR-3/7 |
| 回答语义质量             | 已知缺口 | 建立通用评测       | capacity eval 主要证明闭环                                                                                                                                      | 影片回答混淆上映状态           | HR-1/7 |

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
