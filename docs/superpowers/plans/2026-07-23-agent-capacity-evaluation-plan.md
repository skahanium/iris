# Iris Agent Capacity Evaluation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Build a repeatable, privacy-safe evaluation suite that measures Iris Agent's answer quality and capacity boundaries for no-retrieval, local-only, web-only, and hybrid evidence tasks.

**Architecture:** Extract the normal-domain execution orchestration behind an internal headless entry point so production IPC, deterministic protocol doubles, and approved live profiles execute the same Intake → Context → Policy → Tool → Evidence → Engine path. Keep results outside application tables: an evaluation-only observer produces a strict-whitelist JSON summary and an optional local blind-review packet.

**Tech Stack:** Rust/Tauri 2, existing Tokio/rusqlite/MCP runtime, React/TypeScript npm scripts, existing synthetic Markdown fixture vault. No new dependencies.

## Global Constraints

- Do not create a worktree; implementation stays on `branch-1.2.15`.
- No public IPC/type contract, database schema, new dependency, or user-note mutation.
- Normal production behavior is the baseline: only behavior-equivalent testability refactors are allowed; do not fix answer/retrieval defects discovered by evaluation.
- `@file`, `@folder`, and `#tag` authorize local material, but do not force offline behavior; the web toggle remains an independent capability gate.
- Open foreground/current documents must never become implicit context. Ordinary/work tasks may perform full-vault retrieval only when clearly locally dependent; creative rewrite, novel, and classified tasks retain explicit-material requirements.
- Unauthorized local reads, web calls while offline, scope leaks, and high-risk unsupported claims have a zero-tolerance gate.
- Persisted results must contain only case ID, capability fingerprint, enum verdicts, counts, durations, token counts, and fact IDs. Never persist credentials, prompts, answer text, paths, URLs, tool payloads, evidence excerpts, or real-note content.
- Live verification uses only explicitly approved profiles and existing encrypted credential storage; API keys must never be read into environment variables or logs.
- Follow TDD: every production behavior begins with a focused failing test and its observed RED output.

---

### Task 1: Headless normal-domain execution service

**Files:**

- Create: `src-tauri/src/ai_runtime/normal_run_service.rs`
- Modify: `src-tauri/src/ai_runtime/mod.rs`
- Modify: `src-tauri/src/commands/assistant_commands.rs`
- Modify: `src-tauri/src/ai_runtime/run_tool_loop.rs`
- Test: `src-tauri/src/ai_runtime/normal_run_service_tests.rs`

**Interfaces:**

- Produce an internal `execute_normal_run` service that performs the same policy, context assembly, evidence registration, tool-surface construction, provider route selection, RunEngine dispatch, and terminal error handling currently owned by `spawn_normal_direct_run`/`dispatch_normal_run_after_context`.
- The service accepts the current `Arc<AppState>`, accepted run, optional vault, optional `AppHandle`, and `RunEventSink`; it remains `pub(crate)`.
- `NormalRunToolExecutor` accepts `Option<AppHandle>`; desktop production passes `Some`, headless tests pass `None`.

- [x] Write a failing unit test that executes an accepted normal direct Run through the new service with a recording sink and asserts the same terminal state/content lifecycle as the existing command path.
- [x] Run the focused test and capture the expected missing-service compile/test failure.
- [x] Extract the orchestration with no semantic changes; leave `assistant_run_start` as a thin Tauri sink/spawn adapter.
- [x] Update tool execution to carry an optional app handle without weakening policy, audit, evidence, or dispatcher behavior.
- [x] Add focused tests for tool-loop execution with a `None` app handle and for unchanged desktop construction.
- [x] Run focused Rust tests, format, clippy for touched crate targets, and commit with a Chinese Conventional Commit message.

### Task 2: Evaluation model, scoring, and deterministic protocol doubles

**Files:**

- Create: `src-tauri/src/ai_runtime/agent_capacity_eval.rs`
- Modify: `src-tauri/src/ai_runtime/mod.rs`
- Test: `src-tauri/src/ai_runtime/agent_capacity_eval_tests.rs`
- Create: `docs/eval/fixtures/agent-answer-v1.json`

**Interfaces:**

- Define a test-only manifest with case ID, evidence group, language/domain, web state, explicit references/scope, implicit-vault expectation, required facts/sources, tool policy, answer mode, citation expectations, and disclosure constraints.
- Define strict verdicts for authorization, required evidence, fact correctness, citation support, route efficiency, degradation/clarification, and safety.
- Use deterministic local LLM and MCP protocol doubles only at the external transport boundary; exercise the real Iris model gateway, tool executor, retrieval broker, evidence ledger, and RunEngine.

- [x] Write failing tests for manifest parsing, required-source scoring, offline degradation, unauthorized local-read failure, and non-fatal unnecessary web search.
- [x] Run focused tests and capture RED failures.
- [x] Implement the smallest manifest/scorer/double utilities that pass the tests, keeping raw prompt/response data in memory only.
- [x] Add complete protocol-contract tests for OpenAI-compatible, Anthropic Messages, Responses continuation, MCP search-only, MCP search+fetch, malformed output, timeout, retry, and HTTPS evidence normalization.
- [x] Run focused tests and commit with a Chinese Conventional Commit message.

### Task 3: Core scenarios, telemetry, and CI/manual commands

**Files:**

- Modify: `src-tauri/src/ai_runtime/agent_capacity_eval.rs`
- Modify: `src-tauri/src/ai_runtime/agent_tool_loop.rs`
- Modify: `src-tauri/src/ai_runtime/run_engine.rs`
- Modify: `package.json`
- Create: `scripts/agent-eval.mjs`

**Interfaces:**

- Generate 48 total core scenarios from 24 base questions: 12 scenarios each for no retrieval, local, web, and hybrid groups; the web-toggle variants are included in the 48 total.
- Add an evaluation-only telemetry tap for model turns, token usage, finish reason, first visible token, tool calls, timing, and truncation/budget outcomes. Production persistence remains unchanged.
- Add `agent:eval:smoke`, `agent:eval`, and `agent:eval:live` commands. Results are written beneath ignored `target/agent-eval/` and summary serialization enforces the whitelist.

- [x] Write failing tests for exact 48-scenario generation, group distribution, Chinese/English/mixed language ratio, telemetry aggregation, and summary redaction.
- [x] Run focused tests and capture RED failures.
- [x] Implement scenario generation, telemetry tap, scripts, and strict result serializer.
- [x] Ensure smoke runs a stratified subset plus hard boundary checks; full deterministic run executes all core scenarios.
- [x] Run focused tests, command smoke test, and commit with a Chinese Conventional Commit message.

### Task 4: Capacity staircase, security track, reports, and RAG documentation correction

**Files:**

- Modify: `src-tauri/src/ai_runtime/agent_capacity_eval.rs`
- Modify: `docs/eval/rag-v2-broker-evaluation.md`
- Create: `docs/eval/agent-answer-capacity.md`
- Create: `docs/eval/results/v1.2.15-agent-capacity.json`

**Interfaces:**

- Implement geometric pressure staircases plus focused refinement for input, history, local material, retrieval scale/distractors, reasoning depth, tool loop, web evidence/latency, output, and six combined terminal cases.
- Add 12 independent security cases for implicit-document reads, unauthorized vault search, injection, scope leaks, offline web dispatch, and unnecessary local-to-web disclosure.
- Generate a blind-review CSV locally for all boundary and rule-ambiguous samples plus a 20% stratified sample; do not commit raw answers.

- [x] Write failing tests for each hard boundary: 16,001-character prompt rejection, 13 explicit materials, 32K+ context, ninth model turn, 25th tool call, oversized tool payload, ninth web evidence, and 32,001-character answer.
- [x] Run focused tests and capture RED failures.
- [x] Implement staircase scheduling, stable-boundary calculation (five repetitions; at least four current passes and at most two next-level passes), security scenarios, report generation, and blind-review packet generation.
- [x] Correct RAG fixture documentation from 54/6 to 50/10 and document all-required-source recall.
- [x] Run deterministic full evaluation and commit with a Chinese Conventional Commit message.

### Task 5: Live-profile preflight and approved MiniMax/AnySearch pilot

**Files:**

- Modify: `src-tauri/src/ai_runtime/agent_capacity_eval.rs`
- Modify: `scripts/agent-eval.mjs`
- Modify: `docs/eval/agent-answer-capacity.md`

**Interfaces:**

- Preflight exposes only anonymous profile IDs and capability fingerprints: endpoint family, tools/streaming/reasoning support, context/output buckets, and MCP search/fetch/transport capability.
- Approved live profiles run the same headless path as deterministic tests. Selected non-secret routing/MCP metadata is copied into a temporary state; credential references hydrate through existing encrypted storage only at dispatch.
- Each approved configuration first runs a 12-scenario pilot. Other providers/services remain explicitly `contract_verified` or `live_not_tested`, never presented as live verification.

- [x] Write failing tests for anonymous preflight output, rejected unapproved profile IDs, strict redaction, temporary-state isolation, and live-result status labels.
- [x] Run focused tests and capture RED failures.
- [x] Implement preflight and pilot control flow without reading API key values or writing into the real application database.
- [x] Run the approved MiniMax/AnySearch pilot only after profile approval, produce the capability-specific report section, and leave further live boundary expansion behind the explicit cost checkpoint.
- [x] Run the required full quality suite and commit with a Chinese Conventional Commit message.

### Task 6: Whole-branch audit and final verification

**Files:**

- Review all changed files and generated versioned artifacts.

- [x] Request a broad code review covering policy preservation, data redaction, TDD evidence, provider/MCP claim boundaries, and test adequacy.
- [x] Resolve every Critical or Important finding with focused tests and re-review.
- [x] Run `npm run rag:eval`, `npm run agent:eval:smoke`, `npm run agent:eval`, Rust format/clippy/test, TypeScript lint/format/typecheck/test/e2e, `npm run audit:rust`, `npm audit`, and `npm run docs:check`.
- [x] Record the evidence in the final audit; do not claim live completion for profiles not explicitly approved and run.
