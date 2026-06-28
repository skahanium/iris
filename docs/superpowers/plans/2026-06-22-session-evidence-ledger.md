# Session Evidence Ledger Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a session-level evidence ledger that restores across session switches, assigns stable `[C1]` labels for the whole AI session, exposes a read-only evidence detail tab, and converts AI citations into normal Iris Markdown links on copy/insert.
**Architecture:** Store evidence metadata in SQLite `session_evidence` rows owned by `sessions`; register evidence through backend session APIs before final answer generation; expose typed IPC list/detail/register commands; keep the AI panel ledger state separate from document Markdown links; render evidence details as a non-persistent artifact tab.
**Tech Stack:** Tauri 2.x, Rust, rusqlite/SQLite migrations, React 19, TypeScript, TipTap/ProseMirror, TailwindCSS + shadcn/ui, Vitest, Cargo tests.

---

## File Structure

- Create: `docs/superpowers/specs/2026-06-22-session-evidence-ledger-design.md`
- Create: `src-tauri/migrations/037_session_evidence.sql`
- Create: `src-tauri/migrations/037_session_evidence.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`
- Modify: `src-tauri/src/ai_runtime/session.rs`
- Create: `src-tauri/src/ai_runtime/session_evidence.rs`
- Modify: `src-tauri/src/ai_runtime/mod.rs`
- Modify: `src-tauri/src/commands/ai_commands.rs`
- Modify: `src-tauri/src/commands/file.rs`
- Modify: `src/types/ipc.ts`
- Modify: `src/lib/ipc.ts`
- Create: `src/lib/ai/evidence-citations.ts`
- Modify: `src/lib/ai/session-history.ts`
- Modify: `src/components/ai/hooks/useAssistantConversation.ts`
- Modify: `src/components/ai/AiMessageList.tsx`
- Modify: `src/components/ai/ContextPacketDrawer.tsx`
- Modify: `src/components/ai/UnifiedAssistantPanel.impl.tsx`
- Modify: `src/types/assistant-artifact.ts`
- Modify: `src/lib/assistant-artifact-tabs.ts`
- Modify: `src/hooks/useArtifactTabs.ts`
- Create: `src/components/ai/EvidenceDetailArtifact.tsx`
- Modify: `tests/session-history-evidence.test.ts`
- Create: `tests/session-evidence-ledger.test.ts`
- Create: `tests/evidence-citations.test.ts`
- Create: `tests/evidence-detail-artifact.test.tsx`
- Modify: `tests/use-assistant-conversation.test.tsx`
- Modify: `tests/assistant-artifact-tabs.test.ts`

## Execution Preflight

- [ ] Decide workspace isolation before code changes. Current `D:\Iris` contains unrelated uncommitted fixes from earlier AI work; do not mix new ledger changes into those files until the user confirms whether to continue in this workspace, commit the prior fixes, or create a fresh worktree.
- [ ] If using a new worktree, follow `superpowers:using-git-worktrees` and create a branch such as `codex/session-evidence-ledger`.
- [ ] Run baseline checks that are narrow enough to finish:

```bash
npm.cmd run test -- tests/session-history-evidence.test.ts tests/use-assistant-conversation.test.tsx
npm.cmd run typecheck
cargo test --manifest-path src-tauri/Cargo.toml session --lib
```

Expected: existing branch baseline should pass or expose pre-existing failures that must be recorded before implementation.

## Task 1: Add SQLite Migration For `session_evidence`

**Files:**

- Create: `src-tauri/migrations/037_session_evidence.sql`
- Create: `src-tauri/migrations/037_session_evidence.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`

- [ ] Add a failing Rust migration test in `src-tauri/src/storage/migrate.rs` that migrates an in-memory database and asserts:
  - `session_evidence` exists;
  - `session_id` references `sessions(id)` with `ON DELETE CASCADE`;
  - `(session_id, citation_label)` is unique;
  - `(session_id, packet_key)` is unique;
  - `retired_at` exists for tombstoning retracted evidence;
  - no forbidden body/excerpt/snapshot columns exist.
- [ ] Create `037_session_evidence.sql` with the schema from the spec.
- [ ] Create `037_session_evidence.down.sql` that drops only `session_evidence`.
- [ ] Register migration 037 in `migrate.rs` up/down order.
- [ ] Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml migrate --lib
```

Expected before implementation: FAIL because migration 037 is not registered. Expected after implementation: PASS.

## Task 2: Implement Backend Ledger Registration

**Files:**

- Create: `src-tauri/src/ai_runtime/session_evidence.rs`
- Modify: `src-tauri/src/ai_runtime/session.rs`
- Modify: `src-tauri/src/ai_runtime/mod.rs`

- [ ] Add failing Rust tests for:
  - first registration allocates `[C1]`, `[C2]`;
  - repeated local packet reuses `[C1]`;
  - repeated normalized web URL reuses the old label;
  - new evidence after duplicates gets `[C3]`;
  - labels are not recycled after message retract because retired rows remain as tombstones;
  - web evidence DTO has no body/excerpt field.
- [ ] Define backend structs:

```rust
pub(crate) struct SessionEvidenceRegisterPacket { /* typed metadata only */ }
pub(crate) struct SessionEvidenceItem { /* stored ledger row */ }
pub(crate) enum SessionEvidenceSourceType { Local, Web }
```

- [ ] Implement `packet_key_for_register_packet` with the local and web priority rules from the spec.
- [ ] Implement `register_session_evidence(conn, session_id, message_seq, packets)` in one transaction.
- [ ] Implement `list_session_evidence(conn, session_id)`.
- [ ] Implement `retire_evidence_first_introduced_at_or_after(conn, session_id, message_seq_cutoff)` for retract support without recycling labels.
- [ ] Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml session_evidence --lib
```

Expected before implementation: FAIL due missing module/API. Expected after implementation: PASS.

## Task 3: Wire Ledger Into Session Message Persistence

**Files:**

- Modify: `src-tauri/src/ai_runtime/session.rs`
- Modify: `src-tauri/src/commands/ai_commands.rs`
- Modify: `src/types/ipc.ts`
- Modify: `src/lib/ipc.ts`
- Modify: `src/lib/ai/session-history.ts`
- Modify: `src/components/ai/hooks/useAssistantConversation.ts`

- [ ] Add failing frontend tests in `tests/session-history-evidence.test.ts` and `tests/use-assistant-conversation.test.tsx` asserting that switching away and back restores the session ledger, not only per-message packets.
- [ ] Add typed IPC DTOs:

```ts
export interface SessionEvidence {
  /* mirrors backend metadata */
}
export interface SessionEvidenceDetail {
  /* detail-tab-ready view */
}
export interface SessionEvidenceRegisterPacket {
  /* metadata only */
}
```

- [ ] Add IPC wrappers in `src/lib/ipc.ts`:
  - `sessionEvidenceList(sessionId)`
  - `sessionEvidenceDetail(sessionId)`
  - `sessionEvidenceRegister(sessionId, messageSeq, packets)`
- [ ] Add matching `#[tauri::command]` functions in `ai_commands.rs`.
- [ ] Update session load to fetch messages and ledger together.
- [ ] Ensure final answer flow registers evidence before displaying citations.
- [ ] Run:

```bash
npm.cmd run test -- tests/session-history-evidence.test.ts tests/use-assistant-conversation.test.tsx
npm.cmd run typecheck
```

Expected before implementation: FAIL because no ledger IPC/load path exists. Expected after implementation: PASS.

## Task 4: Handle Lifecycle Updates

**Files:**

- Modify: `src-tauri/src/ai_runtime/session.rs`
- Modify: `src-tauri/src/ai_runtime/session_evidence.rs`
- Modify: `src-tauri/src/commands/file.rs`

- [ ] Add Rust tests for:
  - deleting a session cascades evidence rows;
  - clearing a session deletes evidence rows;
  - retracting messages deletes evidence first introduced by the retracted suffix;
  - local file rename updates `source_path`;
  - local folder rename updates descendant `source_path` values;
  - local file deletion keeps evidence rows.
- [ ] Wire session delete/clear paths to rely on cascade or explicit cleanup where needed.
- [ ] Wire message retract to `delete_evidence_first_introduced_after`.
- [ ] Extend file/folder rename command internals to cascade-update `session_evidence.source_path`.
- [ ] Do not delete evidence on file delete.
- [ ] Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml session_evidence --lib
cargo test --manifest-path src-tauri/Cargo.toml file_ops --lib
```

Expected before implementation: FAIL for lifecycle assertions. Expected after implementation: PASS.

## Task 5: Implement Citation Conversion Utility

**Files:**

- Create: `src/lib/ai/evidence-citations.ts`
- Create: `tests/evidence-citations.test.ts`

- [ ] Add failing Vitest cases for:
  - local `[C1]` converts to `[[folder/file]]`;
  - `.md` extension is removed;
  - web `[C2]` converts to `[Title](https://example.com)`;
  - missing web title falls back to domain and then URL;
  - adjacent `[C1][C2]` converts each citation;
  - unknown `[C99]` remains and is reported;
  - fenced code blocks are skipped;
  - inline code is skipped;
  - existing Markdown links are skipped;
  - wiki-links are skipped.
- [ ] Implement:

```ts
export function resolveCitationToEvidence(
  ref: string,
  ledger: SessionEvidence[],
): SessionEvidence | null;
export function replaceAiCitationsForDocument(
  markdown: string,
  ledger: SessionEvidence[],
): {
  markdown: string;
  missing: string[];
};
```

- [ ] Use a small Markdown-aware scanner rather than a global replace.
- [ ] Run:

```bash
npm.cmd run test -- tests/evidence-citations.test.ts
npm.cmd run typecheck
```

Expected before implementation: FAIL because utility does not exist. Expected after implementation: PASS.

## Task 6: Use Conversion For Copy And Insert

**Files:**

- Modify: `src/components/ai/hooks/useAssistantConversation.ts`
- Modify: `src/components/ai/AiMessageList.tsx`
- Modify: `src/components/ai/UnifiedAssistantPanel.impl.tsx`
- Modify: `tests/use-assistant-conversation.test.tsx`

- [ ] Add failing frontend tests proving copy and insert call the same conversion utility.
- [ ] Add missing-citation warning behavior when conversion reports unresolved citations.
- [ ] Ensure AI conversation display still shows raw `[C1]` labels.
- [ ] Ensure inserted document text receives normal Iris Markdown links only.
- [ ] Run:

```bash
npm.cmd run test -- tests/use-assistant-conversation.test.tsx tests/evidence-citations.test.ts
npm.cmd run typecheck
```

Expected before implementation: FAIL because copy/insert still use raw reply text. Expected after implementation: PASS.

## Task 7: Add Evidence Detail Temporary Artifact Tab

**Files:**

- Modify: `src/types/assistant-artifact.ts`
- Modify: `src/lib/assistant-artifact-tabs.ts`
- Modify: `src/hooks/useArtifactTabs.ts`
- Create: `src/components/ai/EvidenceDetailArtifact.tsx`
- Create: `tests/evidence-detail-artifact.test.tsx`
- Modify: `tests/assistant-artifact-tabs.test.ts`

- [ ] Add failing tests asserting:
  - evidence detail tab can be opened with a `sessionId`;
  - evidence detail tab is not written to localStorage snapshot;
  - switching sessions does not close an already open detail tab;
  - deleting the owning session closes or invalidates the tab;
  - detail body renders headings like `## [C1] Title`;
  - local evidence shows status labels;
  - web evidence shows URL metadata and no saved snapshot notice.
- [ ] Add a non-persistent artifact kind such as `session_evidence_detail`.
- [ ] Ensure localStorage serialization filters this kind.
- [ ] Implement a read-only document-like renderer with left outline generated from evidence headings.
- [ ] Use `sessionEvidenceDetail(sessionId)` as data source; do not store detail tab content in localStorage.
- [ ] Run:

```bash
npm.cmd run test -- tests/evidence-detail-artifact.test.tsx tests/assistant-artifact-tabs.test.ts
npm.cmd run typecheck
```

Expected before implementation: FAIL because tab kind/rendering does not exist. Expected after implementation: PASS.

## Task 8: Update Evidence Package Drawer

**Files:**

- Modify: `src/components/ai/ContextPacketDrawer.tsx`
- Modify: `src/components/ai/AiMessageList.tsx`
- Modify: `src/components/ai/UnifiedAssistantPanel.impl.tsx`
- Modify: `tests/session-history-evidence.test.ts`

- [ ] Add failing tests proving the drawer:
  - groups local and web evidence;
  - shows citation label, title, source type, and confidence;
  - includes `详细`;
  - opens local sources as Iris documents;
  - opens web sources as URLs.
- [ ] Replace per-message-only packet rendering with ledger-aware rendering.
- [ ] Keep the existing button affordance lightweight.
- [ ] Wire `详细` to open the temporary evidence detail tab.
- [ ] Run:

```bash
npm.cmd run test -- tests/session-history-evidence.test.ts tests/evidence-detail-artifact.test.tsx
npm.cmd run typecheck
```

Expected before implementation: FAIL because drawer is not ledger-aware. Expected after implementation: PASS.

## Task 9: Validate Prompt Citation Contract

**Files:**

- Modify: `src-tauri/src/ai_runtime/session_evidence.rs`
- Modify: `src-tauri/src/ai_harness/harness/run.rs`
- Modify: `src-tauri/src/commands/ai_commands.rs`
- Modify: `tests/harness-modernization-contract.test.ts`

- [ ] Add tests proving final prompt evidence labels are session-stable and unknown model citations are preserved.
- [ ] Feed the registered ledger labels into the final answer context.
- [ ] Add response validation that returns known and unknown citation refs to the frontend.
- [ ] Do not rewrite unknown citations.
- [ ] Run:

```bash
npm.cmd run test -- tests/harness-modernization-contract.test.ts
cargo test --manifest-path src-tauri/Cargo.toml session_evidence --lib
```

Expected before implementation: FAIL for missing final prompt ledger integration. Expected after implementation: PASS.

## Task 10: Final Verification

**Files:**

- All modified files.

- [ ] Run:

```bash
npm.cmd run test
```

- [ ] Run:

```bash
npm.cmd run format:check
```

- [ ] Run:

```bash
npm.cmd run typecheck
```

- [ ] Run:

```bash
npm.cmd run lint
```

- [ ] Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
```

- [ ] Run:

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

- [ ] Run targeted Rust tests for migration/session/file lifecycle:

```bash
cargo test --manifest-path src-tauri/Cargo.toml migrate --lib
cargo test --manifest-path src-tauri/Cargo.toml session_evidence --lib
cargo test --manifest-path src-tauri/Cargo.toml file_ops --lib
```

- [ ] If full `cargo test --manifest-path src-tauri/Cargo.toml` still hangs in existing `file_ops`, record the hang with the exact last test name instead of claiming full Rust test completion.
