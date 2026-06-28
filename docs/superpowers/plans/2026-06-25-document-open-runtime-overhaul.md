# Document Open Runtime Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Iris document startup, opening, reopening, and tab switching feel immediate and consistent by replacing fragmented cold-load behavior with one prioritized document-open runtime.

**Architecture:** All visible document opens flow through a single runtime request shape with priority, source, cache namespace, file signature, and trace metadata. The frontend keeps ready editor surfaces hot under a bounded retention policy, prepares likely documents in the background, and lets foreground opens preempt warmup and indexing work. The backend exposes cheap file signatures and a foreground-open activity scope so indexing can yield without making Markdown, TipTap HTML, or SQLite caches authoritative over `.md` files.

**Tech Stack:** Tauri 2.x, Rust, React 19, TipTap/ProseMirror, TypeScript, Vitest, SQLite. No new dependencies.

---

## User Review Decisions Before Execution

This plan is executable with these defaults, but they are the places where product taste matters and should be reviewed with the user before implementation starts.

1. **Mounted editor retention:** Keep the active editor, the pending target editor, all dirty/saving editors, and up to 8 additional ready editor surfaces mounted. Evict least-recent clean ready surfaces above the cap.
2. **Cross-restart preload:** Persist only workspace session metadata: vault id, open note paths, active path, display title, locked flag, and timestamps. Do not persist Markdown body or TipTap HTML across restarts.
3. **Prepared content storage:** Keep prepared Markdown parse output and editor HTML memory-only. A persistent derived HTML cache would need separate approval because it stores derived note content outside the `.md` source of truth.
4. **Background work priority:** Foreground opens and hot tab activation always preempt warmup and indexing. Indexing yields in short bounded slices while a foreground open is active.
5. **Loading UX:** Use one document loading surface for visible opens. Entry rows, Welcome rows, Quick Open rows, and file-tree rows must not show `Opening...` or `正在打开`; all real open progress belongs to the workspace loading surface.
6. **Latency budgets:** Hot mounted tab activation commits within 16ms and does not call `fileRead`. Warm prepared opens show a ready editor within 50ms after selection. Cold opens show a stable loading surface within 100ms and first editor frame within 1000ms for a 50KB Markdown note on a normal development machine.

If the user changes any default, update the constants and the tests named in this plan before writing implementation code.

## Current Findings To Preserve

- `src/hooks/useTabManager.ts` is the tab state boundary. It already has `pendingNoteOpen`, markdown cache, editor HTML cache coordination, and recent staged changes to avoid persistence work for clean hot tab switches.
- `src/lib/document-open-runtime.ts` already reads via `fileRead`, parses Markdown with `parseNoteForEditor`, prepares TipTap HTML with `ingestMarkdownForEditorAsync`, and owns warm/hot budget tracing. It should become the single runtime core rather than staying a helper used by only some entrances.
- `src/hooks/usePreparedNoteOpener.ts`, `src/components/file/QuickOpen.tsx`, `src/components/file/VaultNavigator.tsx`, and `src/components/layout/WelcomeEmpty.tsx` already prepare visible or hovered documents. The plan keeps these affordances but routes them through one scheduler.
- `src/components/layout/AppEditorWorkspace.tsx` already keeps path-stable editor surfaces and has a `DocumentOpenLoadingSurface`. It needs bounded retention and a stricter rule: ready mounted target tabs never show loading.
- `src/components/editor/TipTapEditor.tsx` already moved toward async ingestion and first-frame gating. The plan formalizes that as a contract for all opens.
- `src/hooks/useAppPersistenceLifecycle.ts` persists active and dirty inactive tabs. It needs a registry path that can persist dirty hidden editors without blocking clean tab activation.
- `src-tauri/src/commands/file.rs` performs `file_read` in `spawn_blocking`; `vault_set` starts a background index task. Foreground document opens currently have no way to tell background indexing to yield.
- `src-tauri/src/app.rs` owns `AppState`; `src-tauri/src/lib.rs` registers commands; `src/lib/ipc.ts` is the only allowed frontend IPC wrapper boundary.

## Runtime Invariants

- Every visible open from Welcome, Quick Open, file tree, link navigation, recent notes, and tab restore uses `DocumentOpenRequest`.
- Foreground requests outrank hot, warm, and background requests.
- Activating an already mounted ready clean tab never reads disk, never waits for persistence, and never displays the loading surface.
- Dirty mounted editors remain mounted until saved or until a full Markdown snapshot has been captured in memory for persistence.
- Prepared caches are invalidated by path, namespace, lock state, and file signature. Cached parse/HTML is never treated as more authoritative than the current `.md` file.
- Traces and logs use anonymized request ids, source names, durations, and cache hit states. They never include note paths, titles, Markdown, frontmatter, prompts, selections, API keys, or decrypted content.
- Background warmup and indexing may lag; visible document opening may not wait for them.

## File Map

- Create `src/lib/document-open-scheduler.ts`: pure priority scheduler with coalescing, cancellation, and deterministic tests.
- Modify `src/lib/document-open-runtime.ts`: accept source/priority/signature requests, use scheduler, expose warm/open/consume APIs, keep compatibility wrappers only where tests need a transition seam.
- Keep `src/lib/note-open-preparation.ts`: re-export runtime APIs for existing imports until all call sites are migrated.
- Create `src/lib/workspace-session-snapshot.ts`: local workspace session metadata persistence with no note body or HTML.
- Modify `src/types/ipc.ts`: add `FileSignatureResult` and foreground-open command result types.
- Modify `src/lib/ipc.ts`: add typed wrappers for `file_signature`, `document_open_begin`, and `document_open_end`.
- Modify `src/hooks/usePreparedNoteOpener.ts`: issue source-aware runtime requests and consume prepared records through the scheduler.
- Modify `src/hooks/usePreparedWorkspaceTransitions.ts`: own startup warmup, cache clearing, and foreground-open scope composition.
- Modify `src/hooks/useTabManager.ts`: make tab activation hot-path safe, integrate prepared requests, update dirty editor persistence behavior.
- Modify `src/components/layout/AppEditorWorkspace.tsx`: add bounded ready-surface retention and loading-surface rules.
- Modify `src/components/editor/TipTapEditor.tsx`: keep async first-frame contract and expose readiness/markdown snapshot callbacks needed by hidden editor persistence.
- Modify `src/hooks/useAppPersistenceLifecycle.ts`: use editor registry snapshots before falling back to active-editor waiting.
- Modify `src/components/file/QuickOpen.tsx`, `src/components/file/VaultNavigator.tsx`, and `src/components/layout/WelcomeEmpty.tsx`: pass open source and priority; remove row-level opening text in favor of the workspace loading surface.
- Modify `src-tauri/src/app.rs`: add foreground document-open activity counter.
- Modify `src-tauri/src/commands/file.rs`: add file signature command and bounded index-yield helper.
- Modify `src-tauri/src/lib.rs`: register new commands.
- Modify `src-tauri/src/indexer/scan.rs` only if manual `index_rescan` needs the same yield hook as `vault_set` background indexing.
- Modify `docs/ops/performance-guide.md`: document budgets and manual performance checks.
- Create `docs/testing/document-open-runtime-manual-checklist.md`: repeatable UI verification checklist.

### Task 1: Add Scheduler Contract Tests

**Files:**

- Create: `tests/document-open-scheduler.test.ts`
- No production files modified in this task.

- [ ] **Step 1: Write the failing scheduler tests**

Create `tests/document-open-scheduler.test.ts` with this content:

```ts
import { describe, expect, it, vi } from "vitest";

import {
  DocumentOpenScheduler,
  type DocumentOpenJob,
} from "../src/lib/document-open-scheduler";

function job(
  key: string,
  priority: DocumentOpenJob<string>["priority"],
  order: string[],
): DocumentOpenJob<string> {
  return {
    key,
    namespace: "vault-a",
    path: `${key}.md`,
    source: "test",
    priority,
    run: async () => {
      order.push(key);
      return key;
    },
  };
}

describe("DocumentOpenScheduler", () => {
  it("runs foreground work before queued warm work", async () => {
    const order: string[] = [];
    const scheduler = new DocumentOpenScheduler({ maxConcurrent: 1 });

    const warm = scheduler.enqueue(job("warm", "warm", order));
    const foreground = scheduler.enqueue(
      job("foreground", "foreground", order),
    );

    await expect(foreground.promise).resolves.toBe("foreground");
    await expect(warm.promise).resolves.toBe("warm");
    expect(order).toEqual(["foreground", "warm"]);
  });

  it("coalesces queued jobs with the same key", async () => {
    const run = vi.fn(async () => "ready");
    const scheduler = new DocumentOpenScheduler({ maxConcurrent: 1 });

    const first = scheduler.enqueue({
      key: "same",
      namespace: "vault-a",
      path: "same.md",
      source: "quick-open",
      priority: "warm",
      run,
    });
    const second = scheduler.enqueue({
      key: "same",
      namespace: "vault-a",
      path: "same.md",
      source: "file-tree",
      priority: "foreground",
      run,
    });

    await expect(second.promise).resolves.toBe("ready");
    await expect(first.promise).resolves.toBe("ready");
    expect(first.promise).toBe(second.promise);
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("cancels queued speculative work without cancelling foreground work", async () => {
    const order: string[] = [];
    const scheduler = new DocumentOpenScheduler({ maxConcurrent: 1 });
    const background = scheduler.enqueue(
      job("background", "background", order),
    );
    const foreground = scheduler.enqueue(
      job("foreground", "foreground", order),
    );

    background.cancel();

    await expect(background.promise).rejects.toMatchObject({
      name: "AbortError",
    });
    await expect(foreground.promise).resolves.toBe("foreground");
    expect(order).toEqual(["foreground"]);
  });
});
```

- [ ] **Step 2: Run the failing test**

Run: `npm run test -- tests/document-open-scheduler.test.ts`

Expected: FAIL because `src/lib/document-open-scheduler.ts` does not exist.

- [ ] **Step 3: Commit the failing contract test**

```bash
git add tests/document-open-scheduler.test.ts
git commit -m "test(editor): 覆盖文档打开调度器契约"
```

### Task 2: Implement The Pure Document Open Scheduler

**Files:**

- Create: `src/lib/document-open-scheduler.ts`
- Test: `tests/document-open-scheduler.test.ts`

- [ ] **Step 1: Create the scheduler implementation**

Create `src/lib/document-open-scheduler.ts` with this content:

```ts
export type DocumentOpenPriority = "foreground" | "hot" | "warm" | "background";

export type NoteOpenSource =
  | "welcome"
  | "quick-open"
  | "file-tree"
  | "tab"
  | "link"
  | "startup"
  | "test";

export interface DocumentOpenJob<T> {
  key: string;
  namespace: string;
  path: string;
  source: NoteOpenSource;
  priority: DocumentOpenPriority;
  run: (signal: AbortSignal) => Promise<T>;
}

export interface EnqueuedDocumentOpen<T> {
  promise: Promise<T>;
  cancel: () => void;
}

interface QueueEntry<T> {
  sequence: number;
  job: DocumentOpenJob<T>;
  controller: AbortController;
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
}

const PRIORITY_RANK: Record<DocumentOpenPriority, number> = {
  foreground: 0,
  hot: 1,
  warm: 2,
  background: 3,
};

function abortError(): Error {
  const error = new Error("Document open job cancelled");
  error.name = "AbortError";
  return error;
}

function outranks(
  next: DocumentOpenPriority,
  current: DocumentOpenPriority,
): boolean {
  return PRIORITY_RANK[next] < PRIORITY_RANK[current];
}

export class DocumentOpenScheduler {
  private readonly maxConcurrent: number;
  private nextSequence = 0;
  private pumpQueued = false;
  private readonly pending: QueueEntry<unknown>[] = [];
  private readonly queuedByKey = new Map<string, QueueEntry<unknown>>();
  private readonly runningByKey = new Map<string, Promise<unknown>>();

  constructor(options: { maxConcurrent?: number } = {}) {
    this.maxConcurrent = Math.max(1, options.maxConcurrent ?? 2);
  }

  enqueue<T>(job: DocumentOpenJob<T>): EnqueuedDocumentOpen<T> {
    const running = this.runningByKey.get(job.key);
    if (running) {
      return {
        promise: running as Promise<T>,
        cancel: () => undefined,
      };
    }

    const queued = this.queuedByKey.get(job.key);
    if (queued) {
      if (outranks(job.priority, queued.job.priority)) {
        queued.job = {
          ...queued.job,
          priority: job.priority,
          source: job.source,
        };
        this.sortPending();
      }
      return {
        promise: queued.promise as Promise<T>,
        cancel: () => this.cancelEntry(queued),
      };
    }

    const controller = new AbortController();
    let resolve!: (value: T) => void;
    let reject!: (error: unknown) => void;
    const promise = new Promise<T>((innerResolve, innerReject) => {
      resolve = innerResolve;
      reject = innerReject;
    });

    const entry: QueueEntry<T> = {
      sequence: this.nextSequence,
      job,
      controller,
      promise,
      resolve,
      reject,
    };
    this.nextSequence += 1;

    this.pending.push(entry as QueueEntry<unknown>);
    this.queuedByKey.set(job.key, entry as QueueEntry<unknown>);
    this.sortPending();
    this.queuePump();

    return {
      promise,
      cancel: () => this.cancelEntry(entry as QueueEntry<unknown>),
    };
  }

  private cancelEntry(entry: QueueEntry<unknown>): void {
    if (!this.queuedByKey.has(entry.job.key)) {
      return;
    }
    entry.controller.abort();
    this.queuedByKey.delete(entry.job.key);
    const index = this.pending.indexOf(entry);
    if (index >= 0) {
      this.pending.splice(index, 1);
    }
    entry.reject(abortError());
  }

  private queuePump(): void {
    if (this.pumpQueued) {
      return;
    }
    this.pumpQueued = true;
    queueMicrotask(() => {
      this.pumpQueued = false;
      this.pump();
    });
  }

  private pump(): void {
    while (
      this.runningByKey.size < this.maxConcurrent &&
      this.pending.length > 0
    ) {
      const entry = this.pending.shift();
      if (!entry) {
        return;
      }
      this.queuedByKey.delete(entry.job.key);
      if (entry.controller.signal.aborted) {
        entry.reject(abortError());
        continue;
      }
      const running = entry.job
        .run(entry.controller.signal)
        .then((value) => {
          entry.resolve(value);
          return value;
        })
        .catch((error) => {
          entry.reject(error);
          throw error;
        })
        .finally(() => {
          this.runningByKey.delete(entry.job.key);
          this.queuePump();
        });
      this.runningByKey.set(entry.job.key, running);
      void running.catch(() => undefined);
    }
  }

  private sortPending(): void {
    this.pending.sort((a, b) => {
      const priority =
        PRIORITY_RANK[a.job.priority] - PRIORITY_RANK[b.job.priority];
      return priority === 0 ? a.sequence - b.sequence : priority;
    });
  }
}
```

- [ ] **Step 2: Run the scheduler test**

Run: `npm run test -- tests/document-open-scheduler.test.ts`

Expected: PASS.

- [ ] **Step 3: Commit scheduler implementation**

```bash
git add src/lib/document-open-scheduler.ts tests/document-open-scheduler.test.ts
git commit -m "feat(editor): 添加文档打开优先级调度器"
```

### Task 3: Expand Runtime Request Contracts

**Files:**

- Modify: `src/lib/document-open-runtime.ts`
- Modify: `src/lib/note-open-preparation.ts`
- Test: `tests/note-open-preparation.test.ts`

- [ ] **Step 1: Add request-shape tests before changing runtime code**

Append these tests to `tests/note-open-preparation.test.ts` near the existing preparation-cache tests:

```ts
it("keeps prepared records isolated by namespace and file signature", async () => {
  const first = await prepareNoteOpen({
    namespace: "vault-a",
    path: "same.md",
    source: "quick-open",
    priority: "warm",
    signature: { contentHash: "hash-a", byteLength: 12, modifiedMs: 100 },
  });
  const second = await prepareNoteOpen({
    namespace: "vault-a",
    path: "same.md",
    source: "quick-open",
    priority: "warm",
    signature: { contentHash: "hash-b", byteLength: 14, modifiedMs: 200 },
  });

  expect(second).not.toBe(first);
});

it("records source and priority without exposing the note path in trace output", async () => {
  const traces: string[] = [];
  const restore = setDocumentOpenTraceSinkForTest((event) => {
    traces.push(JSON.stringify(event));
  });

  try {
    await prepareNoteOpen({
      namespace: "vault-a",
      path: "private-folder/private-note.md",
      source: "welcome",
      priority: "foreground",
      signature: { contentHash: "trace-hash", byteLength: 21, modifiedMs: 300 },
    });
  } finally {
    restore();
  }

  expect(traces.join("\n")).toContain("welcome");
  expect(traces.join("\n")).toContain("foreground");
  expect(traces.join("\n")).not.toContain("private-folder");
  expect(traces.join("\n")).not.toContain("private-note");
});
```

- [ ] **Step 2: Run the focused runtime tests and confirm the expected failure**

Run: `npm run test -- tests/note-open-preparation.test.ts`

Expected: FAIL because `signature`, `priority`, `source`, and `setDocumentOpenTraceSinkForTest` are not yet supported by the exported runtime API.

- [ ] **Step 3: Add runtime types and trace sink**

In `src/lib/document-open-runtime.ts`, add these exports near the existing type declarations:

```ts
import {
  DocumentOpenScheduler,
  type DocumentOpenPriority,
  type NoteOpenSource,
} from "@/lib/document-open-scheduler";

export type { DocumentOpenPriority, NoteOpenSource };

export interface FileSignature {
  contentHash: string;
  byteLength: number;
  modifiedMs: number | null;
}

export interface DocumentOpenRequest {
  namespace?: string;
  path: string;
  source?: NoteOpenSource;
  priority?: DocumentOpenPriority;
  title?: string;
  isLocked?: boolean;
  signature?: FileSignature;
  allowClassified?: boolean;
}

export interface DocumentOpenTraceEvent {
  requestId: string;
  source: NoteOpenSource;
  priority: DocumentOpenPriority;
  cacheState: "hit" | "miss" | "stale";
  durationMs: number;
}

type DocumentOpenTraceSink = (event: DocumentOpenTraceEvent) => void;

let documentOpenTraceSink: DocumentOpenTraceSink | null = null;

export function setDocumentOpenTraceSinkForTest(
  sink: DocumentOpenTraceSink,
): () => void {
  documentOpenTraceSink = sink;
  return () => {
    if (documentOpenTraceSink === sink) {
      documentOpenTraceSink = null;
    }
  };
}
```

- [ ] **Step 4: Replace cache-key construction with signature-aware keys**

In `src/lib/document-open-runtime.ts`, replace the existing prepared cache key helper with this function, keeping the existing namespace default value if one already exists in the file:

```ts
const DEFAULT_DOCUMENT_OPEN_NAMESPACE = "default";

function documentOpenNamespace(request: DocumentOpenRequest): string {
  return request.namespace ?? DEFAULT_DOCUMENT_OPEN_NAMESPACE;
}

function signatureKey(signature: FileSignature | undefined): string {
  if (!signature) {
    return "unknown";
  }
  return [
    signature.contentHash,
    signature.byteLength,
    signature.modifiedMs ?? "none",
  ].join("|");
}

function preparedNoteKey(request: DocumentOpenRequest): string {
  return [
    documentOpenNamespace(request),
    request.path,
    request.isLocked === true ? "locked" : "plain",
    signatureKey(request.signature),
  ].join("\0");
}
```

- [ ] **Step 5: Emit anonymized trace events**

Add this helper in `src/lib/document-open-runtime.ts` and call it from the success path and stale-cache path in `prepareNoteOpen`:

```ts
function emitDocumentOpenTrace(
  request: Required<Pick<DocumentOpenRequest, "source" | "priority">>,
  cacheState: DocumentOpenTraceEvent["cacheState"],
  startedAt: number,
): void {
  if (!documentOpenTraceSink) {
    return;
  }
  documentOpenTraceSink({
    requestId: crypto.randomUUID(),
    source: request.source,
    priority: request.priority,
    cacheState,
    durationMs: Math.max(0, performance.now() - startedAt),
  });
}
```

When `crypto.randomUUID` is unavailable in a Vitest environment, use the existing anonymized trace-key helper if present in `document-open-runtime.ts`; otherwise use `Math.random().toString(36).slice(2)` and do not include any path-derived value.

- [ ] **Step 6: Preserve the existing re-export file**

Confirm `src/lib/note-open-preparation.ts` still re-exports from `src/lib/document-open-runtime.ts` so existing imports keep working during migration:

```ts
export * from "@/lib/document-open-runtime";
```

- [ ] **Step 7: Run and commit**

Run: `npm run test -- tests/note-open-preparation.test.ts`

Expected: PASS.

```bash
git add src/lib/document-open-runtime.ts src/lib/note-open-preparation.ts tests/note-open-preparation.test.ts
git commit -m "feat(editor): 扩展文档打开运行时请求契约"
```

### Task 4: Add File Signature And Foreground Open IPC

**Files:**

- Modify: `src/types/ipc.ts`
- Modify: `src/lib/ipc.ts`
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/commands/file.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `tests/media-ipc.test.ts`
- Test: Rust unit tests in `src-tauri/src/commands/file.rs`

- [ ] **Step 1: Add frontend IPC tests**

Append these tests to `tests/media-ipc.test.ts` or create `tests/document-open-ipc.test.ts` if that keeps the file smaller:

```ts
import { describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

describe("document open IPC wrappers", () => {
  it("requests file signatures through the typed ipc wrapper", async () => {
    const { fileSignature } = await import("../src/lib/ipc");
    invoke.mockResolvedValueOnce({
      contentHash: "abc",
      byteLength: 3,
      modifiedMs: 10,
      isLocked: false,
    });

    await expect(
      fileSignature("a.md", { allowClassified: true }),
    ).resolves.toEqual({
      contentHash: "abc",
      byteLength: 3,
      modifiedMs: 10,
      isLocked: false,
    });
    expect(invoke).toHaveBeenCalledWith("file_signature", {
      path: "a.md",
      allowClassified: true,
    });
  });

  it("opens and closes a foreground document-open scope", async () => {
    const { documentOpenBegin, documentOpenEnd } =
      await import("../src/lib/ipc");
    invoke.mockResolvedValueOnce({ token: "scope-1" });
    invoke.mockResolvedValueOnce(undefined);

    await expect(documentOpenBegin()).resolves.toEqual({ token: "scope-1" });
    await expect(documentOpenEnd("scope-1")).resolves.toBeUndefined();
    expect(invoke).toHaveBeenNthCalledWith(1, "document_open_begin");
    expect(invoke).toHaveBeenNthCalledWith(2, "document_open_end", {
      token: "scope-1",
    });
  });
});
```

- [ ] **Step 2: Add TypeScript IPC types and wrappers**

In `src/types/ipc.ts`, add:

```ts
export interface FileSignatureResult {
  contentHash: string;
  byteLength: number;
  modifiedMs: number | null;
  isLocked: boolean;
}

export interface DocumentOpenScopeResult {
  token: string;
}
```

In `src/lib/ipc.ts`, add imports for those types and wrappers near `fileRead`:

```ts
export async function fileSignature(
  path: string,
  options?: { allowClassified?: boolean },
): Promise<FileSignatureResult> {
  return invoke<FileSignatureResult>("file_signature", {
    path,
    allowClassified: options?.allowClassified === true,
  });
}

export async function documentOpenBegin(): Promise<DocumentOpenScopeResult> {
  return invoke<DocumentOpenScopeResult>("document_open_begin");
}

export async function documentOpenEnd(token: string): Promise<void> {
  return invoke("document_open_end", { token });
}
```

- [ ] **Step 3: Add foreground activity state to AppState**

In `src-tauri/src/app.rs`, change the atomic import and add the counter:

```rust
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
```

Add this field to `pub struct AppState`:

```rust
pub foreground_document_opens: AtomicUsize,
```

Initialize it in `new_with_cas_key_override`:

```rust
foreground_document_opens: AtomicUsize::new(0),
```

Add these methods to `impl AppState`:

```rust
pub fn begin_document_open(&self) {
    self.foreground_document_opens
        .fetch_add(1, Ordering::Relaxed);
}

pub fn end_document_open(&self) {
    let mut current = self
        .foreground_document_opens
        .load(Ordering::Relaxed);
    while current > 0 {
        match self.foreground_document_opens.compare_exchange(
            current,
            current - 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(next) => current = next,
        }
    }
}

pub fn has_foreground_document_open(&self) -> bool {
    self.foreground_document_opens.load(Ordering::Relaxed) > 0
}
```

- [ ] **Step 4: Add Rust response structs and commands**

In `src-tauri/src/commands/file.rs`, add imports:

```rust
use std::time::{Duration, Instant, UNIX_EPOCH};

use crate::cas::hash::content_hash as content_hash_bytes;
```

Add these structs near `FileReadResult`:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSignatureResult {
    pub content_hash: String,
    pub byte_length: u64,
    pub modified_ms: Option<i64>,
    pub is_locked: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentOpenScopeResult {
    pub token: String,
}
```

Add these commands near `file_read`:

```rust
#[tauri::command]
pub async fn file_signature(
    state: State<'_, Arc<AppState>>,
    path: String,
    allow_classified: Option<bool>,
) -> AppResult<FileSignatureResult> {
    validate_file_read_path(&path, allow_classified.unwrap_or(false))?;
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    let db = state.inner().db.clone();
    let path_for_db = path.clone();
    tokio::task::spawn_blocking(move || {
        let raw_bytes = std::fs::read(&abs)?;
        let metadata = std::fs::metadata(&abs)?;
        let modified_ms = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as i64);
        let is_locked = query_is_locked(&db, &path_for_db)?;
        Ok(FileSignatureResult {
            content_hash: content_hash_bytes(&raw_bytes),
            byte_length: raw_bytes.len() as u64,
            modified_ms,
            is_locked,
        })
    })
    .await
    .map_err(|e| AppError::msg(format!("task join: {e}")))?
}

#[tauri::command]
pub fn document_open_begin(
    state: State<'_, Arc<AppState>>,
) -> AppResult<DocumentOpenScopeResult> {
    state.begin_document_open();
    let nanos = chrono::Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_default();
    Ok(DocumentOpenScopeResult {
        token: format!("doc-open-{nanos}"),
    })
}

#[tauri::command]
pub fn document_open_end(
    state: State<'_, Arc<AppState>>,
    _token: String,
) -> AppResult<()> {
    state.end_document_open();
    Ok(())
}

fn wait_for_document_open_idle(state: &AppState) {
    let started = Instant::now();
    while state.has_foreground_document_open() && started.elapsed() < Duration::from_millis(2_000) {
        std::thread::sleep(Duration::from_millis(8));
    }
}
```

Call `wait_for_document_open_idle(&state);` at the top of the `for abs in &files` loop in `start_vault_index_task` before reading or indexing each file.

- [ ] **Step 5: Register commands**

In `src-tauri/src/lib.rs`, add the commands to `tauri::generate_handler!` near `file_read`:

```rust
commands::file::file_signature,
commands::file::document_open_begin,
commands::file::document_open_end,
```

- [ ] **Step 6: Add backend tests for counter behavior and file signature shape**

In `src-tauri/src/commands/file.rs`, add tests to the existing test module:

```rust
#[test]
fn document_open_counter_saturates_at_zero() {
    let temp = tempfile::tempdir().unwrap();
    let state = AppState::new_with_test_cas_key(temp.path().to_path_buf(), [0xAB; 32]).unwrap();

    assert!(!state.has_foreground_document_open());
    state.begin_document_open();
    assert!(state.has_foreground_document_open());
    state.end_document_open();
    assert!(!state.has_foreground_document_open());
    state.end_document_open();
    assert!(!state.has_foreground_document_open());
}

#[test]
fn wait_for_document_open_idle_returns_after_bounded_wait() {
    let temp = tempfile::tempdir().unwrap();
    let state = AppState::new_with_test_cas_key(temp.path().to_path_buf(), [0xCD; 32]).unwrap();
    state.begin_document_open();
    let started = std::time::Instant::now();

    wait_for_document_open_idle(&state);

    assert!(started.elapsed() >= std::time::Duration::from_millis(1_900));
    state.end_document_open();
}
```

- [ ] **Step 7: Run and commit IPC work**

Run frontend IPC test: `npm run test -- tests/document-open-ipc.test.ts tests/media-ipc.test.ts`

Run Rust focused tests: `cargo test --manifest-path src-tauri/Cargo.toml commands::file::document_open_counter_saturates_at_zero commands::file::wait_for_document_open_idle_returns_after_bounded_wait`

Expected: PASS.

```bash
git add src/types/ipc.ts src/lib/ipc.ts src-tauri/src/app.rs src-tauri/src/commands/file.rs src-tauri/src/lib.rs tests/document-open-ipc.test.ts tests/media-ipc.test.ts
git commit -m "feat(ipc): 增加文档打开签名与前台活动范围"
```

### Task 5: Route Runtime Work Through The Scheduler

**Files:**

- Modify: `src/lib/document-open-runtime.ts`
- Test: `tests/note-open-preparation.test.ts`

- [ ] **Step 1: Add scheduler behavior tests to the runtime test file**

Append to `tests/note-open-preparation.test.ts`:

```ts
it("coalesces simultaneous foreground opens for the same note", async () => {
  const first = prepareNoteOpen({
    namespace: "vault-a",
    path: "coalesce.md",
    source: "welcome",
    priority: "foreground",
    signature: { contentHash: "same-hash", byteLength: 42, modifiedMs: 1 },
  });
  const second = prepareNoteOpen({
    namespace: "vault-a",
    path: "coalesce.md",
    source: "quick-open",
    priority: "foreground",
    signature: { contentHash: "same-hash", byteLength: 42, modifiedMs: 1 },
  });

  await expect(second).resolves.toEqual(await first);
  expect(fileRead).toHaveBeenCalledTimes(1);
});

it("does not let background warmup delay a foreground open", async () => {
  const order: string[] = [];
  fileRead.mockImplementation(async (path: string) => {
    order.push(path);
    return { content: `# ${path}`, isLocked: false };
  });

  const warm = prepareNoteOpen({
    namespace: "vault-a",
    path: "warm.md",
    source: "startup",
    priority: "background",
    signature: { contentHash: "warm", byteLength: 10, modifiedMs: 1 },
  });
  const foreground = prepareNoteOpen({
    namespace: "vault-a",
    path: "foreground.md",
    source: "quick-open",
    priority: "foreground",
    signature: { contentHash: "foreground", byteLength: 10, modifiedMs: 1 },
  });

  await foreground;
  await warm;
  expect(order[0]).toBe("foreground.md");
});
```

- [ ] **Step 2: Instantiate the scheduler in runtime**

In `src/lib/document-open-runtime.ts`, add a module-level scheduler:

```ts
const documentOpenScheduler = new DocumentOpenScheduler({ maxConcurrent: 2 });

function normalizeDocumentOpenRequest(
  request: DocumentOpenRequest,
): Required<Pick<DocumentOpenRequest, "path" | "source" | "priority">> &
  DocumentOpenRequest {
  return {
    ...request,
    source: request.source ?? "test",
    priority: request.priority ?? "foreground",
  };
}
```

Use an app-facing default source such as `"tab"` in call sites. Keep `"test"` only as the runtime fallback so older tests do not crash before migration.

- [ ] **Step 3: Wrap file read and parse work in a scheduler job**

Inside `prepareNoteOpen`, normalize the request, compute the `preparedNoteKey`, and replace direct `fileRead` promise creation with:

```ts
const normalized = normalizeDocumentOpenRequest(request);
const key = preparedNoteKey(normalized);
const scheduled = documentOpenScheduler.enqueue({
  key,
  namespace: documentOpenNamespace(normalized),
  path: normalized.path,
  source: normalized.source,
  priority: normalized.priority,
  run: async () => {
    const read = await fileRead(normalized.path, {
      allowClassified: normalized.allowClassified,
    });
    const parsed = parseNoteForEditor(read.content, normalized.path);
    const editorHtml = await ingestMarkdownForEditorAsync(read.content);
    return buildPreparedNoteOpen({
      request: normalized,
      read,
      parsed,
      editorHtml,
    });
  },
});
```

`buildPreparedNoteOpen` should be an extracted helper containing the object construction currently inside `prepareNoteOpen`. It must not log body content or frontmatter.

- [ ] **Step 4: Cache the scheduled promise before awaiting it**

Keep the existing “same key shares same in-flight work” behavior by storing `scheduled.promise` in the existing in-flight map before awaiting:

```ts
inflightPreparedNotes.set(key, scheduled.promise);
try {
  const prepared = await scheduled.promise;
  preparedNoteCache.set(key, prepared);
  return prepared;
} finally {
  inflightPreparedNotes.delete(key);
}
```

- [ ] **Step 5: Run and commit runtime scheduler integration**

Run: `npm run test -- tests/document-open-scheduler.test.ts tests/note-open-preparation.test.ts`

Expected: PASS.

```bash
git add src/lib/document-open-runtime.ts tests/note-open-preparation.test.ts
git commit -m "feat(editor): 通过调度器统一文档预加载"
```

### Task 6: Make All Entry Points Source-Aware

**Files:**

- Modify: `src/hooks/usePreparedNoteOpener.ts`
- Modify: `src/hooks/usePreparedWorkspaceTransitions.ts`
- Modify: `src/components/file/QuickOpen.tsx`
- Modify: `src/components/file/VaultNavigator.tsx`
- Modify: `src/components/layout/WelcomeEmpty.tsx`
- Test: `tests/use-prepared-note-opener.test.tsx`
- Test: `tests/quick-open-performance.test.tsx`
- Test: `tests/vault-navigator-corpus.test.tsx`
- Test: `tests/welcome-empty-recent.test.tsx`

- [ ] **Step 1: Add source and priority assertions to existing tests**

In `tests/quick-open-performance.test.tsx`, extend the existing visible-item preparation expectation:

```ts
expect(prepareNoteOpen).toHaveBeenCalledWith(
  expect.objectContaining({
    path: "notes/alpha.md",
    source: "quick-open",
    priority: "warm",
  }),
);
```

In the Quick Open selection test, assert the foreground open:

```ts
expect(openPreparedNote).toHaveBeenCalledWith(
  expect.objectContaining({
    path: "notes/alpha.md",
    source: "quick-open",
    priority: "foreground",
  }),
);
```

In `tests/vault-navigator-corpus.test.tsx`, assert visible folder preparation uses `source: "file-tree"` and `priority: "warm"`.

In `tests/welcome-empty-recent.test.tsx`, assert recent-note hover preparation uses `source: "welcome"` and `priority: "warm"`, while click uses `priority: "foreground"`.

- [ ] **Step 2: Change prepared opener input shape**

In `src/hooks/usePreparedNoteOpener.ts`, add this local interface:

```ts
interface PreparedNoteOpenInput {
  path: string;
  title?: string;
  isLocked?: boolean;
  source: NoteOpenSource;
  priority?: DocumentOpenPriority;
  signature?: FileSignature;
}
```

Replace ad-hoc request construction with this helper:

```ts
function toDocumentOpenRequest(
  input: PreparedNoteOpenInput,
  namespace: string,
): DocumentOpenRequest {
  return {
    namespace,
    path: input.path,
    title: input.title,
    isLocked: input.isLocked,
    source: input.source,
    priority: input.priority ?? "warm",
    signature: input.signature,
    allowClassified: input.path.startsWith(".classified/"),
  };
}
```

- [ ] **Step 3: Wrap foreground opens with backend activity scope**

In `src/hooks/usePreparedWorkspaceTransitions.ts`, add:

```ts
async function withDocumentOpenScope<T>(run: () => Promise<T>): Promise<T> {
  const scope = await documentOpenBegin();
  try {
    return await run();
  } finally {
    await documentOpenEnd(scope.token).catch(() => undefined);
  }
}
```

Use it only when the normalized request priority is `"foreground"` or `"hot"`. Warm and background preparation should not increment the backend foreground counter.

- [ ] **Step 4: Update entry points**

Use these source mappings:

```ts
const NOTE_OPEN_SOURCES = {
  welcome: "welcome",
  quickOpen: "quick-open",
  fileTree: "file-tree",
  tab: "tab",
  link: "link",
  startup: "startup",
} as const;
```

Apply them as follows:

- `QuickOpen.tsx`: visible and hover preparation uses `quick-open/warm`; item activation uses `quick-open/foreground`.
- `VaultNavigator.tsx`: visible folder and hover preparation uses `file-tree/warm`; click activation uses `file-tree/foreground`.
- `WelcomeEmpty.tsx`: recent hover preparation uses `welcome/warm`; click activation uses `welcome/foreground`; row text does not change to `Opening...` or `正在打开` during the real open.
- Internal tab activation uses `tab/hot` when target path is already open and ready, and `tab/foreground` when the tab exists but its surface is not ready.

- [ ] **Step 5: Run and commit source-aware entry migration**

Run: `npm run test -- tests/use-prepared-note-opener.test.tsx tests/quick-open-performance.test.tsx tests/vault-navigator-corpus.test.tsx tests/welcome-empty-recent.test.tsx`

Expected: PASS.

```bash
git add src/hooks/usePreparedNoteOpener.ts src/hooks/usePreparedWorkspaceTransitions.ts src/components/file/QuickOpen.tsx src/components/file/VaultNavigator.tsx src/components/layout/WelcomeEmpty.tsx tests/use-prepared-note-opener.test.tsx tests/quick-open-performance.test.tsx tests/vault-navigator-corpus.test.tsx tests/welcome-empty-recent.test.tsx
git commit -m "feat(editor): 统一文档打开入口来源与优先级"
```

### Task 7: Make Hot Tab Activation A True Hot Path

**Files:**

- Modify: `src/hooks/useTabManager.ts`
- Modify: `src/hooks/useAppPersistenceLifecycle.ts`
- Modify: `src/components/layout/AppEditorWorkspace.tsx`
- Modify: `src/components/editor/TipTapEditor.tsx`
- Test: `tests/use-tab-manager-activate-tab.test.ts`
- Test: `tests/app-editor-workspace-pending-open.test.tsx`
- Test: `tests/use-tauri-close-save.test.ts`

- [ ] **Step 1: Add hot-path regression tests**

In `tests/use-tab-manager-activate-tab.test.ts`, add:

```ts
it("activates a clean ready tab without reading or persisting before switch", async () => {
  const persistBeforeLeave = vi.fn(async () => undefined);
  const { result } = renderHook(() => useTabManager({ persistBeforeLeave }));

  await act(async () => {
    await result.current.openFile({ path: "a.md", title: "A" });
    await result.current.openFile({ path: "b.md", title: "B" });
    await result.current.activateTab("a.md", { ready: true, dirty: false });
  });

  expect(fileRead).toHaveBeenCalledTimes(2);
  expect(persistBeforeLeave).not.toHaveBeenCalled();
  expect(result.current.activePath).toBe("a.md");
});
```

In `tests/app-editor-workspace-pending-open.test.tsx`, add:

```ts
it("does not show the loading surface when switching to an already ready retained surface", async () => {
  render(<ReadyTwoTabWorkspace activePath="notes/a.md" nextPath="notes/b.md" />);

  await userEvent.click(screen.getByRole("tab", { name: /B/ }));

  expect(screen.queryByText(/正在打开|Opening/i)).not.toBeInTheDocument();
  expect(screen.getByTestId("editor-surface-notes-b-md")).toBeVisible();
});
```

`ReadyTwoTabWorkspace` should be a small test-only fixture in that test file that renders `AppEditorWorkspace` with two open note paths and editor mocks that synchronously report first-frame ready.

- [ ] **Step 2: Extend tab activation options**

In `src/hooks/useTabManager.ts`, add:

```ts
interface ActivateTabOptions {
  ready?: boolean;
  dirty?: boolean;
  source?: NoteOpenSource;
}
```

Change `activateTab(path)` to `activateTab(path, options = {})` and use this rule:

```ts
const canUseHotMountedPath = options.ready === true && options.dirty !== true;
if (canUseHotMountedPath) {
  cacheCurrentCleanMarkdownBeforeSwitch();
  setActivePath(path);
  return;
}
```

`cacheCurrentCleanMarkdownBeforeSwitch` should contain the existing clean-tab markdown cache update that does not call `persistAndCacheTab`.

- [ ] **Step 3: Let dirty retained editors switch without disk persistence**

In `src/hooks/useAppPersistenceLifecycle.ts`, add a registry reader:

```ts
export interface EditorSurfaceSnapshot {
  path: string;
  markdown: string;
  title: string;
  isDirty: boolean;
  isReady: boolean;
}

export type EditorSurfaceSnapshotReader = (
  path: string,
) => EditorSurfaceSnapshot | null;
```

When switching away from a dirty retained editor, capture the snapshot and update the in-memory dirty map. Do not await disk persistence during the visual activation. Keep the existing close/app-exit flush path responsible for writing dirty tabs to disk.

- [ ] **Step 4: Expose snapshots from TipTapEditor**

In `src/components/editor/TipTapEditor.tsx`, add a prop:

```ts
onSurfaceSnapshotChange?: (snapshot: EditorSurfaceSnapshot) => void;
```

Invoke it after content-ready, after `onUpdate`, and before unmount with the current Markdown serialization. The callback must pass Markdown text only to the in-memory lifecycle hook; it must not write localStorage or logs.

- [ ] **Step 5: Run and commit hot-path changes**

Run: `npm run test -- tests/use-tab-manager-activate-tab.test.ts tests/app-editor-workspace-pending-open.test.tsx tests/use-tauri-close-save.test.ts`

Expected: PASS.

```bash
git add src/hooks/useTabManager.ts src/hooks/useAppPersistenceLifecycle.ts src/components/layout/AppEditorWorkspace.tsx src/components/editor/TipTapEditor.tsx tests/use-tab-manager-activate-tab.test.ts tests/app-editor-workspace-pending-open.test.tsx tests/use-tauri-close-save.test.ts
git commit -m "perf(editor): 保持已打开文档切换热路径"
```

### Task 8: Add Bounded Ready Surface Retention And Loading Rules

**Files:**

- Modify: `src/components/layout/AppEditorWorkspace.tsx`
- Modify: `src/components/layout/DocumentOpenLoadingSurface.tsx`
- Test: `tests/app-editor-workspace-pending-open.test.tsx`

- [ ] **Step 1: Add retention tests**

Add this test to `tests/app-editor-workspace-pending-open.test.tsx`:

```ts
it("retains active pending dirty and eight clean ready editor surfaces", async () => {
  render(<WorkspaceWithManyReadySurfaces count={12} dirtyPath="notes/dirty.md" />);

  expect(screen.getByTestId("editor-surface-notes-active-md")).toBeInTheDocument();
  expect(screen.getByTestId("editor-surface-notes-pending-md")).toBeInTheDocument();
  expect(screen.getByTestId("editor-surface-notes-dirty-md")).toBeInTheDocument();
  expect(screen.getAllByTestId(/editor-surface-notes-clean-/)).toHaveLength(8);
});
```

Add this loading rule test:

```ts
it("shows the loading surface for cold foreground opens within the workspace", async () => {
  render(<ColdOpenWorkspace path="notes/cold.md" />);

  expect(screen.getByRole("status", { name: /正在打开|opening/i })).toBeInTheDocument();
});
```

- [ ] **Step 2: Add retention constants and helpers**

In `src/components/layout/AppEditorWorkspace.tsx`, add:

```ts
const READY_SURFACE_RETAIN_LIMIT = 8;

function shouldAlwaysRetainSurface(
  record: EditorSurfaceRecord,
  context: {
    activePath: string | null;
    pendingPath: string | null;
  },
): boolean {
  return (
    record.path === context.activePath ||
    record.path === context.pendingPath ||
    record.dirty === true ||
    record.saving === true
  );
}

function retainedSurfaceRecords(
  records: EditorSurfaceRecord[],
  context: { activePath: string | null; pendingPath: string | null },
): EditorSurfaceRecord[] {
  const required = records.filter((record) =>
    shouldAlwaysRetainSurface(record, context),
  );
  const cleanReady = records
    .filter(
      (record) => !shouldAlwaysRetainSurface(record, context) && record.ready,
    )
    .sort((a, b) => b.lastActivatedAt - a.lastActivatedAt)
    .slice(0, READY_SURFACE_RETAIN_LIMIT);
  return [...required, ...cleanReady];
}
```

Use `retainedSurfaceRecords` before mapping editor surfaces. Keep the surface identity path-stable.

- [ ] **Step 3: Remove loading for ready retained target surfaces**

In `AppEditorWorkspace.tsx`, compute loading with:

```ts
const targetSurfaceReady = activeSurfaceRecord?.ready === true;
const shouldShowDocumentLoading =
  pendingOpenLoading !== null &&
  pendingOpenLoading.path === activePath &&
  !targetSurfaceReady;
```

Set minimum visible time to `0` for hot and warm prepared opens, and use `250ms` only for cold foreground opens to avoid flash without keeping the user stuck behind a theatrical animation.

- [ ] **Step 4: Run and commit retention/loading changes**

Run: `npm run test -- tests/app-editor-workspace-pending-open.test.tsx tests/document-open-first-frame.test.tsx`

Expected: PASS.

```bash
git add src/components/layout/AppEditorWorkspace.tsx src/components/layout/DocumentOpenLoadingSurface.tsx tests/app-editor-workspace-pending-open.test.tsx
git commit -m "perf(ui): 保留编辑器热表面并统一打开反馈"
```

### Task 9: Persist Workspace Session Metadata And Warm Startup Candidates

**Files:**

- Create: `src/lib/workspace-session-snapshot.ts`
- Modify: `src/hooks/usePreparedWorkspaceTransitions.ts`
- Modify: `src/hooks/useTabManager.ts`
- Test: `tests/workspace-session-snapshot.test.ts`
- Test: `tests/home-open-transition.test.ts`

- [ ] **Step 1: Add snapshot tests**

Create `tests/workspace-session-snapshot.test.ts`:

```ts
import { beforeEach, describe, expect, it } from "vitest";

import {
  loadWorkspaceSessionSnapshot,
  saveWorkspaceSessionSnapshot,
} from "../src/lib/workspace-session-snapshot";

beforeEach(() => {
  localStorage.clear();
});

describe("workspace session snapshot", () => {
  it("persists paths and tab metadata without note content or editor html", () => {
    saveWorkspaceSessionSnapshot("vault-a", {
      activePath: "notes/a.md",
      openNotes: [
        {
          path: "notes/a.md",
          title: "A",
          isLocked: false,
          lastActiveAt: 10,
        },
      ],
    });

    const raw = localStorage.getItem("iris.workspace-session.v1:vault-a") ?? "";
    expect(raw).toContain("notes/a.md");
    expect(raw).not.toContain("markdown");
    expect(raw).not.toContain("editorHtml");
    expect(raw).not.toContain("content");

    expect(loadWorkspaceSessionSnapshot("vault-a")).toEqual({
      version: 1,
      savedAt: expect.any(Number),
      activePath: "notes/a.md",
      openNotes: [
        {
          path: "notes/a.md",
          title: "A",
          isLocked: false,
          lastActiveAt: 10,
        },
      ],
    });
  });

  it("drops malformed stored snapshots instead of throwing during startup", () => {
    localStorage.setItem("iris.workspace-session.v1:vault-a", "{broken");

    expect(loadWorkspaceSessionSnapshot("vault-a")).toBeNull();
  });
});
```

- [ ] **Step 2: Implement the snapshot module**

Create `src/lib/workspace-session-snapshot.ts`:

```ts
export interface WorkspaceSessionNoteSnapshot {
  path: string;
  title: string;
  isLocked: boolean;
  lastActiveAt: number;
}

export interface WorkspaceSessionSnapshotV1 {
  version: 1;
  savedAt: number;
  activePath: string | null;
  openNotes: WorkspaceSessionNoteSnapshot[];
}

interface WorkspaceSessionSnapshotInput {
  activePath: string | null;
  openNotes: WorkspaceSessionNoteSnapshot[];
}

const SNAPSHOT_PREFIX = "iris.workspace-session.v1:";
const MAX_SNAPSHOT_NOTES = 16;

function storageKey(vaultId: string): string {
  return `${SNAPSHOT_PREFIX}${vaultId}`;
}

function isNoteSnapshot(value: unknown): value is WorkspaceSessionNoteSnapshot {
  if (!value || typeof value !== "object") {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.path === "string" &&
    typeof candidate.title === "string" &&
    typeof candidate.isLocked === "boolean" &&
    typeof candidate.lastActiveAt === "number"
  );
}

function sanitizeSnapshot(
  input: WorkspaceSessionSnapshotInput,
): WorkspaceSessionSnapshotV1 {
  return {
    version: 1,
    savedAt: Date.now(),
    activePath: input.activePath,
    openNotes: input.openNotes
      .filter(isNoteSnapshot)
      .slice(0, MAX_SNAPSHOT_NOTES)
      .map((note) => ({
        path: note.path,
        title: note.title,
        isLocked: note.isLocked,
        lastActiveAt: note.lastActiveAt,
      })),
  };
}

export function saveWorkspaceSessionSnapshot(
  vaultId: string,
  input: WorkspaceSessionSnapshotInput,
): void {
  const snapshot = sanitizeSnapshot(input);
  localStorage.setItem(storageKey(vaultId), JSON.stringify(snapshot));
}

export function loadWorkspaceSessionSnapshot(
  vaultId: string,
): WorkspaceSessionSnapshotV1 | null {
  const raw = localStorage.getItem(storageKey(vaultId));
  if (!raw) {
    return null;
  }
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== "object") {
      return null;
    }
    const candidate = parsed as Record<string, unknown>;
    if (candidate.version !== 1 || !Array.isArray(candidate.openNotes)) {
      return null;
    }
    return {
      version: 1,
      savedAt: typeof candidate.savedAt === "number" ? candidate.savedAt : 0,
      activePath:
        typeof candidate.activePath === "string" ? candidate.activePath : null,
      openNotes: candidate.openNotes
        .filter(isNoteSnapshot)
        .slice(0, MAX_SNAPSHOT_NOTES),
    };
  } catch {
    return null;
  }
}
```

- [ ] **Step 3: Save snapshots from tab state**

In `src/hooks/useTabManager.ts`, call `saveWorkspaceSessionSnapshot` whenever open note tabs, active path, or display titles change. Use a vault id already available in the app state; if only the vault path is available, use the same vault identifier already used for prepared-note namespace.

The saved note list must be ordered by most recent activation first and capped by `MAX_SNAPSHOT_NOTES` in the snapshot module.

- [ ] **Step 4: Warm snapshot candidates after the first shell frame**

In `src/hooks/usePreparedWorkspaceTransitions.ts`, after vault readiness and after the workspace shell has rendered once, load the snapshot and schedule warmup:

```ts
const snapshot = loadWorkspaceSessionSnapshot(namespace);
for (const note of snapshot?.openNotes ?? []) {
  warmPreparedNote({
    namespace,
    path: note.path,
    title: note.title,
    isLocked: note.isLocked,
    source: "startup",
    priority: "background",
  });
}
```

Do not automatically show or activate prior tabs from this snapshot. It is a warmup hint, not UI restoration, unless the user explicitly approves restore-on-startup behavior.

- [ ] **Step 5: Run and commit snapshot warmup**

Run: `npm run test -- tests/workspace-session-snapshot.test.ts tests/home-open-transition.test.ts tests/note-open-preparation.test.ts`

Expected: PASS.

```bash
git add src/lib/workspace-session-snapshot.ts src/hooks/usePreparedWorkspaceTransitions.ts src/hooks/useTabManager.ts tests/workspace-session-snapshot.test.ts tests/home-open-transition.test.ts
git commit -m "feat(editor): 记录工作区会话并启动预热"
```

### Task 10: Complete Cache Invalidation For Mutations And External Changes

**Files:**

- Modify: `src/lib/document-open-runtime.ts`
- Modify: `src/hooks/useCurrentFileChangeListener.ts`
- Modify: `src/hooks/useTabManager.ts`
- Modify: `src/components/file/VaultNavigator.tsx`
- Test: `tests/note-open-preparation.test.ts`
- Test: `tests/use-current-file-change-listener.test.tsx` if the file exists; otherwise create `tests/document-open-invalidation.test.tsx`.

- [ ] **Step 1: Add invalidation tests**

Append to `tests/note-open-preparation.test.ts`:

```ts
it("invalidates prepared records by namespace and path", async () => {
  await prepareNoteOpen({
    namespace: "vault-a",
    path: "changed.md",
    source: "test",
    priority: "warm",
    signature: { contentHash: "before", byteLength: 10, modifiedMs: 1 },
  });

  invalidatePreparedNoteOpen({ namespace: "vault-a", path: "changed.md" });

  await prepareNoteOpen({
    namespace: "vault-a",
    path: "changed.md",
    source: "test",
    priority: "warm",
    signature: { contentHash: "after", byteLength: 11, modifiedMs: 2 },
  });

  expect(fileRead).toHaveBeenCalledTimes(2);
});

it("clears one namespace without clearing another vault namespace", async () => {
  await prepareNoteOpen({
    namespace: "vault-a",
    path: "a.md",
    source: "test",
    priority: "warm",
  });
  await prepareNoteOpen({
    namespace: "vault-b",
    path: "a.md",
    source: "test",
    priority: "warm",
  });

  clearPreparedNoteOpenCache("vault-a");
  await prepareNoteOpen({
    namespace: "vault-b",
    path: "a.md",
    source: "test",
    priority: "warm",
  });

  expect(fileRead).toHaveBeenCalledTimes(2);
});
```

- [ ] **Step 2: Normalize invalidation APIs**

In `src/lib/document-open-runtime.ts`, export:

```ts
export function invalidatePreparedNoteOpen(input: {
  namespace?: string;
  path: string;
}): void {
  const namespace = input.namespace ?? DEFAULT_DOCUMENT_OPEN_NAMESPACE;
  for (const key of preparedNoteCache.keys()) {
    if (key.startsWith(`${namespace}\0${input.path}\0`)) {
      preparedNoteCache.delete(key);
    }
  }
  for (const key of inflightPreparedNotes.keys()) {
    if (key.startsWith(`${namespace}\0${input.path}\0`)) {
      inflightPreparedNotes.delete(key);
    }
  }
}

export function clearPreparedNoteOpenCache(namespace?: string): void {
  if (!namespace) {
    preparedNoteCache.clear();
    inflightPreparedNotes.clear();
    return;
  }
  for (const key of preparedNoteCache.keys()) {
    if (key.startsWith(`${namespace}\0`)) {
      preparedNoteCache.delete(key);
    }
  }
  for (const key of inflightPreparedNotes.keys()) {
    if (key.startsWith(`${namespace}\0`)) {
      inflightPreparedNotes.delete(key);
    }
  }
}
```

- [ ] **Step 3: Call invalidation from all mutation boundaries**

Call `invalidatePreparedNoteOpen` after successful note write, rename, delete, discard, classified import/export/delete, and current-file external change events. Call `clearPreparedNoteOpenCache(namespace)` when vault changes or classified vault lock state changes.

Use existing mutation callbacks in `useTabManager.ts`, `useCurrentFileChangeListener.ts`, and `VaultNavigator.tsx`; do not add direct `invoke()` calls outside `src/lib/ipc.ts`.

- [ ] **Step 4: Run and commit invalidation work**

Run: `npm run test -- tests/note-open-preparation.test.ts tests/use-tab-manager-new-note.test.ts tests/vault-navigator-corpus.test.tsx`

Expected: PASS.

```bash
git add src/lib/document-open-runtime.ts src/hooks/useCurrentFileChangeListener.ts src/hooks/useTabManager.ts src/components/file/VaultNavigator.tsx tests/note-open-preparation.test.ts tests/use-tab-manager-new-note.test.ts tests/vault-navigator-corpus.test.tsx
git commit -m "fix(editor): 完整失效文档打开预热缓存"
```

### Task 11: Enforce Open Budgets And Add Manual Performance Checks

**Files:**

- Modify: `src/lib/document-open-runtime.ts`
- Create: `tests/document-open-budget-contract.test.ts`
- Modify: `docs/ops/performance-guide.md`
- Create: `docs/testing/document-open-runtime-manual-checklist.md`

- [ ] **Step 1: Add budget contract tests**

Create `tests/document-open-budget-contract.test.ts`:

```ts
import { describe, expect, it } from "vitest";

import { DOCUMENT_OPEN_BUDGETS } from "../src/lib/document-open-runtime";

describe("document open performance budgets", () => {
  it("keeps hot and warm visible-open budgets tight", () => {
    expect(DOCUMENT_OPEN_BUDGETS.hotTabCommitMs).toBeLessThanOrEqual(16);
    expect(DOCUMENT_OPEN_BUDGETS.warmPreparedCommitMs).toBeLessThanOrEqual(50);
  });

  it("keeps cold-open feedback fast enough to feel responsive", () => {
    expect(DOCUMENT_OPEN_BUDGETS.coldLoadingVisibleMs).toBeLessThanOrEqual(100);
    expect(DOCUMENT_OPEN_BUDGETS.coldFirstEditorFrameMs).toBeLessThanOrEqual(
      1000,
    );
  });
});
```

- [ ] **Step 2: Export budget constants**

In `src/lib/document-open-runtime.ts`, add:

```ts
export const DOCUMENT_OPEN_BUDGETS = {
  hotTabCommitMs: 16,
  warmPreparedCommitMs: 50,
  coldLoadingVisibleMs: 100,
  coldFirstEditorFrameMs: 1000,
} as const;
```

Use these constants in runtime trace comparisons and in `AppEditorWorkspace.tsx` loading timing logic instead of duplicating literal values.

- [ ] **Step 3: Create the manual checklist**

Create `docs/testing/document-open-runtime-manual-checklist.md`:

```md
# Document Open Runtime Manual Checklist

Use this checklist after automated tests pass.

## Setup

- Start Iris with `npm run tauri dev`.
- Use a vault with at least 100 Markdown notes, including one note around 50KB and one classified note if classified vault is configured.
- Open DevTools performance console if tracing is enabled for local development.

## Checks

- Welcome recent note: hover a recent note, click it, and confirm the editor replaces Welcome without lingering on row-only `正在打开` text.
- Quick Open: open Quick Open, type a query, wait for visible results, open the first result, and confirm no blank workspace frame appears.
- File tree: expand a folder, hover a note, click it, and confirm the same loading surface behavior as Quick Open.
- Hot tab: open two notes, switch between their tabs ten times, and confirm there is no visible loading surface and no repeated `fileRead` trace for the ready tab.
- Dirty tab: edit note A, switch to note B, switch back to note A, and confirm unsaved text remains present without a visual stall.
- Reopen existing document: open a note that already has a tab from Quick Open and confirm focus moves to the existing tab rather than cold-loading a duplicate.
- Startup warmup: restart Iris, open a recently used note from Quick Open, and confirm the runtime trace reports a warm or cache-hit path when possible.
- Background index contention: trigger vault indexing or reindexing, immediately open a note, and confirm visible open feedback appears quickly while index progress may temporarily slow.

## Pass Criteria

- Hot tab activation feels immediate and never shows the loading surface.
- Warm prepared opens feel immediate or near-immediate after selection.
- Cold opens show one stable loading surface quickly and then the editor.
- No trace or log line contains note body, frontmatter, title, or full path.
```

- [ ] **Step 4: Update the performance guide**

Add a `Document Open Runtime` section to `docs/ops/performance-guide.md` with:

```md
## Document Open Runtime

Budgets:

- Hot mounted tab activation: <= 16ms visible commit, no disk read.
- Warm prepared open: <= 50ms visible commit after selection.
- Cold open: loading surface visible within 100ms.
- Cold 50KB Markdown note: first editor frame within 1000ms on a normal development machine.

When investigating regressions, check runtime traces by source (`welcome`, `quick-open`, `file-tree`, `tab`, `startup`) and cache state (`hit`, `miss`, `stale`). Trace output must not include note paths, titles, Markdown body, frontmatter, prompts, selections, credentials, or decrypted classified content.
```

- [ ] **Step 5: Run and commit budget docs**

Run: `npm run test -- tests/document-open-budget-contract.test.ts`

Expected: PASS.

```bash
git add src/lib/document-open-runtime.ts tests/document-open-budget-contract.test.ts docs/ops/performance-guide.md docs/testing/document-open-runtime-manual-checklist.md
git commit -m "docs(editor): 记录文档打开性能预算"
```

### Task 12: Full Verification Before Completion

**Files:**

- No new implementation files.
- Run verification over the files changed by Tasks 1-11.

- [ ] **Step 1: Run focused frontend tests**

```bash
npm run test -- tests/document-open-scheduler.test.ts tests/note-open-preparation.test.ts tests/use-prepared-note-opener.test.tsx tests/quick-open-performance.test.tsx tests/vault-navigator-corpus.test.tsx tests/welcome-empty-recent.test.tsx tests/use-tab-manager-activate-tab.test.ts tests/app-editor-workspace-pending-open.test.tsx tests/document-open-first-frame.test.tsx tests/workspace-session-snapshot.test.ts tests/document-open-budget-contract.test.ts tests/home-open-transition.test.ts tests/use-tauri-close-save.test.ts
```

Expected: PASS.

- [ ] **Step 2: Run full frontend quality gates**

```bash
npm run typecheck
npm run lint
npm run format:check
npm run test
```

Expected: all commands exit 0.

- [ ] **Step 3: Run Rust quality gates**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all commands exit 0.

- [ ] **Step 4: Run manual checklist**

Run every item in `docs/testing/document-open-runtime-manual-checklist.md` and record observed failures in the implementation PR description.

Expected: checklist pass, with any machine-specific latency notes written in the PR body.

- [ ] **Step 5: Final commit if verification required formatting-only updates**

```bash
git add src src-tauri tests docs
git commit -m "chore(editor): 验证文档打开运行时优化"
```

Create this commit only if Step 2 or Step 3 changed files through format or lint fixes.

## Self-Review

- Spec coverage: Startup preload, visible open consistency, Quick Open, Welcome, file tree, repeated open, hot tab switching, dirty tab switching, background indexing contention, loading feedback, cache invalidation, tracing privacy, and verification are each covered by a named task.
- Mechanical scan: no red-flag empty-work instructions remain.
- Type consistency: `DocumentOpenPriority`, `NoteOpenSource`, `FileSignature`, `DocumentOpenRequest`, `FileSignatureResult`, and `DocumentOpenScopeResult` are introduced before tasks that consume them.
- Security check: The plan never persists note body or TipTap HTML across restarts, never logs paths/titles/body/frontmatter, and uses typed IPC wrappers instead of direct frontend `invoke()` calls.
- Data-source check: `.md` files remain authoritative. SQLite and localStorage are used only for indexes, runtime state, session metadata, and derived warmup hints.
- Project-rule check: No new dependency is introduced, no `unsafe` Rust is used, no worktree is created by this plan, and implementation instructions avoid `apply_patch` because the project AGENTS.md forbids it in this workspace.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-25-document-open-runtime-overhaul.md`. Two execution options:

1. **Subagent-Driven (recommended)** - dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** - execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints.

Before execution, confirm or change the six user review decisions at the top of this plan.
