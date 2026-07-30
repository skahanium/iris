# Agent Panel Startup Performance Implementation Plan

> **For agentic workers:** Execute each task with test-first verification in the current workspace; no worktree is created for this user-authorized fix.

**Goal:** Make the default-visible Agent panel reach an interactive first frame without blocking application startup or long-history rendering.

**Architecture:** Keep the Agent feature in a separate Vite chunk, but start loading that chunk while the pre-vault screen is visible. Move optional MCP binding discovery behind the first paint. Use one shared Markdown Worker for both streaming and finalized assistant messages, while retaining a synchronous fallback only when Workers are unavailable or fail.

**Tech Stack:** React 19, Vite, TypeScript, Tauri 2, Vitest.

## Global Constraints

- Preserve the lazy Agent-panel boundary; do not move the full AI feature into the eager application bundle.
- Do not change tool authorization, session persistence, classified-domain isolation, or Markdown output semantics.
- Do not add runtime dependencies.
- All user-visible fallback text remains Chinese.

### Task 1: Preload the default-visible Agent panel

**Files:**

- Modify: `src/components/layout/AppAiPanelSlot.tsx`
- Modify: `src/App.impl.tsx`
- Create: `src/lib/preload-assistant-panel.ts`
- Modify: `tests/app-ai-panel-lazy-contract.test.ts`

- [x] Write a failing source-contract test requiring a named panel preload function while retaining `lazy(() => import(...))`.
- [x] Run the focused test and confirm it fails because the preload API does not exist.
- [x] Keep the dynamic import lazy, add `preloadAssistantPanel`, and invoke it on the first animation frame while the Vault gate is visible.
- [x] Run the focused test and confirm it passes.

### Task 2: Keep optional MCP discovery out of the first frame

**Files:**

- Modify: `src/components/ai/UnifiedAssistantPanel.impl.tsx`
- Modify: `tests/unified-assistant-panel-startup-performance.test.ts`

- [x] Write a failing source-contract test requiring normal-domain MCP binding discovery to be scheduled after first paint and cancellable on unmount.
- [x] Run the focused test and confirm it fails.
- [x] Implement the scheduled discovery without changing the frozen per-Run authorization snapshot logic.
- [x] Run the focused test and confirm it passes.

### Task 3: Render finalized history through one shared Markdown Worker

**Files:**

- Modify: `src/hooks/useMarkdownRenderWorker.ts`
- Modify: `src/components/ai/AiMessageBubble.tsx`
- Modify: `tests/use-markdown-render-worker.test.tsx`
- Modify: `tests/ai-message-worker-pending.test.tsx`

- [x] Write failing tests proving finalized messages dispatch to the Worker and show a lightweight pending state instead of synchronously parsing Markdown.
- [x] Run the focused tests and confirm they fail against the streaming-only Worker behavior.
- [x] Convert worker ownership to a shared request broker; use it for finalized and streaming content, preserve bounded streaming input, and retain the synchronous error fallback.
- [x] Run focused tests plus the existing stream/virtualizer regressions.

### Task 4: Document and verify

**Files:**

- Modify: `docs/design-system.md`
- Modify: `ROADMAP.md`

- [x] Update the Agent-surface rendering policy and current roadmap fact.
- [x] Run formatter, lint, typecheck, all frontend tests, Rust checks, and dependency audits; the Rust advisory fetch remains externally unavailable after an escalated retry, while `npm audit` reports zero vulnerabilities.
