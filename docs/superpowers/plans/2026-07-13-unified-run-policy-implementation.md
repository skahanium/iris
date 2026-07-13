# Unified Run Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route every new Agent Run dispatch through one deterministic policy decision before it can select a Provider or capability.

**Architecture:** Add a Run-facing policy DTO and evaluation method to `PolicyDecisionEngine`; preserve its existing document-scope resolution as the authority for explicit references. Inject the result into normal and classified direct dispatches after accepted persistence. The classified executor receives only a direct text capability decision and remains CEF-only.

**Tech Stack:** Rust, Tauri 2, SQLite for normal Run facts, CEF for classified Run facts, existing `PolicyDecisionEngine`, `RunIntake`, and `ModelGateway`.

---

### Task 1: Define Run policy contracts

**Files:**

- Modify: `D:\Iris\src-tauri\src\ai_runtime\policy_decision_engine.rs`
- Test: `D:\Iris\src-tauri\src\ai_runtime\policy_decision_engine.rs`

- [ ] **Step 1: Write failing policy tests**

Add tests covering: offline envelope requesting `web.search` is denied; classified envelope requesting `web.search` is denied; an explicit reference denied `send_to_model` is denied; normal direct text answer is allowed with `model.text`.

- [ ] **Step 2: Run the focused tests**

Run: `cargo test --offline --manifest-path D:\Iris\src-tauri\Cargo.toml policy_decision_engine --lib -- --test-threads=1`

Expected: FAIL because `RunPolicyRequest` and `evaluate_run` do not exist.

- [ ] **Step 3: Implement immutable request and decision DTOs**

Add `RunPolicyRequest { envelope, explicit_reference_paths, requested_capabilities }` and `RunPolicyDecision { allowed_capabilities, denied_capabilities, denial_code }`. Implement `PolicyDecisionEngine::evaluate_run` using only the request and document policies; do not read a database, editor state, scene, intent, or raw note body.

- [ ] **Step 4: Re-run focused policy tests**

Run the command from Step 2.

Expected: PASS.

### Task 2: Persist and evaluate direct Run capability intent

**Files:**

- Modify: `D:\Iris\src-tauri\src\ai_runtime\run_intake.rs`
- Modify: `D:\Iris\src-tauri\src\ai_runtime\run_intake_tests.rs`
- Modify: `D:\Iris\src-tauri\src\ai_runtime\run_contract.rs`

- [ ] **Step 1: Write failing Intake tests**

Add one normal direct-answer test asserting its persisted envelope requests `model.text`, and one classified request with Web enabled asserting Intake refuses it before a CEF Run is accepted.

- [ ] **Step 2: Run focused Intake tests**

Run: `cargo test --offline --manifest-path D:\Iris\src-tauri\Cargo.toml run_intake_tests --lib -- --test-threads=1`

Expected: FAIL until the request capability list and classified denial are represented consistently.

- [ ] **Step 3: Implement deterministic capability requirements**

Have Intake derive `model.text`, optional `model.vision`, and Web/write capability IDs from the envelope. Reject classified non-direct/offline requests before `classified_run_accept`; do not alter the accepted normal Run transaction.

- [ ] **Step 4: Re-run focused Intake tests**

Run the command from Step 2.

Expected: PASS.

### Task 3: Gate normal and classified direct dispatch

**Files:**

- Modify: `D:\Iris\src-tauri\src\commands\assistant_commands.rs`
- Modify: `D:\Iris\src-tauri\src\ai_runtime\classified_run_engine.rs`
- Modify: `D:\Iris\src-tauri\src\ai_runtime\run_engine.rs`
- Test: `D:\Iris\src-tauri\src\ai_runtime\classified_run_engine.rs`
- Test: `D:\Iris\src-tauri\src\ai_runtime\run_engine_tests.rs`

- [ ] **Step 1: Write failing dispatch-order tests**

Use a Provider test double that increments a counter. Assert a denied decision leaves the counter at zero and writes only a safe `permission_denied` event; assert an allowed normal direct answer invokes the Provider once.

- [ ] **Step 2: Run direct Run tests**

Run: `cargo test --offline --manifest-path D:\Iris\src-tauri\Cargo.toml run_engine --lib -- --test-threads=1`

Expected: FAIL because dispatch bypasses `PolicyDecisionEngine`.

- [ ] **Step 3: Implement policy gate and pre-dispatch recheck**

Evaluate policy after accepted persistence and before route hydration. Emit a persisted `permission_denied` event on refusal. Pass only approved `model.text`/`model.vision` requirements into capability routing. Re-evaluate immediately before future Web/tool dispatch points; no policy decision may be supplied by legacy commands.

- [ ] **Step 4: Re-run direct Run tests**

Run the command from Step 2.

Expected: PASS.

### Task 4: Verify regression and documentation evidence

**Files:**

- Modify: `D:\Iris\progress.md`
- Modify: `D:\Iris\task_plan.md`

- [ ] **Step 1: Run combined policy and Run tests**

Run: `cargo test --offline --manifest-path D:\Iris\src-tauri\Cargo.toml policy_decision_engine run_intake_tests classified_run run_engine --lib -- --test-threads=1`

Expected: PASS with no warnings.

- [ ] **Step 2: Run formatting and diff checks**

Run: `cargo fmt --manifest-path D:\Iris\src-tauri\Cargo.toml -- --check` and `git -C D:\Iris diff --check`.

Expected: both commands succeed.

- [ ] **Step 3: Record verified status only**

Append exact test evidence to `D:\Iris\progress.md`; do not mark the overall agent-harness refactor complete because legacy-chain deletion, migration, UI switch, evaluation, and release gates remain separate plan items.
