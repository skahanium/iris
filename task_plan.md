# Task Plan: Iris Document Open Runtime Planning

## Goal

Create a comprehensive, evidence-led implementation plan for making Iris document startup, open, re-open, and tab switching feel instant and consistent across Welcome, Quick Open, Vault Navigator, wiki links, AI source links, and tabs.

## Current Phase

Phase 3: Review gate and user decision confirmation

## Phases

### Phase 1: Requirements and Discovery

- [x] Capture user intent and constraints
- [x] Inspect current staged changes touching document-open performance
- [x] Inspect current frontend open/preload/cache/loading pipeline
- [x] Inspect backend file_read/workspace_list/index interactions
- [x] Identify decisions that require user confirmation
- **Status:** completed

### Phase 2: Comprehensive Plan Authoring

- [x] Define target architecture and invariants
- [x] Define file-by-file responsibilities
- [x] Write a full implementation plan with TDD steps
- [x] Include verification commands and acceptance scenarios
- **Status:** completed

### Phase 3: Review Gate

- [x] Self-review the plan for gaps, empty-work instructions, and contradictions
- [ ] Present key product/architecture decisions to the user
- [ ] Incorporate user feedback into the plan
- **Status:** in_progress

## Formal Plan

- `docs/superpowers/plans/2026-06-25-document-open-runtime-overhaul.md`

## Key Decisions For User Review

1. Mounted editor retention default: active + pending + dirty/saving + 8 additional clean ready surfaces.
2. Cross-restart preload default: persist session metadata only; no Markdown body or TipTap HTML.
3. Prepared content default: memory-only parse/HTML caches.
4. Background work priority default: foreground opens and hot tab activation preempt warmup/indexing.
5. Loading UX default: one workspace-level loading surface; row-level `正在打开` is secondary.
6. Latency budgets default: hot <=16ms, warm <=50ms, cold loading <=100ms, cold first editor frame <=1000ms for 50KB Markdown.

## Decisions Made

| Decision                                    | Rationale                                                                                                                           |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Do not create a worktree yet                | AGENTS.md forbids creating a worktree without approval. This turn is planning-only.                                                 |
| Avoid apply_patch                           | User-provided AGENTS.md says apply_patch is rejected by the Windows sandbox helper. Planning files are written without apply_patch. |
| Treat this as one cohesive runtime overhaul | User requested the deepest, most comprehensive plan and said not to split into product phases.                                      |
| Do not run tests in this turn               | This turn produced a plan only; tests are specified in the formal plan and should run during implementation.                        |

## Errors Encountered

| Error                                                                        | Attempt | Resolution                                                                                |
| ---------------------------------------------------------------------------- | ------- | ----------------------------------------------------------------------------------------- |
| Reading planning-with-files skill was denied inside sandbox                  | 1       | Re-ran the read with explicit escalation because the skill file is outside the workspace. |
| Windows rejected one-shot plan write with `CreateProcessAsUserW failed: 206` | 1       | Rewrote the plan in smaller PowerShell `Set-Content`/`Add-Content` chunks.                |

## Notes

- Existing staged changes touch document-open performance files; do not overwrite them without understanding them.
- Current user preference: discuss important unresolved product/architecture choices instead of deciding silently.
- Mechanical plan scan found no red-flag empty-work instructions after the self-review edit.
