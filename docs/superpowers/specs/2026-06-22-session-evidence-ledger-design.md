# Session Evidence Ledger Design

## Problem

Iris currently treats evidence packets as per-answer UI payloads. That is too weak for long-running AI conversations:

- switching sessions can lose the visible evidence package;
- citations such as `[C1]` are ambiguous if each answer owns its own packet;
- a detail view would be fragile if it were the data source;
- inserted document content should use normal Iris links rather than AI-only audit markers.

The feature needs a session-level evidence ledger that survives conversation reloads, assigns stable citation labels, and keeps AI conversation audit state separate from normal Markdown document links.

## Goals

- Maintain one append-only evidence ledger per AI session.
- Give every evidence item a session-unique citation label such as `[C1]`; labels are never recycled.
- Reuse an existing label when a new turn cites the same evidence.
- Restore the ledger when switching back to a session.
- Keep local notes as the source of truth; do not store note text snapshots in SQLite.
- Store web evidence metadata only; do not store fetched page body, excerpt, or snapshot.
- Show a lightweight evidence package in the AI panel and a richer read-only detail tab on demand.
- Convert AI citations to normal Iris Markdown links when copying or inserting AI replies into documents.
- Preserve invalid citations in AI text while marking them as unresolved.

## Non-Goals

- No v1 span highlight or scroll-to-range when opening a local source.
- No v1 evidence detail export or copy action.
- No v1 persistence of open evidence detail tabs across app restart.
- No reverse lookup from inserted document links back to the AI session ledger.
- No web page archival, full-text cache, screenshot cache, or excerpt storage.

## Data Model

### `session_evidence`

The table belongs to application runtime state, not the Markdown note index. It is deleted with its owning `sessions` row.

Required columns:

- `id`: integer primary key.
- `session_id`: owning AI session id, foreign key with cascade delete.
- `citation_index`: integer session-local index, unique per session.
- `citation_label`: text such as `[C1]`, unique per session.
- `packet_key`: stable de-duplication key, unique per session.
- `message_seq_first`: first message sequence that introduced this evidence.
- `source_type`: `local` or `web`.
- `title`: display title.
- `source_path`: vault-relative local Markdown path, nullable for web.
- `source_span_start`: optional local UTF-8 span start.
- `source_span_end`: optional local UTF-8 span end.
- `heading_path`: optional JSON/text heading path.
- `content_hash`: optional hash of the source content at registration time.
- `retrieval_reason`: why this evidence was selected.
- `score`: optional retrieval score.
- `confidence`: optional confidence enum or numeric score.
- `url`: original web URL, nullable for local.
- `normalized_url`: canonical web de-duplication URL, nullable for local.
- `domain`: web domain.
- `retrieved_at`: web retrieval/search timestamp.
- `search_backend`: web search backend name.
- `source_rank`: web source rank.
- `failure_reason`: web retrieval/search failure reason.
- `retired_at`: nullable tombstone timestamp used when retract hides evidence without recycling citation numbers.
- `created_at`: insertion timestamp.

Forbidden columns:

- local note body;
- local note excerpt;
- web page body;
- web page excerpt;
- web snapshot or screenshot.

## Registration Rules

Evidence registration happens before the final answer is sent to the model for citation use, so the prompt can use session-stable labels.

For each new turn:

1. Build a packet key for every candidate evidence item.
2. De-duplicate against the current session ledger by `packet_key`.
3. Reuse the old label for duplicates.
4. Allocate the next `citation_index` for new items.
5. Never renumber old items.
6. Pass the session-stable evidence list into the final answer prompt.
7. After model output, validate citation labels:
   - known labels are clickable;
   - unknown labels remain visible and are marked unresolved;
   - Iris does not guess or rewrite unknown labels.

Local packet key priority:

1. `source_path + source_span_start + source_span_end + content_hash`
2. `source_path + heading_path + content_hash`
3. `source_path + heading_path`

Web packet key:

1. normalized URL;
2. original URL if normalization fails.

## Lifecycle

- Session switch: load `session_evidence` with messages and rebuild the AI panel ledger state.
- Session delete: cascade delete ledger rows.
- Session clear: delete messages and ledger rows for that session.
- Session expiration cleanup: delete ledger rows with their expired session.
- Message retract: mark evidence whose `message_seq_first` belongs only to the retracted suffix as retired; keep rows as tombstones so citation numbers are never recycled.
- File rename/move: update `session_evidence.source_path` for affected local evidence rows.
- File delete: keep rows; detail view shows `source_missing`.
- App restart: restore ledgers, but not open evidence detail tabs.

## UI Contract

### AI Evidence Package

The message-level evidence package affordance remains lightweight:

- grouped by local and web evidence;
- shows citation label, title, source type, and confidence;
- provides source opening for known evidence;
- includes a `详细` action that opens a temporary read-only evidence detail tab.

### Evidence Detail Tab

The evidence detail tab is a temporary artifact view:

- it is not persisted to the artifact localStorage snapshot;
- switching sessions does not close it;
- deleting the owning session closes it or marks it invalid;
- reopening after app restart requires opening it from the AI panel again.

The tab body behaves like a read-only Iris document:

```md
# 证据详情：<session title>

## [C1] <title>

来源：本地笔记
状态：来源未变化
路径：...

<live excerpt read from current .md when locatable>
```

Local evidence status:

- `source_unchanged`: path exists and content hash still matches.
- `source_changed`: path exists but hash no longer matches.
- `span_missing`: path exists but the saved span or heading cannot be located.
- `source_missing`: source file no longer exists.

Web evidence detail displays only URL metadata and the notice:

```md
外部网页，未保存正文快照。
```

## Copy And Insert Contract

Copying AI replies and inserting AI replies into documents must use the same transformation:

- local `[C1]` becomes `[[vault/relative/path]]` without `.md`;
- web `[C2]` becomes `[title](https://example.com)`;
- if web title is missing, use domain; if domain is missing, use URL;
- multiple adjacent citations are replaced individually;
- unknown citations stay as `[C99]` and trigger a user-visible warning;
- conversion skips fenced code blocks, inline code, existing Markdown links, and wiki-links.

After insertion, the document contains normal Iris links only. It does not retain AI audit metadata.

## Source Opening

- Clicking a known local citation in AI conversation or evidence detail opens the Iris document.
- Clicking a known web citation opens the original URL.
- Inserted document links use normal Iris Markdown behavior.
