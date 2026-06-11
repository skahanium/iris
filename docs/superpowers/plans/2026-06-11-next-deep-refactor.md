# Iris Deep Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the staged P0/P1/P2 work from `next.md` without changing user-visible IPC contracts, database schema, provider wire shape, or Markdown authority rules.

**Architecture:** Treat `next.md` as the approved design input and execute in checkpoints. Each checkpoint extracts one responsibility behind a narrow hook/module boundary, adds or tightens contract tests first, then verifies behavior with targeted and full quality gates.

**Tech Stack:** Tauri 2.x, Rust, React 19, TipTap/ProseMirror, TailwindCSS/shadcn/ui, SQLite/sqlite-vec, Vitest, Cargo tests and benches.

---

### Task 1: Assistant Artifact State Boundary

**Files:**

- Create: `src/components/ai/hooks/useAssistantArtifacts.ts`
- Modify: `src/components/ai/UnifiedAssistantPanel.impl.tsx`
- Test: `tests/use-assistant-artifacts.test.tsx`
- Test: `tests/assistant-panel-performance-contract.test.ts`

- [ ] **Step 1: Write failing reducer/hook tests**

```ts
it("clears all task artifact state without touching conversation state", () => {
  // Render the hook, seed writing/citation/organize/research/document/error state,
  // call clearTaskSurfaces(), and assert every artifact field returns to empty.
});

it("accepts and rejects writing patches through the artifact boundary", async () => {
  // Mock patchApply, seed two patches, accept one, reject the other,
  // and assert onPatchApplied receives the patched note content.
});

it("applies only selected organize suggestions and refreshes the vault", async () => {
  // Mock organizeApply, select/toggle suggestions, apply,
  // and assert applied suggestions are removed and selection is cleared.
});
```

- [ ] **Step 2: Verify tests fail for missing hook**

Run: `npm run test -- tests/use-assistant-artifacts.test.tsx tests/assistant-panel-performance-contract.test.ts`

Expected: FAIL because `useAssistantArtifacts` does not exist and the panel still owns artifact state directly.

- [ ] **Step 3: Extract artifact hook**

Move `writingPatches`, `citationResult`, `organizeSuggestions`, `organizeSelection`, `researchResult`, `docSummary`, `docIssues`, `lastError`, `clearTaskSurfaces`, patch accept/reject/copy, organize toggle/clear/apply, and their setters into `useAssistantArtifacts`. Keep existing IPC calls through injectable dependencies for tests.

- [ ] **Step 4: Rewire panel to hook return values**

Replace direct state setters in `UnifiedAssistantPanel.impl.tsx` with hook methods while preserving `AssistantTaskSurfaces` props and existing `data-testid` values.

- [ ] **Step 5: Verify targeted tests pass**

Run: `npm run test -- tests/use-assistant-artifacts.test.tsx tests/assistant-panel-performance-contract.test.ts`

Expected: PASS.

### Task 2: Assistant Task Execution Boundary

**Files:**

- Create: `src/components/ai/hooks/useAssistantTasks.ts`
- Modify: `src/components/ai/UnifiedAssistantPanel.impl.tsx`
- Test: `tests/use-assistant-tasks.test.tsx`
- Test: `tests/assistant-execute-ipc.test.ts`

- [ ] **Step 1: Write failing task hook tests**

```ts
it("runs writing, citation, organize, chapter, document, research, and chat through assistantExecute with unchanged payload keys", async () => {
  // Use dependency injection for assistantExecute/contextAssemble/parseDocumentChapters.
  // Assert each intent sends the same fields currently sent by the panel.
});

it("parses document chapters only for chapter tasks", async () => {
  // Run chat, writing, document, and chapter. Assert parseDocumentChapters is called only for chapter.
});

it("reports task failures as system errors and AssistantActionState error status", async () => {
  // Force assistantExecute rejection and assert lastError, system message, and actionState.
});
```

- [ ] **Step 2: Verify tests fail for missing hook**

Run: `npm run test -- tests/use-assistant-tasks.test.tsx tests/assistant-execute-ipc.test.ts`

Expected: FAIL because `useAssistantTasks` does not exist.

- [ ] **Step 3: Extract task execution hook**

Move `assembleContextForChat`, `executeKnowledgeChat`, `runKnowledgeChat`, `runWriting`, `runCitation`, `runOrganize`, `runChapter`, `runDocumentCheck`, `runResearch`, and `send` into `useAssistantTasks`. Keep `getNoteContent: () => string` lazy and preserve selected packet handling, token usage accumulation, evidence refresh notices, and `assistantRun` state updates.

- [ ] **Step 4: Rewire panel to hook return values**

Panel should keep only top-level composition, refs, chrome snapshot, task surfaces, conversation surface, composer, and dialogs.

- [ ] **Step 5: Verify targeted tests pass**

Run: `npm run test -- tests/use-assistant-tasks.test.tsx tests/assistant-execute-ipc.test.ts tests/assistant-panel-performance-contract.test.ts`

Expected: PASS.

### Task 3: Research Control Boundary

**Files:**

- Create: `src/components/ai/hooks/useResearchControl.ts`
- Modify: `src/components/ai/UnifiedAssistantPanel.impl.tsx`
- Test: `tests/use-research-control.test.tsx`
- Test: `tests/e2e/unified-assistant-contract.test.ts`

- [ ] **Step 1: Write failing research hook tests**

```ts
it("tracks research progress listener states without changing test selectors", async () => {
  // Emit running/completed/failed/aborted payloads and assert running/action status transitions.
});

it("aborts the active research request and marks progress aborted", async () => {
  // Seed request id, call abortResearch, assert researchAbort id and local state.
});

it("generates a research note suggestion message", async () => {
  // Seed research result, call generate, assert researchGenerateNote payload and system message.
});
```

- [ ] **Step 2: Verify tests fail for missing hook**

Run: `npm run test -- tests/use-research-control.test.tsx`

Expected: FAIL because `useResearchControl` does not exist.

- [ ] **Step 3: Extract research control hook**

Move `listenResearchProgress`, `researchRequestIdRef`, `researchProgress`, `researchRunning`, `researchPanelExpanded`, `researchDetailRef`, `generatingResearchNote`, `abortResearch`, `handleGenerateResearchNote`, and `handleExpandResearchDetail` into `useResearchControl`. Keep `research-focus` and `research-detail-panel` rendered by `AssistantTaskSurfaces`.

- [ ] **Step 4: Verify targeted tests pass**

Run: `npm run test -- tests/use-research-control.test.tsx tests/e2e/unified-assistant-contract.test.ts`

Expected: PASS.

### Task 4: Conversation Virtualization and Render Isolation

**Files:**

- Modify: `src/components/ai/ConversationSurface.tsx`
- Modify: `src/components/ai/AiMessageList.tsx`
- Test: `tests/assistant-panel-performance-contract.test.ts`
- Test: `tests/editor-performance-regression.test.tsx`

- [ ] **Step 1: Write failing virtualization contract**

```ts
it("virtualizes long assistant conversations with @tanstack/react-virtual", () => {
  const surface =
    read("src/components/ai/ConversationSurface.tsx") +
    read("src/components/ai/AiMessageList.tsx");
  expect(surface).toContain("useVirtualizer");
  expect(surface).toContain("@tanstack/react-virtual");
});
```

- [ ] **Step 2: Verify contract fails before implementation**

Run: `npm run test -- tests/assistant-panel-performance-contract.test.ts`

Expected: FAIL because message rendering is not virtualized.

- [ ] **Step 3: Add virtualized message list**

Use `useVirtualizer` for long lists, keep citation click, selection, retract, quote-to-input, research expansion, and streaming content behavior. Preserve `ConversationSurface` memoization.

- [ ] **Step 4: Add render isolation contract**

Assert artifact modules are not imported by conversation/composer modules and `UnifiedAssistantPanel.impl.tsx` remains below the current checkpoint threshold.

- [ ] **Step 5: Verify targeted tests pass**

Run: `npm run test -- tests/assistant-panel-performance-contract.test.ts tests/editor-performance-regression.test.tsx`

Expected: PASS.

### Task 5: App Shell Hook Extraction

**Files:**

- Create hooks under `src/hooks/` for AI sidecar bridge, editor actions, lifecycle persistence, and overlay/action dispatch.
- Modify: `src/App.impl.tsx`
- Test: existing App, close, tab manager, overlay, command palette, and editor action tests.

- [ ] **Step 1: Write failing line-count and behavior contracts**

Add a contract that `src/App.impl.tsx` stays below the next checkpoint and tests active/inactive flush order, before-close dirty tab save, and overlay keyboard semantics.

- [ ] **Step 2: Extract hooks one boundary at a time**

Move only one responsibility per commit-sized step: AI sidecar bridge, editor actions, lifecycle persistence, overlay dispatch.

- [ ] **Step 3: Verify App shell tests**

Run: `npm run test -- tests/app-close-version-guard.test.ts tests/use-tauri-close-save.test.ts tests/use-tab-manager-activate-tab.test.ts tests/command-palette.test.ts tests/overlay-manager.test.ts`

Expected: PASS.

### Task 6: Rust AI Runtime Module Splits

**Files:**

- Split `src-tauri/src/ai_runtime/model_gateway_impl.rs`
- Split `src-tauri/src/ai_runtime/skills_impl.rs`
- Split `src-tauri/src/ai_runtime/tool_dispatch_impl.rs`
- Split `src-tauri/src/ai_runtime/tool_catalog_impl.rs`
- Split `src-tauri/src/ai_runtime/retrieval_broker_impl.rs`
- Add tests under `src-tauri/src/ai_runtime/**` or `src-tauri/tests/**`

- [ ] **Step 1: Add red tests for public contract preservation**

Cover tool message repair, body JSON shape, usage parsing, stream event parsing, skills metadata scan, prompt injection ordering, resource escape rejection, tool catalog/dispatch consistency, and retrieval ranking/dedup behavior.

- [ ] **Step 2: Split one Rust impl file at a time**

Create child modules exactly as named in `next.md`. Re-export from the existing public facade modules so old imports remain valid.

- [ ] **Step 3: Verify Rust targeted tests after each split**

Run: `cargo test --manifest-path src-tauri/Cargo.toml <module_or_test_name>`

Expected: PASS after each split.

### Task 7: Performance Cache and Benchmark Work

**Files:**

- Modify: `src-tauri/src/ai_runtime/context_cache.rs`
- Modify: `src-tauri/src/ai_runtime/packet_cache.rs`
- Modify related context assembly/send paths.
- Modify: `src-tauri/benches/ai_benchmarks.rs`

- [ ] **Step 1: Add regression tests for cache keys and invalidation**

Assert keys include scene, note path, query, scope, provider context strategy, and input budget. Assert runtime clear, AI cache clear, and reindex invalidate relevant entries.

- [ ] **Step 2: Add benchmark cases**

Add large skill prompt injection, long tool history message/body construction, mixed retrieval rank/dedup, large text guardrails, and context cache hit/miss benches.

- [ ] **Step 3: Verify performance gates**

Run: `cargo bench --manifest-path src-tauri/Cargo.toml --bench ai_benchmarks`

Expected: benchmark suite runs without failures and reports the new cases.

### Task 8: Documentation and Final Gates

**Files:**

- Modify: `ARCHITECTURE.md`
- Modify: `docs/README.md`
- Modify: `docs/audits/2026-06-11-project-review-v1.1.0.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Sync docs to implementation facts**

Document AI runtime module boundaries, frontend AI panel hook boundaries, context cache lifecycle, and `next.md` as a temporary execution checklist.

- [ ] **Step 2: Run full verification**

Run:

```bash
npm run lint
npm run format:check
npm run typecheck
npm run test
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run audit:rust
```

Expected: all commands exit 0.

- [ ] **Step 3: Record residual risks**

If any full gate cannot run in the local environment, record the exact command, failure output, and why it is environmental rather than code-related.
