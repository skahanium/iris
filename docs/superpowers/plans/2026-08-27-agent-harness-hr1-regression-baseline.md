# Agent Harness HR-1 Regression Baseline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建立可独立重现现有生产事故和通用行为缺口的 HR-1 回归基线，同时不提前改变 HR-2 至 HR-5 的生产路由。

**Architecture:** 在既有 Rust 单元/集成测试模块中添加两类证据：一类是当前已具备的通用能力的绿色回归；另一类是标有 `HR-<后续阶段>-target` 的 `#[should_panic]` 红灯夹具，精确记录当前代码与目标合同的差异。红灯夹具的被测入口必须是真实 Intake、ToolLoop 或 Normal Run，不创建第二套 Agent 状态机、测试专用生产分支或 Provider。

**Tech Stack:** Rust、tokio、现有 headless LLM/MCP protocol double、SQLite in-memory/temporary database、Vitest、现有 `AgentToolLoop`/`RunIntake`/`NormalRunService` 测试支持。

## Global Constraints

- 不创建 worktree；不新增依赖、数据库表/迁移、IPC、Provider、模型名称分支或用户数据格式。
- HR-1 不改变生产 Intake、ToolLoop、finalization、自然澄清或写入路由；它只建立可复现基线、补足现有正确行为的回归并同步事实文档。
- 所有新增行为先用现有真实入口写测试；红灯夹具只使用 `#[should_panic(expected = "HR-<阶段>-target")]`，并在测试名、注释和文档中注明由哪个后续 HR 阶段反转。
- 不预编排唯一正确搜索词、唯一工具顺序或完整答案正文；断言 query 是否发生有意义变化、资源 identity 是否新增、最终答案是否出现、状态/来源归属是否正确。
- 只运行本计划涉及的 Rust/Vitest 精确测试；提交前运行受影响语言的 format、clippy、lint、typecheck、docs check 与 diff check。

---

### Task 1: 固化 Intake 的渐进联网红灯与严格边界绿灯

**Files:**

- Modify: `src-tauri/src/ai_runtime/run_intake_tests.rs`
- Modify: `agent-harness/appendices/A-status-and-test-traceability.md`

**Interfaces:**

- Consumes: `RunIntake::resolve_envelope(&AssistantRunStartRequest) -> AppResult<ExecutionEnvelope>`。
- Produces: 三个普通外部问题的 HR-2 红灯夹具，以及明确联网/URL/高风险当前事实仍为 `WebRequired + CurrentRunWeb` 的绿色边界测试。

- [x] **Step 1: 写入普通外部问题目标路由的失败夹具**

  在 `run_intake_tests.rs` 使用已有 `request()` helper 添加：

  ```rust
  #[test]
  #[should_panic(expected = "HR-2-target")]
  fn hr1_ordinary_external_questions_still_record_the_webpreferred_gap() {
      for message in [
          "推荐三本理解组织治理的入门书，并说明适合什么读者。",
          "比较两种常见的知识管理方法，各自适合什么场景？",
          "帮我梳理公开可得的笔记写作建议。",
      ] {
          let mut request = request();
          request.web_enabled = true;
          request.turn.message = message.into();
          let envelope = RunIntake::resolve_envelope(&request).expect("resolve envelope");
          assert_eq!(envelope.freshness, Freshness::WebPreferred, "HR-2-target: {message}");
          assert_eq!(envelope.verification_requirement, VerificationRequirement::None, "HR-2-target: {message}");
      }
  }
  ```

- [x] **Step 2: 运行红灯夹具，确认它因当前 `StrictExternalFact` 路径 panic**

  Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib hr1_ordinary_external_questions_still_record_the_webpreferred_gap -- --nocapture`

  Expected: PASS（`#[should_panic]` 捕获断言；输出包含 `HR-2-target`），证明夹具对当前缺口敏感。

- [x] **Step 3: 写入严格边界绿色回归**

  添加表驱动测试，至少覆盖“请联网核实该 URL”“今天的金融价格”“当前法律规则是否生效”，断言均为 `WebRequired + CurrentRunWeb`；同时断言 `web_enabled = false` 时不暴露 `web.search`。

- [x] **Step 4: 运行 Intake 精确测试**

  Run: `cargo test --manifest-path src-tauri/Cargo.toml run_intake_tests -- --nocapture`

  Expected: PASS；普通问题红灯和严格边界绿灯同时存在。

- [x] **Step 5: 更新证据账本**

  在附录 A 的“渐进联网”行写入上述红灯夹具名称、严格边界绿色测试名称与 `HR-2` 反转责任；状态保持“已知缺陷”。

### Task 2: 建立非预编排的通用 ToolLoop 行为基线

**Files:**

- Modify: `src-tauri/src/ai_runtime/agent_tool_loop_tests.rs`
- Modify: `agent-harness/appendices/A-status-and-test-traceability.md`

**Interfaces:**

- Consumes: `AgentToolLoop::execute`、`ToolLoopProvider`、`ToolLoopExecutor`、`ToolCallResult`。
- Produces: 自适应搜索、无进展强制综合、本地多跳、相同失败调用上限四类独立回归。

- [x] **Step 1: 写入自适应搜索绿色夹具**

  在测试文件添加只在测试中使用的 executor，记录每个 `web_search` 的规范化 `query` 并依 query 返回不同 `resource_id`。脚本 Provider 的第一轮调用宽泛 query、第二轮调用另一 query、第三轮提交自然正文。断言：

  ```rust
  assert_ne!(queries[0], queries[1]);
  assert_ne!(resource_ids[0], resource_ids[1]);
  assert_eq!(outcome.content, "已基于第二轮的新资料完成回答。");
  ```

  不断言第二 query 的具体文本；只断言它不同且带来新资源。

- [x] **Step 2: 运行自适应搜索夹具并确认通过**

  Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib hr1_adaptive_search_accepts_a_refined_query_with_a_new_resource -- --nocapture`

  Expected: PASS，且 Provider 第二轮收到第一轮的工具结果。

- [x] **Step 3: 写入两轮无进展后的强制综合红灯夹具**

  用同一 canonical resource/content hash 的两次不同 query 构造 Provider，再安排第三轮自然答案。测试应期待 `AgentToolLoopOutcome { content: "基于已知资料的保守结论" }`，并标记：

  ```rust
  #[should_panic(expected = "HR-3-target")]
  ```

  断言消息携带 `HR-3-target`。当前 `fresh_research_no_new_evidence` 错误会触发该红灯；HR-3 统一进展逻辑后移除属性并保留断言。

- [x] **Step 4: 写入本地多跳绿色夹具**

  Scripted Provider 依次调用 `search_keyword`、两个不同 `read_note`，随后自然回答。测试 executor 对两个 note 返回不同 resource identity；断言两个 read 都执行、最终正文出现、没有 Web capability 或 `web_search` 调用。

- [x] **Step 5: 写入相同失败调用上限绿色夹具**

  Scripted Provider 连续三次发出相同有效 `web_search` 调用；executor 前两次返回同一可重试失败。断言 executor 只真正执行两次，第三次 tool result 的稳定错误为 `tool_call_repeated`，并且最后一轮自然回答可完成。

- [x] **Step 6: 运行 ToolLoop 精确测试**

  Run: `cargo test --manifest-path src-tauri/Cargo.toml agent_tool_loop_tests -- --nocapture`

  Expected: PASS；其中无进展目标由 `#[should_panic]` 记录为 HR-3 红灯。

- [x] **Step 7: 更新证据账本**

  为“重复成功与失败重试”“Web 专用无进展停止”“本地多轮检索”写入具体 HR-1 测试名，明确无进展红灯由 HR-3 反转。

### Task 3: 建立普通最终化、自然澄清、投影与来源兼容基线

**Files:**

- Modify: `src-tauri/src/ai_runtime/normal_run_service_tests.rs`
- Modify: `src-tauri/src/ai_runtime/agent_evidence_repository_tests.rs`
- Modify: `tests/use-assistant-run-transcript.test.tsx`
- Modify: `tests/assistant-run-events.test.ts`
- Modify: `agent-harness/appendices/A-status-and-test-traceability.md`

**Interfaces:**

- Consumes: `execute_normal_run`、`RunIntake::control_with_sink`、headless protocol double、`replayAssistantRunEvents`、`reduceAssistantRunEvent`。
- Produces: 普通回答被协议误拒绝、普通缺参暂停同一 Run 的 HR-4 红灯夹具；同 Run 投影、来源隔离与旧输入恢复的绿色回归。

- [x] **Step 1: 写入普通正文不应被非严格协议覆盖的红灯夹具**

  使用现有 headless LLM double 让普通宽泛问题在有 Web 工具结果后返回自然正文、但不调用 `submit_final_answer`。测试目标断言 Run 为 `Completed` 且保留该正文，并标记 `#[should_panic(expected = "HR-4-target")]`。当前严格路由要求终局工具，夹具必须因实际终态/错误而触发。

- [x] **Step 2: 写入普通缺参应自然完成的红灯夹具**

  对需要地点的普通问题提供可用的 assistant 澄清自然正文，目标断言首个 Run 直接 `Completed` 且不存在 `pending_input`；标记 `#[should_panic(expected = "HR-4-target")]`。该夹具使用真实 `execute_normal_run`，不得只测试 reducer。

- [x] **Step 3: 加强绿色来源与投影回归**

  在现有高 ledger ID fixture 基础上添加同会话两个 Run 均使用 `W1` 的测试，断言第二 Run 不能绑定第一 Run 的 evidence。前端测试补充“只有 user 历史行 + `input_provided → content_delta → completed`”最终只投影一条同 Run assistant 消息，以及无同 Run 用户行的迟到事件仍被忽略。

- [x] **Step 4: 运行 Normal Run 和 Vitest 精确测试**

  Run:

  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml normal_run_service_tests -- --nocapture
  npm run test -- tests/use-assistant-run-transcript.test.tsx tests/assistant-run-events.test.ts
  ```

  Expected: PASS；两个 `HR-4-target` `#[should_panic]` 夹具记录当前缺口，来源/投影回归为绿色。

- [x] **Step 5: 更新事故与账本**

  将 INC-HR-001 至 INC-HR-004 逐一链接到新增红灯/绿色 fixture；不得把现有生产缺陷标为“已修复”。

### Task 4: 复核确认型写入、质量夹具和阶段事实

**Files:**

- Modify: `src-tauri/src/ai_runtime/agent_capacity_eval_tests.rs`
- Modify: `agent-harness/05-implementation-roadmap.md`
- Modify: `agent-harness/06-evaluation-performance-and-acceptance.md`
- Modify: `agent-harness/appendices/A-status-and-test-traceability.md`

**Interfaces:**

- Consumes: 现有 capacity eval observation/scorer、`FrozenChangePlan` 与确认测试。
- Produces: 不依赖模型名称或领域关键词的质量夹具，以及 HR-1 已验收所需事实记录。

- [x] **Step 1: 写入质量夹具的失败与通过样本**

  在 `agent_capacity_eval_tests.rs` 为同一通用“当前信息推荐”任务提供两个 scripted observation：一个把“已上映、即将上映、传闻”混为一谈，另一个明确 status、scope 和证据归属。评分断言不得依赖电影名、固定 query 或完整正文；它必须拒绝前者、接受后者，并将 verdict 记录为 deterministic fixture quality，而非真实 Provider 质量。

- [x] **Step 2: 运行质量夹具与确认型写入精确测试**

  Run:

  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml --lib hr1_current_recommendation_quality_fixture_requires_status_scope_and_bound_sources -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml --lib frozen_confirmation_is_bound_to_its_run_hash_and_single_consumption -- --nocapture
  ```

  Expected: PASS；写入只验证当前单操作、不越权的 HR-1 安全基线，不实现 HR-5 多操作或写后验证。

- [x] **Step 3: 将 HR-1 状态同步到现行文档**

  仅在所有夹具可运行且每项都有明确红/绿归属后，把 HR-1 标为“已验收”。文档必须注明：红灯夹具代表尚未实施的 HR-2/3/4 目标，不能被写作当前产品能力；质量夹具不等于真实 Provider 试点。

### Task 5: 阶段验证、审查与交付

**Files:**

- Modify: 仅 Task 1–4 实际改变的文件。

- [x] **Step 1: 运行全部 HR-1 精确回归**

  Run:

  ```bash
  cargo test --manifest-path src-tauri/Cargo.toml run_intake_tests -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml agent_tool_loop_tests -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml normal_run_service_tests -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml agent_evidence_repository_tests -- --nocapture
  cargo test --manifest-path src-tauri/Cargo.toml --lib hr1_current_recommendation_quality_fixture_requires_status_scope_and_bound_sources -- --nocapture
  npm run test -- tests/use-assistant-run-transcript.test.tsx tests/assistant-run-events.test.ts
  ```

  Expected: 所有命令 exit 0；`#[should_panic]` 只用于列明的 HR-2/3/4 红灯夹具。

- [x] **Step 2: 运行必要质量门禁**

  Run:

  ```bash
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  npm run lint
  npm run format:check
  npm run typecheck
  npm run docs:check
  git diff --check
  ```

  Expected: 全部 exit 0。

- [x] **Step 3: 逐项完成审计**

  对照 HR-1 八项，确认每一项都有真实入口、独立 fixture、红/绿状态、后续阶段归属；确认无生产路由、Provider、迁移、IPC 或模型专用分支被改动。

- [x] **Step 4: 提交和推送**

  ```bash
  git add src-tauri/src/ai_runtime/agent_tool_loop_tests.rs src-tauri/src/ai_runtime/agent_capacity_eval_tests.rs src-tauri/src/ai_runtime/agent_evidence_repository_tests.rs src-tauri/src/ai_runtime/normal_run_service_tests.rs src-tauri/src/ai_runtime/run_intake_tests.rs tests/use-assistant-run-transcript.test.tsx agent-harness/05-implementation-roadmap.md agent-harness/06-evaluation-performance-and-acceptance.md agent-harness/appendices/A-status-and-test-traceability.md docs/README.md docs/superpowers/plans/2026-08-27-agent-harness-hr1-regression-baseline.md
  git commit -m "test(ai): 建立 Harness HR-1 通用回归基线"
  git push origin branch-v1.3.0
  ```

  Expected: 当前分支与 `origin/branch-v1.3.0` 指向同一提交。

## 计划自检

- HR-1 八项均已映射：渐进联网（Task 1）、差结果和无进展/本地多跳/失败重试（Task 2）、普通最终化/澄清/投影/来源（Task 3）、确认型写入和质量夹具（Task 4）。
- 红灯夹具由后续 HR-2、HR-3、HR-4 明确接管；未在 HR-1 中修改其生产路由。
- 没有依赖真实 Provider、固定领域 entity、固定唯一 query 或完整答案正文；质量夹具只验证结构化 task completion 标记和来源归属。
- 本计划不含占位文本，且每项均指定文件、接口、命令与验证结果。
