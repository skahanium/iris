import { isClassifiedVaultPath } from "@/lib/classified-path";
import { displayTitleFromMarkdown } from "@/lib/document-title";
import { editorHtmlDigest, setCachedEditorHtml } from "@/lib/editor-html-cache";
import { ingestMarkdownForEditor } from "@/lib/editor-ingest";
import { fileRead } from "@/lib/ipc";
import { parseNoteForEditor } from "@/lib/markdown";
import { pathStem, resolveNoteDisplayTitle } from "@/lib/note-display";

interface NoteOpenSignature {
  isLocked?: boolean;
  updatedAt?: string;
}

export type NoteOpenNamespace = "normal" | "classified";
export type NoteOpenBudgetKind = "hot" | "warm" | "none";
type NoteOpenTracePhase =
  | "cache-hit"
  | "prepare-start"
  | "file-read"
  | "parse-ingest"
  | "prepare-done"
  | "visible-commit"
  | "prepare-error"
  | "prepare-denied";

export interface PrepareNoteOpenRequest {
  allowClassified?: boolean;
  meta?: NoteOpenSignature | null;
  path: string;
  titleHint?: string;
}

export interface PreparedNoteOpen {
  bodyMarkdown: string;
  content: string;
  frontmatterYaml: string | null;
  isLocked: boolean;
  namespace: NoteOpenNamespace;
  path: string;
  signature: string;
  title: string;
  traceKey: string;
}

export interface NoteOpenTrace {
  budgetExceeded: boolean;
  budgetKind: NoteOpenBudgetKind;
  budgetMs: number | null;
  cache: "hit" | "miss" | "write" | "none";
  durationMs: number;
  key: string;
  namespace: NoteOpenNamespace;
  phase: NoteOpenTracePhase;
  status: "ok" | "error" | "denied";
}

interface PreparationEntry {
  path: string;
  promise?: Promise<PreparedNoteOpen>;
  signature: string;
  value?: PreparedNoteOpen;
}

export type NoteOpenTraceSink = (trace: NoteOpenTrace) => void;

const preparedByNamespace = new Map<
  NoteOpenNamespace,
  Map<string, PreparationEntry>
>([
  ["normal", new Map()],
  ["classified", new Map()],
]);
export const NOTE_OPEN_HOT_PATH_BUDGET_MS = 16;
export const NOTE_OPEN_WARM_PATH_BUDGET_MS = 50;
export const NOTE_OPEN_PERFORMANCE_ENTRY_PREFIX = "iris.note-open.";

const MAX_PREPARED_NOTES_PER_NAMESPACE = 40;
const MAX_NOTE_OPEN_PERFORMANCE_ENTRIES = 160;
let traceSink: NoteOpenTraceSink | null = null;
const noteOpenPerformanceEntryNames: string[] = [];
let traceSessionSalt = createTraceSessionSalt();

function namespaceFor(request: PrepareNoteOpenRequest): NoteOpenNamespace {
  return isClassifiedVaultPath(request.path) ? "classified" : "normal";
}

function canAccessNamespace(request: PrepareNoteOpenRequest): boolean {
  return (
    namespaceFor(request) !== "classified" || request.allowClassified === true
  );
}

function createTraceSessionSalt(): string {
  const crypto = globalThis.crypto;
  if (crypto?.randomUUID) return crypto.randomUUID();
  if (crypto?.getRandomValues) {
    const bytes = new Uint32Array(4);
    crypto.getRandomValues(bytes);
    return Array.from(bytes, (value) => value.toString(16)).join("");
  }
  return String(performance.now()) + ":" + String(Math.random());
}

function stableHash(input: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < input.length; i++) {
    hash ^= input.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16);
}

function anonymousKey(request: PrepareNoteOpenRequest): string {
  const namespace = namespaceFor(request);
  return (
    namespace +
    ":" +
    stableHash(traceSessionSalt + "\0" + namespace + "\0" + request.path)
  );
}

function signatureFor(request: PrepareNoteOpenRequest): string {
  const updatedAt = request.meta?.updatedAt ?? "";
  const isLocked = request.meta?.isLocked;
  return [
    namespaceFor(request),
    request.path,
    updatedAt,
    String(isLocked ?? "unknown"),
    request.allowClassified === true ? "classified-allowed" : "normal-only",
  ].join("\0");
}

function namespaceMap(
  namespace: NoteOpenNamespace,
): Map<string, PreparationEntry> {
  return preparedByNamespace.get(namespace)!;
}

function remember(
  namespace: NoteOpenNamespace,
  cacheKey: string,
  entry: PreparationEntry,
): void {
  const entries = namespaceMap(namespace);
  if (
    entries.size >= MAX_PREPARED_NOTES_PER_NAMESPACE &&
    !entries.has(cacheKey)
  ) {
    const oldest = entries.keys().next().value;
    if (oldest !== undefined) entries.delete(oldest);
  }
  entries.set(cacheKey, entry);
}

function performanceMeasureName(trace: NoteOpenTrace): string {
  return [
    NOTE_OPEN_PERFORMANCE_ENTRY_PREFIX + trace.phase,
    trace.status,
    trace.cache,
    trace.budgetKind,
    trace.namespace,
    trace.key,
  ].join(".");
}

function recordPerformanceTrace(trace: NoteOpenTrace, startedAt: number): void {
  try {
    const name = performanceMeasureName(trace);
    performance.measure(name, {
      start: Math.max(0, startedAt),
      duration: trace.durationMs,
    });
    noteOpenPerformanceEntryNames.push(name);
    while (
      noteOpenPerformanceEntryNames.length > MAX_NOTE_OPEN_PERFORMANCE_ENTRIES
    ) {
      const oldest = noteOpenPerformanceEntryNames.shift();
      if (oldest) performance.clearMeasures(oldest);
    }
  } catch {
    // Performance Timeline support differs across test and WebView runtimes.
  }
}

function budgetForTrace(
  phase: NoteOpenTracePhase,
  budgetKindOverride?: NoteOpenBudgetKind,
): {
  budgetKind: NoteOpenBudgetKind;
  budgetMs: number | null;
} {
  if (budgetKindOverride === "hot") {
    return { budgetKind: "hot", budgetMs: NOTE_OPEN_HOT_PATH_BUDGET_MS };
  }
  if (budgetKindOverride === "warm") {
    return { budgetKind: "warm", budgetMs: NOTE_OPEN_WARM_PATH_BUDGET_MS };
  }
  if (budgetKindOverride === "none") {
    return { budgetKind: "none", budgetMs: null };
  }
  if (phase === "cache-hit") {
    return { budgetKind: "hot", budgetMs: NOTE_OPEN_HOT_PATH_BUDGET_MS };
  }
  if (phase === "prepare-done") {
    return { budgetKind: "warm", budgetMs: NOTE_OPEN_WARM_PATH_BUDGET_MS };
  }
  return { budgetKind: "none", budgetMs: null };
}

function emitTrace(
  request: PrepareNoteOpenRequest,
  phase: NoteOpenTracePhase,
  startedAt: number,
  status: NoteOpenTrace["status"] = "ok",
  cache: NoteOpenTrace["cache"] = "none",
  budgetKindOverride?: NoteOpenBudgetKind,
): void {
  const durationMs = Math.max(0, performance.now() - startedAt);
  const budget = budgetForTrace(phase, budgetKindOverride);
  const trace: NoteOpenTrace = {
    ...budget,
    budgetExceeded:
      budget.budgetMs === null ? false : durationMs > budget.budgetMs,
    cache,
    durationMs,
    key: anonymousKey(request),
    namespace: namespaceFor(request),
    phase,
    status,
  };
  recordPerformanceTrace(trace, startedAt);
  traceSink?.(trace);
}

export function setNoteOpenTraceSink(sink: NoteOpenTraceSink | null): void {
  traceSink = sink;
}

export function emitNoteOpenVisibleCommitTrace(
  request: PrepareNoteOpenRequest,
  startedAt: number,
  budgetKind: NoteOpenBudgetKind,
): void {
  emitTrace(request, "visible-commit", startedAt, "ok", "none", budgetKind);
}

export function resetNoteOpenTraceSession(): void {
  traceSessionSalt = createTraceSessionSalt();
}

export function clearNoteOpenPerformanceEntries(): void {
  for (const name of noteOpenPerformanceEntryNames) {
    performance.clearMeasures(name);
  }
  noteOpenPerformanceEntryNames.length = 0;
}

export function clearNoteOpenPreparationCache(
  namespace?: NoteOpenNamespace,
): void {
  if (namespace) {
    namespaceMap(namespace).clear();
    return;
  }
  preparedByNamespace.forEach((entries) => entries.clear());
}

export function invalidateNoteOpenPreparation(path: string): void {
  preparedByNamespace.forEach((entries) => {
    for (const [cacheKey, entry] of entries) {
      if (entry.path === path) entries.delete(cacheKey);
    }
  });
}

export function getPreparedNoteOpen(
  request: PrepareNoteOpenRequest,
): PreparedNoteOpen | null {
  if (!canAccessNamespace(request)) return null;
  const namespace = namespaceFor(request);
  const signature = signatureFor(request);
  const cacheKey = anonymousKey(request);
  const entry = namespaceMap(namespace).get(cacheKey);
  if (!entry || entry.signature !== signature || !entry.value) return null;
  return entry.value;
}

export function warmNoteOpen(request: PrepareNoteOpenRequest): void {
  if (!canAccessNamespace(request)) {
    emitTrace(request, "prepare-denied", performance.now(), "denied", "none");
    return;
  }
  void prepareNoteOpen(request).catch(() => {
    /* Warm-up is speculative; the explicit open path reports failures. */
  });
}

export function prepareNoteOpen(
  request: PrepareNoteOpenRequest,
): Promise<PreparedNoteOpen> {
  const startedAt = performance.now();
  if (!canAccessNamespace(request)) {
    emitTrace(request, "prepare-denied", startedAt, "denied", "none");
    return Promise.reject(
      new Error("Classified note preparation requires explicit permission"),
    );
  }

  const namespace = namespaceFor(request);
  const signature = signatureFor(request);
  const cacheKey = anonymousKey(request);
  const current = namespaceMap(namespace).get(cacheKey);
  if (current?.signature === signature) {
    if (current.value) {
      emitTrace(request, "cache-hit", startedAt, "ok", "hit");
      return Promise.resolve(current.value);
    }
    if (current.promise) {
      emitTrace(request, "cache-hit", startedAt, "ok", "hit");
      return current.promise;
    }
  }

  emitTrace(request, "prepare-start", startedAt, "ok", "miss");
  const promise = fileRead(request.path, {
    allowClassified: request.allowClassified === true,
  })
    .then(({ content, isLocked }) => {
      emitTrace(request, "file-read", startedAt);
      const parsed = parseNoteForEditor(content, pathStem(request.path));
      const fromMarkdown = displayTitleFromMarkdown(content, "");
      const title = resolveNoteDisplayTitle({
        path: request.path,
        title: fromMarkdown || request.titleHint?.trim() || parsed.title,
        markdown: content,
      });
      const { tipTapHtml } = ingestMarkdownForEditor({
        bodyMarkdown: parsed.bodyMd.trim(),
      });
      setCachedEditorHtml(
        request.path,
        tipTapHtml,
        editorHtmlDigest(parsed.bodyMd),
        namespace,
      );
      emitTrace(request, "parse-ingest", startedAt, "ok", "write");
      const prepared: PreparedNoteOpen = {
        bodyMarkdown: parsed.bodyMd,
        content,
        frontmatterYaml: parsed.yaml,
        isLocked,
        namespace,
        path: request.path,
        signature,
        title,
        traceKey: anonymousKey(request),
      };
      remember(namespace, cacheKey, {
        path: request.path,
        signature,
        value: prepared,
      });
      emitTrace(request, "prepare-done", startedAt, "ok", "write");
      return prepared;
    })
    .catch((error: unknown) => {
      const latest = namespaceMap(namespace).get(cacheKey);
      if (latest?.signature === signature) {
        namespaceMap(namespace).delete(cacheKey);
      }
      emitTrace(request, "prepare-error", startedAt, "error", "none");
      throw error;
    });

  remember(namespace, cacheKey, { path: request.path, promise, signature });
  return promise;
}
