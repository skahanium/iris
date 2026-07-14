# MCP Run Evidence Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make web-enabled Agent Runs deterministically obtain MCP evidence before answering, preserve accurate safe failures, prevent internal reasoning from becoming visible content, and make MCP diagnostics explicit and non-duplicated.

**Architecture:** The Run engine owns the first WebRequired evidence acquisition through the same broker used by MCP diagnostics. The model receives bounded, persisted evidence and is responsible only for reasoning and expression; later tools remain optional and policy-bound. Frontend diagnostic state is ephemeral and exists only after an explicit live diagnostic.

**Tech Stack:** Rust, Tauri 2, React 19, TypeScript, Vitest, SQLite, existing WebEvidenceBroker and Agent Run event log.

## Global Constraints

- Do not add business entities, persistent diagnostic history, migrations, credentials, or new dependencies.
- Never log, persist, or render API keys, raw MCP payloads, user-private content, or model reasoning.
- Use test-first red/green cycles; no production behavior change without a failing focused regression test.
- Preserve normal direct local answers and avoid web work for explicit local, rewrite, translation, creative, and greeting requests.
- First WebRequired search has a 15-second total deadline, returns bounded search evidence only, and does not auto-fetch pages.

---

### Task 1: Lock the regressions and remove stale model-slot test assumptions

**Files:**

- Modify: `src-tauri/src/ai_runtime/run_intake_tests.rs`
- Modify: `src-tauri/src/ai_runtime/run_engine_tests.rs`
- Modify: `src-tauri/src/ai_runtime/model_gateway/streaming.rs`
- Modify: `src-tauri/src/ai_runtime/tool_dispatch/runtime.rs`

- [ ] Add failing Run Intake tests for web-enabled current-event queries and local-only transformation exemptions.
- [ ] Add failing Run Engine tests proving WebRequired evidence is obtained before provider dispatch and that evidence failures dispatch no model call.
- [ ] Add failing streaming tests for a longer-than-500-character meta-analysis prefix and final persistence without that prefix.
- [ ] Replace the obsolete capability snapshot assertion for the removed `vision` slot with model-pool assertions.

### Task 2: Build the engine-owned WebRequired evidence stage

**Files:**

- Modify: `src-tauri/src/commands/assistant_commands.rs`
- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/web_evidence_broker.rs`
- Modify: `src-tauri/src/ai_runtime/run_engine.rs`
- Modify: `src-tauri/src/ai_runtime/run_contract.rs`

- [ ] Route WebRequired Runs through one bounded pre-answer evidence stage before model selection/dispatch.
- [ ] Reuse the selected MCP provider, tool mapping, audit/evidence ledger and packet format; reject missing, timeout, runtime and parse failures with typed safe codes.
- [ ] Ensure initial WebRequired answers do not need model function-calling support; retain capability filtering for real follow-up tool surfaces.
- [ ] Emit existing durable stage/tool/evidence events in their valid lifecycle order and prevent model calls after evidence-stage failure.

### Task 3: Make visibility normalization authoritative

**Files:**

- Modify: `src-tauri/src/ai_runtime/text_support.rs`
- Modify: `src-tauri/src/ai_runtime/model_gateway/streaming.rs`
- Modify: `src-tauri/src/ai_runtime/run_engine.rs`

- [ ] Remove the arbitrary meta-analysis suppression budget.
- [ ] Normalize the visible stream and both finalization paths with one shared safe-output helper.
- [ ] Fail safely when normalization leaves no answer; never persist or replay stripped content.

### Task 4: Consolidate MCP diagnostics UI and IPC

**Files:**

- Modify: `src-tauri/src/commands/ai_commands.rs`
- Modify: `src/lib/ipc.ts`
- Modify: `src/components/ai/skills/McpProfilesPanel.tsx`
- Modify: `src/components/ai/skills/McpProfileCard.tsx`
- Modify: relevant Vitest files under `tests/`

- [ ] Remove the duplicate Test Connection control and static/non-live diagnostic mode.
- [ ] Retain one explicit live diagnostic that runs discovery, mapping, credential-presence and one bounded search parse through the runtime path.
- [ ] Keep results only in component state for the open panel; clear them after close, save, edit, enable/disable, or provider reload.
- [ ] Render no diagnostic result area before the explicit action and avoid static wording that claims real connectivity.

### Task 5: Verify the complete contract

- [ ] Run focused Rust and Vitest regressions after every task.
- [ ] Run `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `npm run lint`, `npm run format:check`, `npm run typecheck`, `npm run test`, `npm run audit:rust`, `npm audit`, and `npm run test:e2e`.
- [ ] Audit each plan requirement against source, event behavior, and command output before reporting completion.
