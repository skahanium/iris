# AH-2 自适应研究循环 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` (recommended) or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不增加第二网络工具、研究引擎、证据账本或持久化真相源的前提下，使当前事实研究按 Quick/Standard/Deep 档位执行可恢复、可取消且绝不越界的搜索与深抓取循环。

**Architecture:** `fresh_research_plan` 冻结 profile 与计数/时限；`NormalRunToolExecutor` 是唯一 Run 内预留、恢复、provenance 与 deadline 执行点；`web_evidence_broker` 仅在既有 `web_search` 合同下执行 Host 授权的搜索/抓取；`AgentToolLoop` 继续是唯一模型轮次控制器。

**Tech Stack:** Rust、Tokio、既有 SQLite `agent_run_steps` 恢复状态、`web_search` / evidence ledger / MCP provider snapshot；不新增依赖、表、IPC 或模型可见工具。

## 实施记录（2026-08-23）

- AH-2 已完成：三档 profile、schema 3 无正文恢复状态、current-Run URL provenance、实际抓取消费、最多 3 并发抓取、deadline、修复额度和两轮无新增证据停机均已接入；未新增依赖、表、IPC 或网络工具。
- AH-3 已完成：按答案合同区分精确事实与研究型事实；无 binding 时统一进入通用 Web research，News 独占回退已删除。
- AH-4 已完成代码清理：移除 `fresh_domains` 模块级 dead-code 许可及不可达分支。真实 provider/model 的 p50/p95/token 试点仍须按 `agent-harness/06-live-pilot-and-archive.md` 取得明确授权，不能以本地 fixture 替代。
- 计划中的旧复选框保留为施工审计轨迹；本轮最终执行证据以附录 A 和提交前门禁记录为准。

## Global Constraints

- `.md` 是笔记唯一权威；研究状态不得保存查询正文、网页正文、凭证或模型推理。
- 只保留模型可见工具 `web_search`，输入仍为 `query`、`gap`、`urls`。
- Deep 仅由用户明确“深入研究”或 UI 明确选择触发，模型不能升级档位。
- Host 独占权限、URL provenance、SSRF/redirect/类型/大小校验、预算、evidence 注册与最终化。
- profile 上限：Quick `1/2/2/4/20s`，Standard `3/6/4/8/45s`，Deep `5/10/6/12/90s`（搜索/抓取/模型续接/evidence/deadline）；全局仍为 8 模型轮次、24 工具调用和 32K packet。
- 单轮抓取最多并发 3；两轮无新增有效 evidence ID、预算/deadline/取消/权限撤销时必须停止。
- 只能复用 `agent_run_steps` 保存 body-free 恢复状态；不得新增 migration、provider registry 或 evidence 表。
- 每项生产变更先有命名失败测试；阶段末才可更新 Harness 状态。

## 文件结构

- `src-tauri/src/ai_runtime/fresh_research_plan.rs`：纯 profile 选择、冻结预算和 body-free 状态校验。
- `src-tauri/src/ai_runtime/run_tool_loop.rs`：Run 内搜索/抓取 reservation、URL provenance、deadline、停止与恢复接线。
- `src-tauri/src/ai_runtime/web_evidence_broker.rs`：在 Host 授权输入下最多 3 个并发抓取，并报告实际 usage。
- `src-tauri/src/ai_runtime/agent_tool_loop.rs`：现有 `RunBudgetPolicy` 的 profile 收窄模型续接上限。
- `src-tauri/src/ai_runtime/normal_run_service.rs`：在 intake 后把 frozen profile 同时交给 executor 与 loop policy。
- `src-tauri/src/ai_runtime/agent_run_repository.rs`：继续使用既有 `fresh_research` resume JSON。
- `agent-harness/02-current-state-and-debt.md`、`agent-harness/05-implementation-roadmap.md`、`agent-harness/appendices/A-status-and-test-traceability.md`：只在全部退出条件有本轮证据时更新。

### Task 1: 冻结三档研究 profile

**Files:**

- Modify: `src-tauri/src/ai_runtime/fresh_research_plan.rs`
- Test: `src-tauri/src/ai_runtime/fresh_research_plan.rs`

**Interfaces:**

- Produces: `ResearchProfile::{Quick, Standard, Deep}`、`ResearchProfile::budget() -> ResearchBudget` 与 `FreshResearchPlan.profile`。

- [x] **Step 1: Write the failing test**

  #[test]
  fn explicit_deep_request_is_the_only_message_path_to_deep_profile() {
  let deep = plan_for("请深入研究上海本周电影");
  let normal = plan_for("推荐上海本周电影");
  let quick = plan_for("苹果现在股价多少");
  assert_eq!(deep.profile, ResearchProfile::Deep);
  assert_eq!(normal.profile, ResearchProfile::Standard);
  assert_eq!(quick.profile, ResearchProfile::Quick);
  }

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml fresh_research_plan::tests::explicit_deep_request_is_the_only_message_path_to_deep_profile`

Expected: FAIL because `FreshResearchPlan.profile` and `ResearchProfile` do not exist.

- [x] **Step 3: Write minimal implementation**

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub(crate) enum ResearchProfile { Quick, Standard, Deep }
  impl ResearchProfile {
  fn budget(self) -> ResearchBudget {
  match self {
  Self::Quick => ResearchBudget { max_searches: 1, max_fetches: 2, max_repairs: 1 },
  Self::Standard => ResearchBudget { max_searches: 3, max_fetches: 6, max_repairs: 1 },
  Self::Deep => ResearchBudget { max_searches: 5, max_fetches: 10, max_repairs: 1 },
  }
  }
  }

Only explicit `深入研究` / `deep research` selects Deep; recommendation/comparison/causal/multi-source wording selects Standard; simple current fact selects Quick.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml fresh_research_plan::tests::explicit_deep_request_is_the_only_message_path_to_deep_profile`

Expected: PASS.

- [ ] **Step 5: Commit**

  git add src-tauri/src/ai_runtime/fresh_research_plan.rs
  git commit -m "feat(ai): 冻结自适应研究档位"

### Task 2: 持久化完整且无正文的预算

**Files:**

- Modify: `src-tauri/src/ai_runtime/fresh_research_plan.rs`
- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Test: `src-tauri/src/ai_runtime/fresh_research_plan.rs`
- Test: `src-tauri/src/ai_runtime/run_tool_loop.rs`

**Interfaces:**

- Consumes: Task 1 的 `ResearchProfile` 与 `ResearchBudget`。
- Produces: schema-versioned `FreshResearchResumeState`，含 profile、最大值、已用搜索/抓取/修复、deadline 单调安全表示、query/URL hash 与 provider winner；不含正文或 URL。

- [ ] **Step 1: Write the failing tests**

  #[test]
  fn resume_state_rejects_used_counts_above_frozen_limits() {
  let state = FreshResearchResumeState {
  schema_version: 2, max_searches: 1, search_count: 1,
  max_fetches: 2, fetch_count: 3, max_repairs: 1, repair_count: 0,
  ..valid_resume_state()
  };
  assert_eq!(state.validate().expect_err("overrun").to_string(), "fresh_research_resume_state_invalid");
  }

  #[test]
  fn fresh_research_resume_restores_remaining_fetch_budget() {
  let resumed = executor_after_persisted_fetches(ResearchBudget { max_searches: 3, max_fetches: 2, max_repairs: 1 });
  assert_eq!(resumed.remaining_fresh_fetches().expect("remaining"), 1);
  }

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml resume_state_rejects_used_counts_above_frozen_limits fresh_research_resume_restores_remaining_fetch_budget`

Expected: FAIL because fetch consumption is not persisted or validated.

- [ ] **Step 3: Write minimal implementation**

Add `fetch_count`, `repair_count`, canonical URL hashes and `profile` to the resume state; bump its schema and reject mismatches. Add executor-owned counters, reserve before broker dispatch, persist after each reservation, and retain the existing `agent_run_steps` API.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml resume_state_rejects_used_counts_above_frozen_limits fresh_research_resume_restores_remaining_fetch_budget`

Expected: PASS.

- [ ] **Step 5: Commit**

  git add src-tauri/src/ai_runtime/fresh_research_plan.rs src-tauri/src/ai_runtime/run_tool_loop.rs
  git commit -m "feat(ai): 持久化研究剩余预算"

### Task 3: 在 web_search 内执行 provenance-safe 深抓取

**Files:**

- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/web_evidence_broker.rs`
- Test: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Test: `src-tauri/src/ai_runtime/web_evidence_broker.rs`

**Interfaces:**

- Consumes: Task 2 的剩余抓取预算和 current-Run URL hash ledger。
- Produces: `WebEvidenceUsage.successful_page_fetches`；只接受用户本轮明示或 current-Run 已登记的 canonical HTTPS URL。

- [ ] **Step 1: Write the failing tests**

  #[tokio::test]
  async fn subsequent_fetch_rejects_foreign_run_url_before_broker_dispatch() {
  let result = executor.execute_web_search(&json!({
  "query": "上海电影", "gap": "missing_timestamp",
  "urls": ["https://foreign.example/article"]
  }), 2).await.expect("tool result");
  assert_eq!(result.error.as_deref(), Some("web_url_not_in_current_run"));
  }

  #[tokio::test]
  async fn page_fetches_never_exceed_three_concurrent_requests() {
  let report = collect_with_instrumented_fetcher(6, 6).await;
  assert_eq!(report.peak_in_flight, 3);
  assert_eq!(report.successful_page_fetches, 6);
  }

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml subsequent_fetch_rejects_foreign_run_url_before_broker_dispatch page_fetches_never_exceed_three_concurrent_requests`

Expected: FAIL because URLs are forwarded without current-Run validation and fetches are serial.

- [ ] **Step 3: Write minimal implementation**

Validate/canonicalize URL candidates in `NormalRunToolExecutor`; allow current-Run evidence URLs or an URL extracted from the current user turn. De-duplicate hashes, reserve `min(remaining_fetches, urls.len())`, pass quota as `max_fetches`, and commit only actual successful fetch usage. In the broker, run batches of at most three fetch futures under its 8-second deadline and retain snippets on failure.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml subsequent_fetch_rejects_foreign_run_url_before_broker_dispatch page_fetches_never_exceed_three_concurrent_requests`

Expected: PASS.

- [ ] **Step 5: Commit**

  git add src-tauri/src/ai_runtime/run_tool_loop.rs src-tauri/src/ai_runtime/web_evidence_broker.rs
  git commit -m "feat(ai): 接通当前运行网页深抓取预算"

### Task 4: 将 deadline、修复与模型续接接入唯一循环

**Files:**

- Modify: `src-tauri/src/ai_runtime/fresh_research_plan.rs`
- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/agent_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/normal_run_service.rs`
- Test: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Test: `src-tauri/src/ai_runtime/agent_tool_loop_tests.rs`

**Interfaces:**

- Consumes: frozen profile and Task 2 state.
- Produces: profile-clamped model continuations, deadline-aware dispatch, one repair budget and early stop after two no-new-evidence rounds.

- [ ] **Step 1: Write the failing tests**

  #[tokio::test]
  async fn two_research_rounds_without_new_evidence_stop_before_next_provider_turn() {
  let outcome = run_tool_script([no_new_evidence_call(), no_new_evidence_call(), another_call()]).await;
  assert_eq!(outcome.error_code(), "agent_run_fresh_evidence_insufficient");
  assert_eq!(outcome.provider_turns, 2);
  }

  #[test]
  fn standard_profile_never_exceeds_its_model_continuation_limit() {
  assert_eq!(ResearchProfile::Standard.max_model_continuations(), 4);
  }

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml two_research_rounds_without_new_evidence_stop_before_next_provider_turn standard_profile_never_exceeds_its_model_continuation_limit`

Expected: FAIL because profile limits do not reach `AgentToolLoop` and no-evidence streaks are not tracked.

- [ ] **Step 3: Write minimal implementation**

Add model continuation, evidence and deadline limits to the frozen profile. Derive a narrowed copy of existing `RunBudgetPolicy` in `normal_run_service`, share one `Instant` with executor and loop, reject work at deadline/cancellation, and count only actual new current-Run evidence per research round. Use `max_repairs` for one bounded retry class; delete any field/helper that remains non-controlling.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml two_research_rounds_without_new_evidence_stop_before_next_provider_turn standard_profile_never_exceeds_its_model_continuation_limit`

Expected: PASS.

- [ ] **Step 5: Commit**

  git add src-tauri/src/ai_runtime/fresh_research_plan.rs src-tauri/src/ai_runtime/run_tool_loop.rs src-tauri/src/ai_runtime/agent_tool_loop.rs src-tauri/src/ai_runtime/normal_run_service.rs
  git commit -m "feat(ai): 约束自适应研究循环时限"

### Task 5: 删除失效行为并更新可审计状态

**Files:**

- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Modify: `agent-harness/02-current-state-and-debt.md`
- Modify: `agent-harness/05-implementation-roadmap.md`
- Modify: `agent-harness/appendices/A-status-and-test-traceability.md`
- Test: `src-tauri/src/ai_runtime/run_tool_loop.rs`

**Interfaces:**

- Consumes: Tasks 1–4 的完整控制面。
- Produces: 没有 `max_fetches: 0` 生产行为、没有无控制预算字段，且 AH-2 状态只引用本轮命名测试。

- [ ] **Step 1: Write the failing test**

  #[test]
  fn fresh_research_web_calls_forward_the_remaining_fetch_quota() {
  let input = broker_input_for_remaining_budget(&ResearchBudget::standard(), 2);
  assert_eq!(input.max_fetches, 4);
  }

- [ ] **Step 2: Run test and static scan to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml fresh_research_web_calls_forward_the_remaining_fetch_quota && rg -n "max_fetches: 0" src-tauri/src/ai_runtime/run_tool_loop.rs`

Expected: test FAIL and scan finds the legacy production literal.

- [ ] **Step 3: Write minimal removal and documentation update**

Remove production `max_fetches: 0`, duplicate planner helpers and tests that only assert inert behavior. Mark AH-2 “已验证” only after Task 5 verification is fresh; otherwise record “部分实现” with the exact remaining exit condition.

- [ ] **Step 4: Run relevant verification**

Run: `cargo test --manifest-path src-tauri/Cargo.toml fresh_research web_evidence_broker run_tool_loop && npm run docs:check && npm run agent:eval:smoke && cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`

Expected: every command exits 0 and documentation agrees with actual output.

- [ ] **Step 5: Commit**

  git add src-tauri/src/ai_runtime/run_tool_loop.rs agent-harness/02-current-state-and-debt.md agent-harness/05-implementation-roadmap.md agent-harness/appendices/A-status-and-test-traceability.md
  git commit -m "refactor(ai): 清理失效研究预算路径"

## Self-review

- Spec coverage: Tasks 1–4 implement profiles, complete budget/recovery, current-Run URL provenance, bounded concurrent fetches, model limits, deadline, cancellation and two-round no-evidence stop. Task 5 deletes the `max_fetches: 0` path and updates the single Harness fact source.
- No placeholders: each task has exact file paths, named tests, commands, expected red/green result and implementation boundary.
- Type consistency: `ResearchProfile`/`ResearchBudget` originate in Task 1; Task 2 owns resume state; Task 3 reports fetch usage; Task 4 consumes only these types in existing executor/loop paths.
