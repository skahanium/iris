# AI 双域会话与涉密协作 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Iris AI 改造成普通显式上下文 + 涉密强绑定加密域的双域系统，并同时修复涉密编辑器等价性、快捷 AI、消息布局、流式性能和长文超时问题。

**Architecture:** 前端用 `AiDomain` 统一驱动 AI 面板、编辑器快捷 AI、会话历史、上下文和缓存生命周期；普通域继续使用现有 session 表，涉密域使用保险库加密 thread store 与解锁期内存缓存。后端把涉密会话、检索和执行路径从普通 SQLite/trace/session 中隔离出来；流式输出改为进展感知和轻渲染。

**Tech Stack:** Tauri 2.x, Rust, React 19, TypeScript, TipTap/ProseMirror, TailwindCSS + shadcn/ui, SQLite, encrypted classified vault files, Vitest, Cargo tests.

---

## Implementation Rule

这是一个单一总方案。下面的阶段只是施工依赖顺序，不是产品分期；所有阶段完成并通过验收后才算交付。不要合并一个“涉密 AI 半开放但历史/缓存未隔离”的中间状态。

## File Structure

Expected new files:

- `src/lib/ai-domain.ts`: 前端 AI domain 派生、domain guard、普通/涉密上下文判定。
- `src/hooks/useAiDomainRuntime.ts`: 前端普通/涉密会话、输入草稿、缓存清理、切文档恢复逻辑。
- `src-tauri/src/ai_runtime/classified_session.rs`: 保险库加密 AI thread 存取，不写普通 session 表。
- `src-tauri/src/ai_runtime/classified_retrieval.rs`: 解锁期内存涉密检索索引。
- `tests/ai-domain-routing.test.tsx`: 普通显式上下文与涉密自动域切换。
- `tests/classified-ai-session-store.test.ts`: 涉密历史加密存储 contract。
- `tests/classified-ai-runtime-lifecycle.test.tsx`: 前端涉密缓存与域切换清理。
- `tests/classified-editor-ai-parity.test.tsx`: 涉密编辑器 Markdown/目录/快捷 AI 等价性。
- `tests/ai-message-left-rail.test.tsx`: 左侧单轨消息操作布局。
- `tests/assistant-streaming-lifecycle.test.tsx`: 流式轻渲染、滚动跟随与不抢滚动。
- `src-tauri/tests/classified_ai_security.rs`: 后端涉密安全、普通 DB/trace 不泄露。

Expected modified files:

- `src/hooks/useWorkspaceAssistantRouting.ts`
- `src/hooks/useAppEditorActions.ts`
- `src/hooks/useEditorContextMenu.ts`
- `src/hooks/useInlineAi.ts`
- `src/components/layout/AppAiPanelSlot.tsx`
- `src/components/layout/AppEditorWorkspace.tsx`
- `src/components/ai/UnifiedAssistantPanel.impl.tsx`
- `src/components/ai/AssistantPanelHeader.tsx`
- `src/components/ai/SessionHistoryDropdown.tsx`
- `src/components/ai/ConversationSurface.tsx`
- `src/components/ai/AiMessageList.tsx`
- `src/components/ai/AiMessageBubble.tsx`
- `src/components/ai/AiMessageSelectionUi.tsx`
- `src/components/editor/TipTapEditor.tsx`
- `src/components/editor/EditorOutline.tsx`
- `src/styles/globals.css`
- `src/lib/ipc.ts`
- `src/types/ipc.ts`
- `src/types/ai.ts`
- `src-tauri/src/commands/ai_commands.rs`
- `src-tauri/src/commands/assistant_commands.rs`
- `src-tauri/src/commands/classified.rs`
- `src-tauri/src/ai_runtime/mod.rs`
- `src-tauri/src/ai_runtime/session.rs`
- `src-tauri/src/ai_harness/harness/run.rs`
- `src-tauri/src/ai_runtime/model_gateway/streaming.rs`
- `src-tauri/src/network/cert_pinning.rs`
- `tests/classified-vault-phase7.test.ts`
- `tests/assistant-hang-fixes-contract.test.ts`
- Existing message selection and process status tests as needed.

## Stage 0: Baseline And Contract Tests

**Purpose:** Lock the intended behavior before implementation. These tests should fail against the current code.

- [ ] Add `tests/ai-domain-routing.test.tsx`.
  - Assert normal document switching does not change AI note content unless a context reference is explicitly added.
  - Assert active `.classified/...` note with unlocked vault yields `domain: "classified"`.
  - Assert switching from classified note to normal note returns `domain: "normal"` and clears classified runtime state.
  - Assert media/artifact tabs never inherit classified permissions.

- [ ] Add `tests/classified-ai-session-store.test.ts`.
  - Read source files and assert classified AI session APIs do not call `SessionManager::ensure`, `session_list`, or ordinary `session_messages`.
  - Assert encrypted payload APIs depend on `classified_io` and `VaultKey`.
  - Assert thread filenames are uuid/hash based and do not include `.classified` paths or titles.

- [ ] Add `src-tauri/tests/classified_ai_security.rs`.
  - Unlock a test vault, save a classified AI thread, lock it, and assert load fails while locked.
  - Query ordinary `sessions` and `session_messages` tables and assert no classified message text, title, or path exists.
  - Assert classified trace/log-safe metadata contains no plaintext path or body.

- [ ] Add `tests/classified-editor-ai-parity.test.tsx`.
  - Render a classified editor surface and type `# 标题`.
  - Assert the first body node becomes heading without reopen.
  - Assert `EditorOutline` updates after editor `update`.
  - Assert right-click actions are present for classified editor when unlocked and route to classified AI handlers.

- [ ] Add `tests/ai-message-left-rail.test.tsx`.
  - Assert select/copy/retract controls are outside `.ai-message-bubble`.
  - Assert assistant rows do not render a right action rail.
  - Assert `.ai-message-body` text selection does not call message select.

- [ ] Add `tests/assistant-streaming-lifecycle.test.tsx`.
  - Assert token batches are throttled.
  - Assert user-scrolled-up state prevents forced scroll-to-bottom.
  - Assert returning to bottom resumes follow mode.

- [ ] Update `tests/assistant-hang-fixes-contract.test.ts`.
  - Replace the requirement “run_harness wraps the whole body in fixed 300 second timeout” with “run_harness enforces idle/stall timeout and abort polling”.
  - Keep the requirement that ordinary streaming abort remains available from the composer stop button.

- [ ] Run expected failing tests:

```bash
npm run test -- tests/ai-domain-routing.test.tsx tests/classified-editor-ai-parity.test.tsx tests/ai-message-left-rail.test.tsx tests/assistant-streaming-lifecycle.test.tsx tests/assistant-hang-fixes-contract.test.ts
cargo test --manifest-path src-tauri/Cargo.toml classified_ai --test classified_ai_security
```

Expected: FAIL because the new domain model, encrypted thread store, left rail layout, and progress-aware timeout do not exist yet.

## Stage 1: AI Domain Model And Frontend Runtime

**Purpose:** Make every AI surface domain-aware before enabling classified content.

- [ ] Create `src/lib/ai-domain.ts` with:

```ts
export type AiDomain = "normal" | "classified";

export interface AiDomainState {
  domain: AiDomain;
  normalActivePath: string | null;
  classifiedActivePath: string | null;
  classifiedUnlocked: boolean;
}

export function deriveAiDomainState(input: {
  activePath: string | null;
  activeNoteIsClassified: boolean;
  classifiedUnlocked: boolean;
  activeArtifactTab: unknown | null;
  activeMediaTab: unknown | null;
}): AiDomainState {
  const canUseClassified =
    input.activeNoteIsClassified &&
    input.classifiedUnlocked &&
    !input.activeArtifactTab &&
    !input.activeMediaTab &&
    input.activePath !== null;

  return {
    domain: canUseClassified ? "classified" : "normal",
    normalActivePath:
      !input.activeNoteIsClassified &&
      !input.activeArtifactTab &&
      !input.activeMediaTab
        ? input.activePath
        : null,
    classifiedActivePath: canUseClassified ? input.activePath : null,
    classifiedUnlocked: input.classifiedUnlocked,
  };
}

export function shouldAttachNormalCurrentDocument(input: {
  explicitContext: boolean;
  uiAction: "chat" | "editor_action" | "selection_quote" | "mention";
}): boolean {
  return input.explicitContext || input.uiAction !== "chat";
}
```

- [ ] Create `src/hooks/useAiDomainRuntime.ts`.
  - Keep separate ordinary and classified input drafts.
  - Keep separate selected message sets.
  - Track `classifiedThreadByPath`.
  - On domain switch from classified to normal: abort classified request, clear classified volatile cache, keep encrypted thread history.
  - On classified path switch: save current in-memory thread snapshot, load target path's latest encrypted thread summary.

- [ ] Modify `src/hooks/useWorkspaceAssistantRouting.ts`.
  - Replace `nonNoteSurfaceActive` with domain-aware logic.
  - For normal chat, return `assistantNotePath: null` unless context was explicitly attached.
  - For classified, return a classified request context instead of calling normal `getLiveMarkdown`/`getWritingContext`.

- [ ] Modify `src/components/layout/AppAiPanelSlot.tsx`.
  - Pass `aiDomain`, `classifiedPath`, and domain-aware insert handler to `UnifiedAssistantPanel`.
  - Do not pass classified content through `getNoteContent` used by normal AI.

- [ ] Update `src/types/ai.ts` with:

```ts
export type AiDomain = "normal" | "classified";

export type AiConversationRef =
  | { domain: "normal"; sessionId: number | null }
  | { domain: "classified"; threadId: string | null; documentPath: string };

export interface AssistantRequestContext {
  domain: AiDomain;
  notePath: string | null;
  contextReferences: ContextReference[];
  classifiedThreadId?: string | null;
}
```

- [ ] Run:

```bash
npm run test -- tests/ai-domain-routing.test.tsx
npm run typecheck
```

Expected: PASS.

## Stage 2: Encrypted Classified AI Thread Store

**Purpose:** Persist涉密 AI history safely under the unlocked vault, never in ordinary session tables.

- [ ] Create `src-tauri/src/ai_runtime/classified_session.rs`.
  - Use `require_unlocked` equivalent logic from `commands/classified.rs` or move shared helpers to a small internal module.
  - Use `classified_io::encrypt_cef` and `classified_io::decrypt_cef`.
  - Store encrypted thread files under `.classified/.iris-ai/sessions/`.
  - Store encrypted thread index under `.classified/.iris-ai/index.cef`.
  - Hide `.iris-ai` from `classified_files_inner`.

- [ ] Define Rust structs:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClassifiedAiThread {
    pub version: u32,
    pub thread_id: String,
    pub document_path: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<ClassifiedAiMessage>,
    pub evidence_packets: Vec<serde_json::Value>,
    pub token_usage: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClassifiedAiMessage {
    pub seq: i64,
    pub role: String,
    pub content: String,
    pub content_parts: Option<serde_json::Value>,
    pub tool_calls: Option<serde_json::Value>,
    pub created_at: String,
}
```

- [ ] Add IPC commands in `src-tauri/src/commands/ai_commands.rs` or a focused classified AI command module:
  - `classified_ai_thread_list(document_path: Option<String>)`
  - `classified_ai_thread_load(thread_id: String)`
  - `classified_ai_thread_save(thread: ClassifiedAiThread)`
  - `classified_ai_thread_delete(thread_id: String)`
  - `classified_ai_cache_clear()`

- [ ] Register commands in `src-tauri/src/lib.rs`.

- [ ] Add typed wrappers to `src/lib/ipc.ts` and DTOs to `src/types/ipc.ts`.

- [ ] Modify `src/components/ai/SessionHistoryDropdown.tsx`.
  - Add `domain` prop.
  - In normal domain call `sessionList`.
  - In classified domain call `classifiedAiThreadList`.
  - Use the same UI shape but separate data source.

- [ ] Modify `src/components/ai/hooks/useAssistantConversation.ts`.
  - Support `AiConversationRef`.
  - Normal domain keeps current session id behavior.
  - Classified domain saves and loads encrypted thread snapshots.
  - Retract in classified domain updates encrypted thread only.

- [ ] Run:

```bash
npm run test -- tests/classified-ai-session-store.test.ts tests/use-assistant-conversation.test.tsx tests/session-history-dropdown.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml classified_ai --test classified_ai_security
```

Expected: PASS.

## Stage 3: Classified Runtime Security And Cache Lifecycle

**Purpose:** Ensure涉密 prompt/output/cache/index/buffer cleanup is centralized and testable.

- [ ] Extend `useAiDomainRuntime` with a `clearClassifiedVolatileState(reason)` function.
  - Clear stream buffer.
  - Clear selected messages.
  - Clear selection/context menu snapshots.
  - Clear pending patches and writing artifacts.
  - Clear measured row/render caches owned by classified domain.
  - Clear classified retrieval index through IPC.

- [ ] Modify `src/hooks/useClassifiedVaultSession.ts`.
  - Call `onLocked` after lock and expose a stable event to clear classified AI runtime.
  - Ensure lock while classified request is active first aborts request.

- [ ] Modify `src/hooks/useAssistantLlmStream.ts`.
  - Make stream buffers domain-scoped.
  - Ignore late token events from a classified request after leaving classified domain.

- [ ] Modify trace/error paths.
  - In `src-tauri/src/ai_runtime/trace.rs` and assistant command error handling, redact classified path/content.
  - Store only domain, request id, status, token counts, tool names.

- [ ] Add security assertions to `src-tauri/tests/classified_ai_security.rs`.
  - Search ordinary DB tables for a sentinel classified phrase after save and after failed request.
  - Assert trace rows do not contain `.classified/`, document title, or sentinel body.

- [ ] Run:

```bash
npm run test -- tests/classified-ai-runtime-lifecycle.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml classified_ai --test classified_ai_security
```

Expected: PASS.

## Stage 4: Classified In-Memory Retrieval

**Purpose:** Provide local涉密 search without ordinary index/vector persistence.

- [ ] Create `src-tauri/src/ai_runtime/classified_retrieval.rs`.
  - Store an in-memory map keyed by vault identity/session, protected by `Mutex` or `RwLock`.
  - Index only while vault is unlocked.
  - Exclude `.classified/.iris-ai`.
  - Split Markdown into heading-aware chunks.
  - Rank with local lexical signals: query term hits, heading hits, current document boost, path/folder match, recency if available.

- [ ] Add IPC:
  - `classified_ai_context_search(query, current_document, scope_paths, limit)`
  - `classified_ai_retrieval_clear()`

- [ ] Add tests:
  - unlocked vault can search two encrypted classified docs.
  - locked vault search fails.
  - clear removes indexed chunks.
  - ordinary search still excludes `.classified`.
  - no remote embedding or ordinary vector table writes happen.

- [ ] Modify assistant context assembly.
  - Classified domain uses classified retrieval IPC/tool path.
  - Normal domain never calls classified retrieval.
  - Query “查涉密库/保险库/其他涉密文档” expands scope; ordinary chat stays current document only.

- [ ] Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml classified_retrieval --lib
npm run test -- tests/ai-domain-routing.test.tsx
```

Expected: PASS.

## Stage 5: Classified Editor Parity And Quick AI

**Purpose:** Make classified editing and AI actions match ordinary note editing.

- [ ] Modify `src/hooks/useEditorContextMenu.ts`.
  - Replace `if (locked) return` with a more precise check: locked editor cannot open edit menu, unlocked classified editor can open AI menu.
  - Include `domain` and `classifiedUnlocked` in `EditorActionContext`.

- [ ] Modify `src/hooks/useAppEditorActions.ts`.
  - Remove blanket `activeNoteIsClassified` rejections.
  - For classified active path, route inline AI, slash commands, send selection, and insert-to-editor through classified AI domain.
  - Keep ordinary route for normal documents.
  - Ensure `getWritingContext` returns classified context only to classified domain, never to normal AI.

- [ ] Modify `src/hooks/useInlineAi.ts`.
  - Accept `domain` and request context.
  - For classified domain, call classified assistant execution path and write result into current editor transaction.
  - Preserve undo stack and AI stream node behavior.

- [ ] Modify `src/components/editor/TipTapEditor.tsx` and `EditorOutline.tsx` only if tests show live heading/outline failure.
  - Ensure classified cache namespace does not bypass input rules.
  - Ensure outline effect re-subscribes when active editor instance changes.
  - Ensure `update` events from classified editor are not hidden by inactive surface wrappers.

- [ ] Modify CSS in `src/styles/globals.css`.
  - Use shared Ghost Spine reserve/inset for normal and classified editor surfaces.
  - Add classified AI color tokens using amber/gold, not red.

- [ ] Update `tests/classified-vault-phase7.test.ts`.
  - Replace “App never forwards classified note material into AI surfaces” with “App never forwards classified material into normal AI surfaces; classified AI uses classified domain only”.

- [ ] Run:

```bash
npm run test -- tests/classified-editor-ai-parity.test.tsx tests/classified-vault-phase7.test.ts tests/editor-performance-regression.test.tsx tests/outline-ghost-spine.test.tsx
npm run typecheck
```

Expected: PASS.

## Stage 6: AI Panel Visual Domain And Message Left Rail

**Purpose:** Make domain visible through color and recover message width.

- [ ] Modify `src/components/ai/UnifiedAssistantPanel.impl.tsx`.
  - Accept `aiDomain`.
  - Add root data attribute: `data-ai-domain="normal|classified"`.
  - Keep normal and classified input drafts separate through `useAiDomainRuntime`.

- [ ] Modify `src/components/ai/AssistantPanelHeader.tsx` and `SessionHistoryDropdown.tsx`.
  - In classified domain, use amber/gold focus/hover/selected styling.
  - Do not add persistent explanatory text labels.

- [ ] Modify `src/components/ai/AiMessageList.tsx`.
  - Replace assistant row grid `grid-cols-[1.75rem_minmax(0,1fr)_3.5rem]` with a left rail plus content layout.
  - Put select/copy/retract in one vertical rail.
  - Remove right action rail entirely.
  - Use stable callback maps as currently done.

- [ ] Modify `src/components/ai/AiMessageBubble.tsx`.
  - Keep message-level controls out of bubble.
  - Keep selected ring light.
  - Ensure `.ai-message-body` remains `select-text`.

- [ ] Modify `src/components/ai/AiMessageSelectionUi.tsx`.
  - Keep selection snapshot model.
  - Ensure classified domain selection snapshots are cleared on domain exit.
  - Ensure `Cmd/Ctrl+C` does not intercept editor/composer selection.

- [ ] Add/adjust tests:
  - `tests/ai-message-left-rail.test.tsx`
  - existing `tests/ai-message-selection-behavior.test.tsx`

- [ ] Run:

```bash
npm run test -- tests/ai-message-left-rail.test.tsx tests/ai-message-selection-behavior.test.tsx
npm run typecheck
```

Expected: PASS.

## Stage 7: Streaming Rendering, Scroll State, And Progress-Aware Timeout

**Purpose:** Make long AI output smooth and prevent false 300s abort.

- [ ] Modify `src/hooks/useAssistantLlmStream.ts`.
  - Keep 50ms minimum flush interval or adjust based on measurement.
  - Add domain/request id guard so stale classified/normal tokens do not cross domains.
  - Emit progress timestamps for each token batch.

- [ ] Modify `src/hooks/useStreamingContent.ts`.
  - Keep light streaming render.
  - Prefer paragraph boundary updates and final full render.
  - Avoid full Markdown reparse on every token.

- [ ] Add a scroll follow state to `ConversationSurface` or `AiMessageList`.
  - `following`: user is near bottom, auto-scroll.
  - `detached`: user scrolled up, preserve scrollTop.
  - Resume `following` when user scrolls back near bottom.

- [ ] Modify virtual list behavior in `AiMessageList`.
  - Do not let hover controls change row width.
  - Re-measure only changed row during streaming.
  - Keep content-aware estimate.

- [ ] Modify `src-tauri/src/ai_harness/harness/run.rs`.
  - Replace fixed whole-run `tokio::time::timeout(Duration::from_secs(300), ...)` with progress-aware idle timeout.
  - Track last progress time from model token, tool event, status event, or explicit heartbeat.
  - Preserve max rounds and task budget protections.

- [ ] Modify `src-tauri/src/network/cert_pinning.rs` and `model_gateway/streaming.rs`.
  - Provide a streaming HTTP client/builder without total 300s timeout.
  - Retain `read_timeout(60s)` or equivalent per-read stall detection.
  - Retain `ABORT_POLL_INTERVAL` around `stream.next()`.

- [ ] Update `tests/assistant-hang-fixes-contract.test.ts`.
  - Assert progress-aware idle timeout exists.
  - Assert fixed 300s wall-clock truncation does not exist on streaming path.
  - Assert abort poll still exists.

- [ ] Run:

```bash
npm run test -- tests/assistant-streaming-lifecycle.test.tsx tests/assistant-hang-fixes-contract.test.ts tests/assistant-panel-performance-contract.test.ts tests/ai-message-list-scroll-perf.test.ts
cargo test --manifest-path src-tauri/Cargo.toml harness --lib
```

Expected: PASS.

## Stage 8: End-To-End Integration And Safety Review

**Purpose:** Verify the total design works as one system, not as isolated patches.

- [ ] Add integration test covering:
  - open normal doc, chat in normal AI;
  - switch to another normal doc, normal AI context does not silently change;
  - unlock classified vault and open classified doc;
  - classified AI auto-enters amber domain and loads document thread;
  - run selected-text rewrite in classified editor;
  - switch to another classified doc and restore its latest thread;
  - switch back to normal doc and verify ordinary history has no classified entry.

- [ ] Add security scan test:
  - Use sentinel classified phrase.
  - Run classified AI save/rewrite/search.
  - Search ordinary SQLite tables, frontend persisted localStorage keys used by assistant, and trace rows for sentinel/path/title.
  - Expected: no matches outside encrypted classified files.

- [ ] Add manual QA checklist to PR description or implementation notes:
  - AI panel normal/classified visual switch.
  - AI message text selection + right click copy.
  - Long streaming story generation over 300 seconds with continuous tokens.
  - User scrolls up during streaming; panel does not yank to bottom.
  - Classified vault lock clears visible classified AI runtime.

- [ ] Run final verification:

```bash
npm run typecheck
npm run lint
npm run format:check
npm run test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Expected: all commands PASS. If full `cargo clippy` or `cargo test` is too slow for the environment, record the exact command attempted, output, and narrowed successful command; do not claim full completion without running it.

## Acceptance Checklist

- [ ] Normal AI only receives document content through explicit context.
- [ ] Classified AI auto-enters/exits based on active classified document and vault unlock state.
- [ ] Classified session history is encrypted under `.classified`, not ordinary SQLite.
- [ ] Ordinary history cannot list or load classified threads.
- [ ] Classified retrieval uses only unlock-period memory and clears on lock/domain exit.
- [ ] Classified editor Markdown heading input and outline live refresh match normal editor.
- [ ] Classified right-click/inline/slash AI actions work in the same locations as normal editor actions.
- [ ] Message controls use left single rail and do not reduce right-side content width.
- [ ] AI message partial text selection, right-click copy/quote, and `Cmd/Ctrl+C` remain reliable.
- [ ] Streaming output uses light render and stable scroll behavior.
- [ ] Continuous long output is not killed by fixed 300s deadline.
- [ ] Idle/stalled output still times out and manual stop still works.
- [ ] Logs, traces, ordinary DB, ordinary cache, and ordinary history contain no classified plaintext.
