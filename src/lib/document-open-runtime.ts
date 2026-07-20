import { isClassifiedVaultPath } from "@/lib/classified-path";
import {
  DocumentOpenScheduler,
  type DocumentOpenPriority,
  type NoteOpenSource,
} from "@/lib/document-open-scheduler";
import {
  editorHtmlDigest,
  getCachedEditorHtml,
  setCachedEditorHtml,
} from "@/lib/editor-html-cache";
import { ingestMarkdownForEditorAsync } from "@/lib/editor-ingest-async";
import { fileRead } from "@/lib/ipc";
import { parseNoteForEditor } from "@/lib/markdown";
import type { MarkdownSyntaxFragment } from "@/lib/markdown-contract/types";
import { pathStem, resolveNoteDisplayTitle } from "@/lib/note-display";

interface NoteOpenSignature {
  isLocked?: boolean;
  updatedAt?: string;
}

export interface FileSignature {
  byteLength: number;
  contentHash: string;
  modifiedMs: number | null;
}

export type { DocumentOpenPriority, NoteOpenSource };

export type NoteOpenNamespace = "normal" | "classified";
export type NoteOpenBudgetKind = "hot" | "warm" | "none";
export type PreparedEditorHtmlStatus =
  | "cache-hit"
  | "worker"
  | "sync"
  | "pending"
  | "failed";
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
  priority?: DocumentOpenPriority;
  signature?: FileSignature | null;
  source?: NoteOpenSource;
  titleHint?: string;
}

export interface PreparedNoteOpen {
  bodyMarkdown: string;
  content: string;
  editorHtmlDigest?: string;
  editorHtmlStatus?: PreparedEditorHtmlStatus;
  frontmatterYaml: string | null;
  isLocked: boolean;
  namespace: NoteOpenNamespace;
  path: string;
  preparedEditorHtml?: string;
  preserveFragments?: MarkdownSyntaxFragment[];
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
  priority: DocumentOpenPriority;
  source: NoteOpenSource;
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

export const DOCUMENT_OPEN_BUDGETS = {
  hotTabCommitMs: 16,
  warmPreparedCommitMs: 50,
  coldLoadingVisibleMs: 100,
  coldFirstEditorFrameMs: 1000,
} as const;
export const NOTE_OPEN_HOT_PATH_BUDGET_MS =
  DOCUMENT_OPEN_BUDGETS.hotTabCommitMs;
export const NOTE_OPEN_WARM_PATH_BUDGET_MS =
  DOCUMENT_OPEN_BUDGETS.warmPreparedCommitMs;
export const NOTE_OPEN_PERFORMANCE_ENTRY_PREFIX = "iris.note-open.";

const MAX_PREPARED_NOTES_PER_NAMESPACE = 40;
const MAX_NOTE_OPEN_PERFORMANCE_ENTRIES = 160;
const documentOpenScheduler = new DocumentOpenScheduler({ maxConcurrent: 2 });
let traceSink: NoteOpenTraceSink | null = null;
const noteOpenPerformanceEntryNames: string[] = [];
let traceSessionSalt = createTraceSessionSalt();

function namespaceFor(request: PrepareNoteOpenRequest): NoteOpenNamespace {
  return isClassifiedVaultPath(request.path) ? "classified" : "normal";
}

function normalizeRequest(
  request: PrepareNoteOpenRequest,
): Required<Pick<PrepareNoteOpenRequest, "path" | "priority" | "source">> &
  PrepareNoteOpenRequest {
  return {
    ...request,
    priority: request.priority ?? "warm",
    source: request.source ?? "test",
  };
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

function preparationCacheKey(request: PrepareNoteOpenRequest): string {
  const namespace = namespaceFor(request);
  return namespace + ":" + stableHash(namespace + "\0" + request.path);
}

function explicitSignatureFor(signature: FileSignature): string {
  return [
    signature.contentHash,
    String(signature.byteLength),
    String(signature.modifiedMs ?? "none"),
  ].join("|");
}

function signatureFor(request: PrepareNoteOpenRequest): string {
  const explicitSignature = request.signature
    ? explicitSignatureFor(request.signature)
    : "no-explicit-signature";
  const updatedAt = request.meta?.updatedAt ?? "";
  const isLocked = request.meta?.isLocked;
  return [
    namespaceFor(request),
    request.path,
    explicitSignature,
    updatedAt,
    String(isLocked ?? "unknown"),
    request.allowClassified === true ? "classified-allowed" : "normal-only",
  ].join("\0");
}

function schedulerKey(cacheKey: string, signature: string): string {
  return cacheKey + "\0" + stableHash(signature);
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
    trace.source,
    trace.priority,
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

async function prepareEditorHtml(
  path: string,
  namespace: NoteOpenNamespace,
  bodyMarkdown: string,
): Promise<{
  digest: string;
  html?: string;
  preserveFragments?: MarkdownSyntaxFragment[];
  status: PreparedEditorHtmlStatus;
}> {
  const digest = editorHtmlDigest(bodyMarkdown);
  if (!bodyMarkdown.trim()) {
    return {
      digest,
      html: "<p></p>",
      preserveFragments: [],
      status: "sync",
    };
  }
  const cached = getCachedEditorHtml(path, digest, namespace);
  if (cached) {
    return { digest, html: cached, status: "cache-hit" };
  }
  try {
    const result = await ingestMarkdownForEditorAsync({ bodyMarkdown });
    setCachedEditorHtml(path, result.tipTapHtml, digest, namespace);
    return {
      digest,
      html: result.tipTapHtml,
      preserveFragments: result.preserveFragments,
      status: bodyMarkdown.length > 50 * 1024 ? "worker" : "sync",
    };
  } catch {
    return { digest, status: "failed" };
  }
}

function emitTrace(
  request: PrepareNoteOpenRequest,
  phase: NoteOpenTracePhase,
  startedAt: number,
  status: NoteOpenTrace["status"] = "ok",
  cache: NoteOpenTrace["cache"] = "none",
  budgetKindOverride?: NoteOpenBudgetKind,
): void {
  const normalized = normalizeRequest(request);
  const durationMs = Math.max(0, performance.now() - startedAt);
  const budget = budgetForTrace(phase, budgetKindOverride);
  const trace: NoteOpenTrace = {
    ...budget,
    budgetExceeded:
      budget.budgetMs === null ? false : durationMs > budget.budgetMs,
    cache,
    durationMs,
    key: anonymousKey(normalized),
    namespace: namespaceFor(normalized),
    phase,
    priority: normalized.priority,
    source: normalized.source,
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
  const normalized = normalizeRequest(request);
  const namespace = namespaceFor(normalized);
  const signature = signatureFor(normalized);
  const cacheKey = preparationCacheKey(normalized);
  const entry = namespaceMap(namespace).get(cacheKey);
  if (!entry || entry.signature !== signature || !entry.value) return null;
  return entry.value;
}

export function warmNoteOpen(request: PrepareNoteOpenRequest): void {
  if (!canAccessNamespace(request)) {
    emitTrace(request, "prepare-denied", performance.now(), "denied", "none");
    return;
  }
  void prepareNoteOpen({
    ...request,
    priority: request.priority ?? "background",
  }).catch(() => {
    /* Warm-up is speculative; the explicit open path reports failures. */
  });
}

async function buildPreparedNoteOpen(
  normalized: ReturnType<typeof normalizeRequest>,
  namespace: NoteOpenNamespace,
  signature: string,
  content: string,
  isLocked: boolean,
  startedAt: number,
): Promise<PreparedNoteOpen> {
  const parsed = parseNoteForEditor(content, pathStem(normalized.path));
  const title = resolveNoteDisplayTitle({ path: normalized.path });
  const preparedHtml = await prepareEditorHtml(
    normalized.path,
    namespace,
    parsed.bodyMd,
  );
  emitTrace(normalized, "parse-ingest", startedAt, "ok", "none");
  return {
    bodyMarkdown: parsed.bodyMd,
    content,
    editorHtmlDigest: preparedHtml.digest,
    editorHtmlStatus: preparedHtml.status,
    frontmatterYaml: parsed.yaml,
    isLocked,
    namespace,
    path: normalized.path,
    preparedEditorHtml: preparedHtml.html,
    preserveFragments: preparedHtml.preserveFragments,
    signature,
    title,
    traceKey: anonymousKey(normalized),
  };
}

export async function prepareNoteOpenFromContent(
  request: PrepareNoteOpenRequest,
  source: { content: string; isLocked: boolean },
): Promise<PreparedNoteOpen> {
  const normalized = normalizeRequest(request);
  const startedAt = performance.now();
  if (!canAccessNamespace(normalized)) {
    emitTrace(normalized, "prepare-denied", startedAt, "denied", "none");
    return Promise.reject(
      new Error("Classified note preparation requires explicit permission"),
    );
  }

  const namespace = namespaceFor(normalized);
  const signature = signatureFor(normalized);
  const cacheKey = preparationCacheKey(normalized);
  emitTrace(normalized, "prepare-start", startedAt, "ok", "miss");
  try {
    const prepared = await buildPreparedNoteOpen(
      normalized,
      namespace,
      signature,
      source.content,
      source.isLocked,
      startedAt,
    );
    remember(namespace, cacheKey, {
      path: normalized.path,
      signature,
      value: prepared,
    });
    emitTrace(normalized, "prepare-done", startedAt, "ok", "write");
    return prepared;
  } catch (error: unknown) {
    emitTrace(normalized, "prepare-error", startedAt, "error", "none");
    throw error;
  }
}

export function prepareNoteOpen(
  request: PrepareNoteOpenRequest,
): Promise<PreparedNoteOpen> {
  const normalized = normalizeRequest(request);
  const startedAt = performance.now();
  if (!canAccessNamespace(normalized)) {
    emitTrace(normalized, "prepare-denied", startedAt, "denied", "none");
    return Promise.reject(
      new Error("Classified note preparation requires explicit permission"),
    );
  }

  const namespace = namespaceFor(normalized);
  const signature = signatureFor(normalized);
  const cacheKey = preparationCacheKey(normalized);
  const current = namespaceMap(namespace).get(cacheKey);
  if (current?.signature === signature) {
    if (current.value) {
      emitTrace(normalized, "cache-hit", startedAt, "ok", "hit");
      return Promise.resolve(current.value);
    }
    if (current.promise) {
      documentOpenScheduler.promote(
        schedulerKey(cacheKey, signature),
        normalized.priority,
        normalized.source,
      );
      emitTrace(normalized, "cache-hit", startedAt, "ok", "hit");
      return current.promise;
    }
  }

  emitTrace(normalized, "prepare-start", startedAt, "ok", "miss");
  const scheduled = documentOpenScheduler.enqueue<PreparedNoteOpen>({
    key: schedulerKey(cacheKey, signature),
    namespace,
    path: normalized.path,
    priority: normalized.priority,
    source: normalized.source,
    run: async (signal) => {
      if (signal.aborted) {
        throw new DOMException("Document open job cancelled", "AbortError");
      }
      const { content, isLocked } = await fileRead(normalized.path, {
        allowClassified: normalized.allowClassified === true,
      });
      if (signal.aborted) {
        throw new DOMException("Document open job cancelled", "AbortError");
      }
      emitTrace(normalized, "file-read", startedAt);
      return buildPreparedNoteOpen(
        normalized,
        namespace,
        signature,
        content,
        isLocked,
        startedAt,
      );
    },
  });

  const promise: Promise<PreparedNoteOpen> = scheduled.promise
    .then((prepared) => {
      const latest = namespaceMap(namespace).get(cacheKey);
      if (latest?.signature === signature && latest.promise === promise) {
        remember(namespace, cacheKey, {
          path: normalized.path,
          signature,
          value: prepared,
        });
      }
      emitTrace(normalized, "prepare-done", startedAt, "ok", "write");
      return prepared;
    })
    .catch((error: unknown) => {
      const latest = namespaceMap(namespace).get(cacheKey);
      if (latest?.signature === signature && latest.promise === promise) {
        namespaceMap(namespace).delete(cacheKey);
      }
      emitTrace(normalized, "prepare-error", startedAt, "error", "none");
      throw error;
    });

  remember(namespace, cacheKey, {
    path: normalized.path,
    promise,
    signature,
  });
  return promise;
}
