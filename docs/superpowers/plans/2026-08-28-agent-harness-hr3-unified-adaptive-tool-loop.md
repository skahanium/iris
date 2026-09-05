# Agent Harness HR-3：统一自适应工具循环实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 以单一冻结 `RunBudgetPolicy`、通用资源进展判定和一次保留的最终综合，统一 Web、本地、external read 与 runtime 的多轮工具循环。

**Architecture:** `AgentToolLoop` 成为 Direct 之外唯一的模型—工具编排器：它从持久化预算取得总量和分类上限，以 catalog 的受控工具类别计数，基于资源 identity 判断完整轮次是否产生新进展，并在两轮无进展或探索空间将尽时关闭业务工具、保留最后一次模型综合。`NormalRunToolExecutor` 仅执行、登记证据和暴露安全进展身份；Web 不再拥有 `FreshResearchPlan`、`EvidenceGap`、单独 deadline 或 search/fetch/repair 计数器。旧预算 JSON 与旧领域 envelope 继续保守读取，旧 `fresh_research` checkpoint 不重放也不删除。

**Tech Stack:** Rust、Tokio、SQLite JSON 存储、现有 `AgentToolLoop`、Tool Catalog、Rust 单元/集成测试；不新增依赖、迁移、IPC、Provider 或模型名称分支。

## Global Constraints

- 不创建 worktree；在已授权的 `branch-v1.3.0` 当前工作区施工。
- 不新增数据库表、迁移、IPC 字段、Provider、依赖或领域专用状态机。
- `RunBudgetPolicy` 是模型轮次、总工具数和分类工具数的唯一冻结事实源；旧 JSON 只按同 envelope 的保守 canonical policy 物化。
- 分类为 `local=12`、`network=6`、`external_read=6`、`runtime=4`、`confirmed_change=6`，所有类别继续受 24 总工具上限；Direct 为 1/0。
- `ToolLoopExecutor` 不授予权限；它只执行已经冻结且经 catalog/permission gate 允许的调用。
- 不实现 HR-4 自然正文终局/澄清或 HR-5 确认后验证；严格证据门禁保持。
- 测试先行：每项生产行为先写失败断言并确认旧实现因缺少统一预算、进展或综合行为而失败。

---

### Task 1: 冻结分类预算并兼容读取旧 policy

**Files:**

- Modify: `src-tauri/src/ai_runtime/run_contract.rs:286-499`
- Modify: `src-tauri/src/ai_runtime/agent_run_repository.rs:2090-2230`
- Modify: `src-tauri/src/ai_runtime/agent_run_repository_tests.rs`
- Modify: `src-tauri/src/ai_runtime/agent_tool_loop_tests.rs`

**Interfaces:**

- Produces `RunBudgetPolicy::{max_local_tool_calls,max_network_tool_calls,max_external_read_tool_calls,max_runtime_tool_calls,max_confirmed_change_calls}` with schema version `2`.
- Produces `RunBudgetPolicy::for_envelope` canonical `Direct=1/0` and `Standard/Delegated/DurableApply=8/24` plus HR-3 category limits.
- Consumes old `{}`、schema-1 full policy 与 pre-token legacy policy, materializing the exact schema-2 policy implied by the stored envelope.

- [x] **Step 1: Write failing policy tests**

Add assertions that a new Standard policy contains `12/6/6/4/6`, Direct contains zero category limits, a schema-1 full policy with no category fields is materialized to schema 2, and a tampered category limit fails closed.

- [x] **Step 2: Run the exact tests and confirm RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib agent_run_repository_tests`

Expected: failures because schema-1 policy is currently the only accepted shape and no category fields exist.

- [x] **Step 3: Implement the minimal policy and compatibility projection**

Add the five persisted limits to `RunBudgetPolicy`; change canonical new policies to schema 2; add exact schema-1-with-token-fields and pre-token legacy projection types in `materialize_budget_policy`. Never deserialize missing fields as unbounded values and never accept a policy that differs from the envelope-derived canonical policy.

- [x] **Step 4: Run the exact tests and confirm GREEN**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib agent_run_repository_tests`

Expected: PASS with legacy materialization and tamper rejection both covered.

### Task 2: 将工具类别收口到 catalog，并建立通用进展身份

**Files:**

- Modify: `src-tauri/src/ai_runtime/tool_catalog_impl.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/capability.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/tests.rs`
- Modify: `src-tauri/src/ai_runtime/agent_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/agent_tool_loop_tests.rs`

**Interfaces:**

- Produces `ToolBudgetClass::{Local,Network,ExternalRead,Runtime,ConfirmedChange}` and `catalog_tool_budget_class(name)`.
- Built-ins obtain their class from `ToolExecutionMetadata` or deterministic catalog access/confirmation metadata; unknown frozen external snapshots resolve only to `ExternalRead`.
- Produces a non-content `ToolProgressIdentity` assembled only from resource ID, canonical URL, content hash, revision, or target file hash returned by a successful tool.

- [x] **Step 1: Write failing catalog and loop tests**

Add a table-driven class test for `search_keyword`→local, `web_search`→network, `system_time_now`→runtime and `insert_text_at_cursor`→confirmed_change; add an external unknown name expectation of external_read. Add a loop test where two different Web queries produce the same canonical URL/content hash and a local multi-hop test where distinct note hashes count as progress.

- [x] **Step 2: Run exact tests and confirm RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib agent_tool_loop_tests tool_catalog::tests`

Expected: missing classification/progress API or the current Web-only evidence counter behavior.

- [x] **Step 3: Implement catalog-only classification and safe identity extraction**

Define the category enum in the catalog facade. Use the already-present `cost_class` as the explicit metadata source, with exhaustive access/confirmation fallback for catalog entries and `ExternalRead` only for non-catalog frozen tools. Extract progress identities from only named identity fields; never hash raw result bodies or pass note contents to logs/prompts.

- [x] **Step 4: Run exact tests and confirm GREEN**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib agent_tool_loop_tests tool_catalog::tests`

Expected: PASS; same resource across different queries is no progress, distinct local resource identity is progress.

### Task 3: 把预算、进展和最终综合收口到 AgentToolLoop

**Files:**

- Modify: `src-tauri/src/ai_runtime/agent_tool_loop.rs:1-660`
- Modify: `src-tauri/src/ai_runtime/run_engine/mod.rs:700-785`
- Modify: `src-tauri/src/ai_runtime/agent_tool_loop_tests.rs`

**Interfaces:**

- `AgentToolLoop::from_policy` enforces both total and category counters from the frozen policy.
- `AgentToolLoop` tracks progress per complete model—tool round; two rounds with no new identity, or one reserved final turn remaining, replace the business-tool surface with an explicit synthesis-only turn.
- `submit_final_answer`, when present for a strict legacy contract, remains available in synthesis-only mode and does not consume a business-tool category or total dispatch budget.

- [x] **Step 1: Write failing behavioral regressions**

Replace `hr1_no_progress_still_fails_before_the_reserved_final_synthesis` with a normal green expectation: two no-progress rounds result in a third, tool-free final synthesis. Add tests that network call 7, local call 13, external call 7 and runtime call 5 are rejected without reaching the executor; add a mixed local/Web/external sequence that completes at the global 24 boundary and never exceeds its category counters. Preserve existing successful-duplicate and failed-twice tests.

- [x] **Step 2: Run exact tests and confirm RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib agent_tool_loop_tests`

Expected: current loop returns `fresh_research_no_new_evidence`, and category limits do not exist.

- [x] **Step 3: Implement one generic loop controller**

Remove `ResearchBudget`, fresh-research deadline and executor research hooks. Count only concrete, authorized business dispatch attempts. On category exhaustion return a bounded failed tool result (so the model can synthesize); on two no-progress complete rounds or exploration exhaustion append a generic system synthesis instruction and hide business tools for the next model turn. Do not return `ToolLoopLimit` merely because progress stopped; fail only if the final synthesis is empty, strict evidence remains absent, a cancellation occurs, or a hard permission boundary is crossed.

- [x] **Step 4: Run exact tests and confirm GREEN**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib agent_tool_loop_tests`

Expected: all loop regressions pass and no expected error string contains `fresh_research`.

### Task 4: 删除 Web 专用研究控制面并让 normal route 使用同一循环

**Files:**

- Delete: `src-tauri/src/ai_runtime/fresh_research_plan.rs`
- Modify: `src-tauri/src/ai_runtime/mod.rs`
- Modify: `src-tauri/src/ai_runtime/normal_run_service.rs`
- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/run_engine/mod.rs`
- Modify: `src-tauri/src/ai_runtime/agent_run_repository.rs`
- Modify: `src-tauri/src/ai_runtime/fresh_domains/service.rs`
- Modify: `src-tauri/src/ai_runtime/tool_catalog/web.rs`
- Modify: `src-tauri/src/ai_runtime/normal_run_service_tests.rs`
- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs` tests

**Interfaces:**

- New ToolLoop Runs, including `WebRequired`, enter the same `AgentToolLoop` with the normal authorized surface; strict runs keep `requires_web_evidence()` as their final gate.
- `web_search` accepts only `query` and current-Run `urls`; it no longer accepts a closed `gap` enum.
- Legacy domain DTO dispatch may still read an old frozen mapping, but no longer receives a `FreshResearchPlan`, `EvidenceGap`, search/fetch/repair counters, or dedicated deadline.

- [x] **Step 1: Write failing integration regressions**

Add/convert tests proving: a poor first Web result can be followed by a different query that yields a new resource; a same-resource second query triggers tool closure then a final answer; local and Web calls share the same generic loop; a strict Web run uses `web_search` in the unified loop and fails only when it reaches finalization without current-Run evidence. Add a static behavior assertion that `web_search` schema does not contain `gap`.

- [x] **Step 2: Run exact tests and confirm RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib normal_run_service_tests run_tool_loop::tests agent_tool_loop_tests`

Expected: current strict path deterministically prefetches outside the loop; `FreshResearchPlan` remains reachable; no-progress returns a Web-specific error.

- [x] **Step 3: Delete the parallel controller and rewire production**

Delete `fresh_research_plan` and its repository checkpoint API; remove `CurrentTaskShape`, city extraction, evidence-gap parsing, `with_fresh_research_budget`, research counters and research deadline. Make `dispatch_normal_run_after_context` select the generic ToolLoop for every new ToolLoop/WebRequired Run, with WebRequired represented only by its frozen surface and final evidence requirement. Keep the remaining deterministic prefetch helper inside the historical non-empty-domain compatibility adapter for HR-6; it has no research-planning dependency and is unreachable from new intake.

- [x] **Step 4: Run exact tests and confirm GREEN**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib normal_run_service_tests run_tool_loop::tests agent_tool_loop_tests`

Expected: PASS; no production source imports `fresh_research_plan`, `EvidenceGap`, or a Web-specific loop deadline.

### Task 5: 事实源、评测和交付审计

**Files:**

- Modify: `ARCHITECTURE.md`
- Modify: `agent-harness/02-current-state-and-debt.md`
- Modify: `agent-harness/05-implementation-roadmap.md`
- Modify: `agent-harness/06-evaluation-performance-and-acceptance.md`
- Modify: `agent-harness/appendices/A-status-and-test-traceability.md`
- Modify: `docs/README.md`

- [x] **Step 1: Update implementation facts only after green code tests**

Mark HR-3 accepted only if the source confirms one policy, class caps, generic no-progress synthesis and removed Web controller. State separately that HR-4 ordinary finalization and HR-5 confirmed writes remain pending.

- [x] **Step 2: Run exact verification**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib agent_run_repository_tests
cargo test --manifest-path src-tauri/Cargo.toml --lib agent_tool_loop_tests
cargo test --manifest-path src-tauri/Cargo.toml --lib normal_run_service_tests
cargo test --manifest-path src-tauri/Cargo.toml --lib run_tool_loop::tests
cargo test --manifest-path src-tauri/Cargo.toml --lib tool_catalog::tests
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run lint
npm run format:check
npm run typecheck
npm run docs:check
git diff --check
```

Expected: all targeted regressions and repository gates pass; no real Provider is called.

- [x] **Step 3: Conduct the requirement-by-requirement self-audit**

Verify each HR-3 contract item against a named test and final source search: one loop, 8/24 + categories, duplicate rules, two no-progress synthesis, final-turn reservation, local/Web/external mixing, cancellation, total limits, no `FreshResearchPlan`/`EvidenceGap` production path, no Provider-specific branch, compatibility reads and documentation consistency.

- [ ] **Step 4: Commit and push**

Run `git diff --cached --check`, inspect the staged file list and commit all reviewed HR-3 changes with a Chinese Conventional Commit message. Push `branch-v1.3.0` only after the checks above pass.
