# AI Conversation Rendering Stability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make assistant conversations session-isolated, stream without rebuilding completed Markdown, and keep virtualized long answers readable and stable.

**Architecture:** A single projection hook turns Run and presentation state into one assistant-row patch. Message/session identity drives React and virtualizer lifetime. Finalized Markdown renders synchronously from the existing renderer cache; streaming Markdown appends only lexer-proven stable blocks and rewrites an isolated tail. The virtual list measures actual rows through one batched observer and delegates follow/detach decisions to a reading-anchor controller.

**Tech Stack:** React 19, TypeScript, Vitest/jsdom, `marked`, `@tanstack/react-virtual`, existing Markdown contract renderer.

## Global Constraints

- Preserve `.md` files as the authoritative note format; this work does not modify note content.
- Do not introduce dependencies; remove `morphdom` and `@types/morphdom` after their replacement is covered.
- Keep the existing typed IPC boundary unchanged.
- Use strict TypeScript; no `any`; preserve existing AI visible-text sanitization and trusted-HTML handling.
- Run targeted tests during each task, then lint, format check, typecheck, and the relevant complete frontend tests before handoff.

---

### Task 1: Establish session and message identities

**Files:**

- Create: `src/lib/ai-message-identity.ts`
- Test: `tests/ai-message-identity.test.ts`
- Modify: `src/components/ai/UnifiedAssistantPanel.impl.tsx`, `src/components/ai/AiMessageList.tsx`, `src/components/ai/ConversationSurface.tsx`

**Interfaces:**

- Produces `assistantSessionIdentity(session): string` and `assistantMessageIdentity(message, index): string`.
- `assistantMessageIdentity` selects `runId`, then `clientRequestId`, then `seq`, then `index`; role and turn ID disambiguate same-run rows.

- [ ] **Step 1: Write identity tests before implementation.** Assert each priority case and assert that session changes yield a different root key.
- [ ] **Step 2: Run `npm run test -- tests/ai-message-identity.test.ts` and confirm it fails because the module is absent.**
- [ ] **Step 3: Implement the two pure identity functions and key `ConversationSurface` from the session identity.**
- [ ] **Step 4: Pass identity keys to `useVirtualizer({ getItemKey })` and use them as rendered row keys.**
- [ ] **Step 5: Run the identity and virtual-list component tests; confirm historical-session DOM cannot be reused.**

### Task 2: Make state projection single-writer and row-local

**Files:**

- Create: `src/components/ai/hooks/useAssistantConversationProjection.ts`
- Create: `tests/use-assistant-conversation-projection.test.tsx`
- Modify: `src/components/ai/hooks/useAssistantConversation.ts`, `src/lib/ai-payload-store.ts`, `src/components/ai/UnifiedAssistantPanel.impl.tsx`
- Delete: `src/components/ai/hooks/useAssistantRunTranscript.ts`, `src/components/ai/hooks/useAssistantPresentationPlayback.ts`

**Interfaces:**

- `useAssistantConversation` exposes `patchAssistantMessage(runId, patch)` which copies only the matching row.
- Projection consumes `run`, `presentation`, `session`, `messages`, `patchAssistantMessage`, and UI state setters; its dedupe key is `runId:lastSeq:transientRevision:presentationSeq`.

- [ ] **Step 1: Add failing hook tests for duplicate-event idempotence, presentation ownership, citation hydration, and preservation of untouched row references.**
- [ ] **Step 2: Run the new test and confirm the missing unified hook fails.**
- [ ] **Step 3: Add the row-local patch API and make `compactChatLinesForState` return the previous array when every row reference is unchanged.**
- [ ] **Step 4: Move the durable/presentation decision and completion/failure handling into the single projection hook; use it from the panel.**
- [ ] **Step 5: Delete the two former writer hooks and their obsolete tests; run projection, payload-store, and Run rendering tests.**

### Task 3: Separate finalized and streaming Markdown renderers

**Files:**

- Create: `src/lib/streaming-markdown-splitter.ts`, `tests/streaming-markdown-splitter.test.ts`
- Create: `src/components/ai/StreamingMessageBody.tsx`, `tests/streaming-message-body.test.tsx`
- Modify: `src/components/ai/FinalizedMessageBody.tsx`, `src/components/ai/AiMessageBubble.tsx`, `src/hooks/useMarkdownRenderWorker.ts`
- Delete: `src/components/ai/StableMarkdownHtml.tsx`, `tests/stable-markdown-html.test.tsx`, `tests/stable-markdown-html-identity.test.tsx`
- Modify: `package.json`, `package-lock.json`

**Interfaces:**

- `splitStreamingMarkdown(content)` returns `{ stableMarkdown, tailMarkdown, stableBlockCount }`.
- `StreamingMessageBody` owns a `data-streaming-tail` element; it only appends newly stable rendered blocks and updates that tail.

- [ ] **Step 1: Write splitter tests for ordinary paragraphs, closed/open code fences, list/quote/table termination, headings, and horizontal rules.**
- [ ] **Step 2: Run the splitter test; confirm RED.**
- [ ] **Step 3: Implement conservative lexer-based stabilization: any non-final token is stable; final text/list/quote/table requires a terminating boundary; code requires a closed fence.**
- [ ] **Step 4: Write and run a failing component test proving a completed block keeps DOM identity while only `data-streaming-tail` changes.**
- [ ] **Step 5: Implement synchronous `FinalizedMessageBody`, stable-block streaming rendering, and select the renderer by the actual message streaming state. Do not route finalized messages through the worker.**
- [ ] **Step 6: Remove morphdom and its packages, update tests/contracts, and run all Markdown renderer tests.**

### Task 4: Stabilize virtual measurements and citation rows

**Files:**

- Modify: `src/components/ai/AiMessageList.tsx`, `src/components/ai/AiMessageBubble.tsx`
- Modify: `tests/ai-message-list-scroll-perf.test.ts`, `tests/ai-message-list-real-virtualizer-regression.test.tsx`, `tests/assistant-markdown-stream-contract.test.tsx`

**Interfaces:**

- Virtual rows include `{ type: "message", messageIndex }` and `{ type: "citations", messageIndex }`.
- `estimateSize` uses cached/static estimates; a streaming message has a fixed provisional estimate, and batched `ResizeObserver` measurements are the only actual-height truth.

- [ ] **Step 1: Write failing tests that require an independent citation virtual row and prohibit `content.length` from calculating streaming-row height.**
- [ ] **Step 2: Run the focused virtual-list tests; confirm RED.**
- [ ] **Step 3: Implement typed citation rows, stable identity keys, fixed streaming estimates, and observer-based measurement cleanup for unmounted nodes.**
- [ ] **Step 4: Render `AssistantCitationFooter` in its own row after finalization, rather than inside the measured message body.**
- [ ] **Step 5: Run virtualizer regression and contract tests; confirm citations no longer mutate the message row's layout identity.**

### Task 5: Add reading-anchor control and finish verification

**Files:**

- Create: `src/components/ai/hooks/useConversationReadingAnchor.ts`, `tests/use-conversation-reading-anchor.test.tsx`
- Modify: `src/components/ai/AiMessageList.tsx`
- Modify: `tests/ai-message-list-scroll-perf.test.ts`

**Interfaces:**

- The hook consumes the scroll viewport and streaming-tail observation signal and returns `{ following, returnToLatest }`.
- It keeps short content bottom-aligned; on long content it targets `tailTop - viewportHeight * 0.60`; upward wheel/touch/scroll detaches; reduced motion avoids smooth scroll.

- [ ] **Step 1: Write failing tests for short-answer bottom alignment, 60% long-tail target, user detachment, and return-to-latest.**
- [ ] **Step 2: Run the reading-anchor test; confirm RED.**
- [ ] **Step 3: Implement layout-effect writes and tail observation, then show an accessible “回到最新” control only while detached.**
- [ ] **Step 4: Run the focused AI conversation suite, then `npm run lint`, `npm run format:check`, `npm run typecheck`, and `npm run test`.**
- [ ] **Step 5: Manually verify a normal stream, fenced code/list/table stream, session switching, rapid scroll, final citations, themes, reduced motion, and a 100+ message session in the desktop app. Record any environment limitation honestly.**
