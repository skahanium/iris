# Tech Debt P0-P2 Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve the P0-P2 items in `TECH_DEBT_REVIEW.md` with tests and without broad rewrites.

**Architecture:** Keep existing public APIs compatible while moving expensive work out of synchronous command paths, tightening IPC DTOs, and improving indexer hot paths. Dynamic AI tool JSON remains dynamic; stable command responses become typed.

**Tech Stack:** Tauri 2.x, Rust 1.80, tokio, rusqlite/SQLite migrations, React 19, TypeScript, Vitest, Cargo tests.

---

## Task 1: P0 Vault Switch And Folder Rename

**Files:**

- Modify: `src-tauri/src/commands/file.rs`
- Test: `src-tauri/src/commands/file.rs`

- [ ] Add tests proving `vault_runtime_cleanup_sql` is no longer used by `vault_set` and folder rename reindexes only affected paths.
- [ ] Replace automatic runtime table deletion with in-memory state clearing only.
- [ ] Move `vault_set` indexing into a spawned blocking task that emits progress events.
- [ ] Change `folder_rename` to reindex renamed subtree files plus cascade-modified source files, not the whole vault.
- [ ] Run `cargo test file_ops --lib` or the narrow Rust tests that compile this module.

## Task 2: P1 Single-Read Indexing And Hot Paths

**Files:**

- Modify: `src-tauri/src/indexer/chunker.rs`
- Modify: `src-tauri/src/indexer/wikilink.rs`
- Modify: `src-tauri/src/indexer/image_ref.rs`
- Modify: `src-tauri/src/indexer/frontmatter.rs`
- Modify: `src-tauri/src/indexer/scan.rs`
- Modify: `src-tauri/src/watcher/mod.rs`
- Modify: `src-tauri/src/commands/writing_commands.rs`
- Modify: `src-tauri/src/commands/organize_commands.rs`

- [ ] Add failing tests for long chunk splitting, regex extraction stability, and watcher single-read indexing helper behavior.
- [ ] Implement incremental char counting in `chunk_markdown`.
- [ ] Convert hot-path regex construction to `LazyLock<Regex>`.
- [ ] Reuse `index_file_from_content` in watcher, patch apply, and organize write paths.
- [ ] Batch tag inserts and preload wiki-link title/path maps where local and safe.
- [ ] Run targeted Rust tests for indexer modules.

## Task 3: P1 Migration And IPC Typing

**Files:**

- Create: `src-tauri/migrations/031_links_single_column_indexes.sql`
- Create: `src-tauri/migrations/031_links_single_column_indexes.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`
- Modify: `src-tauri/src/commands/ai_commands.rs`
- Modify: `src-tauri/src/commands/research_commands.rs`
- Modify: `src/types/ai.ts`
- Modify: `src/lib/ipc.ts`
- Test: `tests/runtime-contracts.test.ts`

- [ ] Add failing tests for migration 031 registration and IPC DTO field names.
- [ ] Register migration 031 in up/down order.
- [ ] Introduce named Rust response structs for stable AI/research command responses.
- [ ] Add `AiScene::parse_wire` helper and replace string-format enum parsing.
- [ ] Update TypeScript response interfaces only where stable field names are preserved.
- [ ] Run `npm run typecheck` and targeted Vitest contract tests.

## Task 4: P2 Policy, Security Hygiene, And Semantic Guardrails

**Files:**

- Modify: `src/lib/frontmatter.ts`
- Modify: `tests/frontmatter.test.ts`
- Modify: `src-tauri/src/credentials.rs`
- Modify: `src-tauri/src/commands/classified.rs`
- Modify: `src-tauri/src/embedding/engine.rs`
- Modify: `tests/runtime-contracts.test.ts`

- [ ] Add tests pinning frontend frontmatter subset behavior for complex YAML.
- [ ] Add tests that credential bundle serialization does not expose values outside the keyring marker path.
- [ ] Wrap cached API key values with zeroizing storage internally.
- [ ] Shorten classified password lifetime in backend command internals and keep frontend clearing tests green.
- [ ] Add/keep semantic fallback tests proving the 8000 chunk guard behavior is explicit.
- [ ] Run targeted TypeScript and Rust tests.

## Task 5: Final Verification

**Files:**

- All modified files.

- [ ] Run `npm run format:check`.
- [ ] Run `npm run lint`.
- [ ] Run `npm run typecheck`.
- [ ] Run `npm run test`.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --all-targets -- -D warnings`.
- [ ] Run `cargo test`.
- [ ] Update `TECH_DEBT_REVIEW.md` only if implementation changes the remaining-risk picture.
