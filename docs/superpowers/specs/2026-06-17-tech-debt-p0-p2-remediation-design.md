# Tech Debt P0-P2 Remediation Design

## Goal

Resolve the actionable P0-P2 items from `TECH_DEBT_REVIEW.md` without changing the Markdown authority model, adding non-compliant dependencies, or weakening existing security boundaries.

## Scope

This design covers:

- Vault switching and folder rename indexing behavior.
- Runtime data retention across vault switches.
- Indexer hot paths: duplicate reads, chunking, regex compilation, tag/wiki-link SQL, link indexes.
- IPC response typing for high-frequency AI/research commands and scene parsing.
- Frontmatter policy alignment between frontend and backend.
- Sensitive runtime data hygiene: retention controls, credential cache zeroization, password lifecycle.
- Semantic-search and embedding risk reduction through measurable fallback behavior and conservative tests.

This design does not introduce a new Markdown parser dependency in this pass. M1 is resolved by creating a shared contract corpus and explicit parser-boundary tests, leaving a future AST parser adoption as a separately evaluated dependency decision.

## Architecture

### Background Vault Indexing

`vault_set` will stop performing all indexing inline. It will switch vault state, restart the watcher, emit a `vault:index_started` event, and spawn a background blocking task that indexes files one at a time. Completion and failure are reported with progress events.

The frontend wrapper remains `vaultSet(path): Promise<void>` so existing callers do not need to migrate immediately. The user-visible behavior improves because the command returns after setup, while indexing continues in the background.

### Runtime Vault Retention

Runtime tables already have `vault_id` columns from migration 030. `vault_set` will no longer delete runtime rows unconditionally. Instead, it will clear only in-memory AI state and rely on SQL queries to scope by the active vault where that behavior already exists or can be safely introduced. A separate cleanup command can be added later; this pass removes the destructive automatic cleanup.

### Targeted Reindexing

`folder_rename` will reindex only:

- Files under the renamed folder.
- Source files whose wiki-links were rewritten by cascade logic.

It will still prune stale indexes after the rename, but it will not collect and reindex the entire vault.

### Single-Read Indexing

`index_file_from_content` already provides the desired API for content already in memory. Watcher, patch apply, organize write, and organize rename paths will use content/hash-aware variants where possible so they avoid read-hash-read cycles.

### Indexer Hot Path Cleanup

Regexes in wiki-link, image, frontmatter/body-tag extraction, and link-context construction will use `LazyLock<Regex>`. `chunk_markdown` will maintain character counts incrementally and split by char boundaries in one pass. Tag and wiki-link indexing will keep behavior stable while reducing repeated SQL where straightforward.

### IPC Typing

Dynamic tool schemas can keep `serde_json::Value`. Stable command responses should use named structs:

- `AiSendMessageResponse`
- `ToolConfirmResponse`
- `ResearchExecuteResponse`
- `ResearchStatusResponse`
- `KnowledgeReindexResponse`
- `AiToolInfo`

Existing JSON field names must remain compatible with TypeScript interfaces. Scene parsing will move to one helper so error handling and wire names stay consistent.

### Frontmatter Policy

Frontend parsing will be documented and tested as an Iris UI subset parser, not a complete YAML parser. Backend remains the canonical index parser. UI helpers should not silently claim support for YAML features they do not parse. Tests will pin unsupported complex YAML behavior.

### Sensitive Runtime Data Hygiene

Credential bundle values will use a zeroizing wrapper internally and scrub cache replacements. Classified password command bodies will use `Zeroizing<String>` where Tauri command support allows; if not, the shortest viable lifecycle will be enforced in command internals and frontend tests will confirm password inputs clear on success and failure. Session/deposit retention cleanup will be exposed through explicit command-level behavior rather than implicit vault switching.

### Semantic Search

This pass will not replace fastembed concurrency internals. It will add tests and guards around cosine fallback limits and clarify the behavior path. Any model-pool or sqlite-vec rework needs benchmark evidence first.

## Error Handling

- Background index failures are logged and emitted as progress failure events without reverting the vault switch.
- Targeted folder rename reindex failures remain non-fatal per file, matching current behavior, but the renamed subtree itself is always attempted.
- IPC DTO parsing errors use clear `AppError::msg` messages with the invalid scene value omitted from logs if it could contain user input.
- Credential and classified password changes must never log secret values.

## Testing Strategy

- Rust unit tests for chunker, regex behavior, targeted folder rename source logic, vault runtime cleanup behavior, and migration registration.
- Rust command/source contract tests where full Tauri app integration is too heavy.
- TypeScript contract tests for IPC response field names and password input clearing.
- Existing format, lint, typecheck, Rust fmt/clippy/test, and frontend tests remain final gates.

## Implementation Order

1. P0: vault background indexing, runtime retention, targeted folder rename.
2. P1: single-read indexing, regex/chunker, links indexes, IPC DTOs.
3. P2: frontmatter policy tests, SQL batching, sensitive runtime hygiene, semantic fallback tests.

Each task must add or update tests before production code.
