# Findings: Iris Document Open Runtime Planning

## Requirements

- Make Iris feel fast and smooth when opening and switching documents; speed is a core product reason for using Rust.
- Cover all entry points: Welcome recent notes, Quick Open, Vault Navigator/file tree, tab switching, repeated open of existing documents, wiki/source links, and related settings/startup behavior.
- Eliminate inconsistent loading behavior such as Welcome showing only "Opening...", some cold opens showing skeletons, repeated opens stalling, and tab switches feeling delayed.
- Build the deepest and most comprehensive plan, not a narrow patch list.
- Keep decisions collaborative; ask the user about consequential trade-offs.
- Respect project constraints from AGENTS.md: no apply_patch, no unapproved worktree, Tauri/Rust/React/TipTap/Tailwind/shadcn stack, no unsafe shortcuts, test-first implementation.

## Research Findings

- Existing warm-open pieces already exist: `usePreparedNoteOpener`, `document-open-runtime`, `editor-html-cache`, `DocumentOpenLoadingSurface`, and open traces with hot/warm budgets.
- Current prepared-open cache is memory-only and keyed through anonymized request keys/signatures; it reads note content, parses frontmatter/body, ingests TipTap HTML, and stores HTML in an in-memory editor cache.
- Quick Open and Vault Navigator prepare only a small number of visible candidates. Welcome recent notes prepare on hover/focus and through `useHomeRecentNotes`.
- `useTabManager` owns tabs, active path, pending note open, in-memory markdown cache, file lock cache, and first-frame commit staging.
- `AppEditorWorkspace` keeps editor surfaces by path and gates visibility until `onFirstFrameReady`; it has a loading watchdog and minimum loading duration.
- `TipTapEditor` has already been changed in the staged diff to async ingest via `ingestMarkdownForEditorAsync`, delayed first-frame notification until content is ready, and cache writes by digest/namespace.
- `workspace_list` prefers SQLite index and falls back to vault scan only if index query fails. Quick Open/file tree should not normally scan disk on every open.
- Backend `file_read` uses `tokio::task::spawn_blocking` for disk read/decrypt/lock query, so single-note read is not the obvious main-thread blocker.
- Startup indexing is background from the user's perspective, but it can still compete for disk/CPU with document opens and workspace list refreshes.
- Existing tests already cover some behaviors: no re-read for open tab activation, loading surface until first frame, no warm prepared hidden editor mounts, quick-open visible preparation, and cache namespace isolation.
- Current staged changes include deleting `fix-plan-doc-loading-performance.md` and modifying `TipTapEditor.tsx`, `AppEditorWorkspace.tsx`, `useHomeWorkspaceTransitions.ts`, and `useTabManager.ts`.

## Technical Decisions

| Decision                                               | Rationale                                                                                                    |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------ |
| Model the solution as a single "document open runtime" | The current behavior is fragmented across hooks/components; a named runtime gives entry points one contract. |
| Use measurable latency budgets                         | The issue is perceived responsiveness; the plan must define hot/warm/cold budgets and trace them.            |
| Preserve Markdown as source of truth                   | Preload/cache must not create another authoritative content store.                                           |
| Separate visible-open priority from background work    | Startup indexing and speculative warming should yield to the document the user is opening.                   |

## Issues Encountered

| Issue                                                | Resolution                                                               |
| ---------------------------------------------------- | ------------------------------------------------------------------------ |
| Planning skill file initially blocked by sandbox     | Escalated the read and captured the error in task_plan.md.               |
| Existing staged edits are already in the target area | Treat them as current working state and plan around them; do not revert. |

## Resources

- `src/hooks/useTabManager.ts`: tab state, pending open, read/persist/activate behavior.
- `src/hooks/usePreparedNoteOpener.ts`: warm preparation API used by workspace transitions.
- `src/lib/document-open-runtime.ts`: prepared note cache, tracing, hot/warm budgets.
- `src/components/layout/AppEditorWorkspace.tsx`: editor surface retention and loading gate.
- `src/components/editor/TipTapEditor.tsx`: TipTap initialization, async ingest, editor HTML cache.
- `src/components/file/QuickOpen.tsx`: Quick Open visible-result warming.
- `src/components/file/VaultNavigator.tsx`: file tree/folder visible warming.
- `src/components/layout/WelcomeEmpty.tsx`: recent notes and current Opening text behavior.
- `src-tauri/src/commands/file.rs`: `file_read`, vault indexing startup task, index rescan.
- `src-tauri/src/commands/media.rs`: `workspace_list` index-first listing.
- `tests/note-open-preparation.test.ts`, `tests/document-open-first-frame.test.tsx`, `tests/use-tab-manager-activate-tab.test.ts`, `tests/quick-open-performance.test.tsx`.

## Open Questions for User

- Is memory usage allowed to rise noticeably to keep every open editor mounted, or should we cap retained surfaces?
- Is cross-restart document HTML cache acceptable if stored as derived runtime cache outside Markdown, or should all preloading remain memory-only for now?
- During heavy startup indexing, should Iris pause/throttle indexing immediately when the user opens a note, even if search/index freshness completes later?

## Additional Findings - Persistence and Restore

- `useAppPersistenceLifecycle` flushes active/inactive tabs before tab leave/app close and updates editor HTML cache from the live editor after active-tab persistence.
- No existing ordinary note-tab snapshot restore was found in the frontend search; artifact tabs use localStorage, but note tabs are runtime-only.
- Cross-restart startup preloading would therefore require a new workspace session snapshot for note tab paths, active path, title, locked/dirty metadata, and derived cache metadata.
- `persistBeforeLeaveRef` waits for `editorRef` for active tabs; this is correct for dirty active content, but it is too expensive for clean hot tab switches and should be bypassed when a tab is known clean.

## Formal Plan Summary

- Plan path: `docs/superpowers/plans/2026-06-25-document-open-runtime-overhaul.md`.
- Plan shape: 12 implementation tasks, each with tests, expected results, implementation snippets, and commit guidance.
- Core architecture: one source-aware, priority-aware document-open runtime; hot mounted tab activation bypasses disk and visible loading; startup warms metadata-selected candidates; foreground opens preempt speculative work and indexing.
- User review decisions are captured at the top of the formal plan with defaults instead of hidden unilateral choices.
