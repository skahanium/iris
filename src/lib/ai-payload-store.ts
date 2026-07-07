import type { ChatLine } from "@/components/ai/AiMessageList";
import { assistantContentHash } from "@/lib/assistant-stream-buffer";

export type AiPayloadKind =
  | "assistant_message"
  | "user_message"
  | "task_event"
  | "artifact_payload"
  | "document_summary"
  | "research_payload"
  | "evidence_packet"
  | "generic";

export interface AiPayloadRef {
  id: string;
  kind: AiPayloadKind;
  length: number;
  hash: string;
  preview: string;
  omittedChars: number;
}

export interface ProjectedText {
  content: string;
  payloadRef?: AiPayloadRef;
}

interface StoreEntry {
  kind: AiPayloadKind;
  value: string;
  length: number;
  hash: string;
  createdAt: number;
  refCount: number;
}

export interface AiPayloadStoreSnapshot {
  entryCount: number;
  totalChars: number;
  totalEstimatedBytes: number;
  entries: Array<{
    id: string;
    kind: AiPayloadKind;
    length: number;
    hash: string;
    refCount: number;
  }>;
}

export interface AiPayloadStore {
  putText(value: string, kind?: AiPayloadKind): AiPayloadRef;
  getText(ref: AiPayloadRef | string | null | undefined): string | null;
  retain(ref: AiPayloadRef | string | null | undefined): void;
  release(ref: AiPayloadRef | string | null | undefined): void;
  clear(): void;
  snapshot(): AiPayloadStoreSnapshot;
}

const DEFAULT_INLINE_TEXT_CHARS = 80_000;
const DEFAULT_PREVIEW_TEXT_CHARS = 38_000;
const DEFAULT_SANITIZE_TEXT_CHARS = 8_000;
const MAX_SANITIZE_ARRAY_ITEMS = 50;
const MAX_SANITIZE_DEPTH = 7;

function nowMs(): number {
  return typeof performance !== "undefined" ? performance.now() : Date.now();
}

function refIdFor(kind: AiPayloadKind, hash: string, length: number): string {
  return `ai-payload:${kind}:${hash}:${length}`;
}

function createPreviewWindow(
  value: string,
  maxChars: number,
): { content: string; omittedChars: number } {
  const budget = Math.max(0, Math.floor(maxChars));
  if (value.length <= budget) return { content: value, omittedChars: 0 };
  if (budget <= 0) return { content: "", omittedChars: value.length };
  const notice = "\n\n[content omitted for memory safety]\n\n";
  const bodyBudget = Math.max(0, budget - notice.length);
  const prefixLength = Math.min(Math.floor(bodyBudget * 0.35), value.length);
  const suffixLength = Math.max(0, bodyBudget - prefixLength);
  const prefix = value.slice(0, prefixLength);
  const suffix = value.slice(
    Math.max(prefixLength, value.length - suffixLength),
  );
  return {
    content: `${prefix}${notice}${suffix}`,
    omittedChars: Math.max(0, value.length - prefix.length - suffix.length),
  };
}

function refFromEntry(
  id: string,
  entry: StoreEntry,
  previewChars: number,
): AiPayloadRef {
  const preview = createPreviewWindow(entry.value, previewChars);
  return {
    id,
    kind: entry.kind,
    length: entry.length,
    hash: entry.hash,
    preview: preview.content,
    omittedChars: preview.omittedChars,
  };
}

function refKey(ref: AiPayloadRef | string | null | undefined): string | null {
  if (!ref) return null;
  return typeof ref === "string" ? ref : ref.id;
}

export function createAiPayloadStore(): AiPayloadStore {
  const entries = new Map<string, StoreEntry>();

  return {
    putText(value: string, kind: AiPayloadKind = "generic"): AiPayloadRef {
      const hash = assistantContentHash(value);
      const id = refIdFor(kind, hash, value.length);
      const existing = entries.get(id);
      if (existing) {
        existing.refCount += 1;
        return refFromEntry(id, existing, DEFAULT_PREVIEW_TEXT_CHARS);
      }
      const entry: StoreEntry = {
        kind,
        value,
        length: value.length,
        hash,
        createdAt: nowMs(),
        refCount: 1,
      };
      entries.set(id, entry);
      return refFromEntry(id, entry, DEFAULT_PREVIEW_TEXT_CHARS);
    },

    getText(ref: AiPayloadRef | string | null | undefined): string | null {
      const key = refKey(ref);
      if (!key) return null;
      return entries.get(key)?.value ?? null;
    },

    retain(ref: AiPayloadRef | string | null | undefined): void {
      const key = refKey(ref);
      if (!key) return;
      const entry = entries.get(key);
      if (entry) entry.refCount += 1;
    },

    release(ref: AiPayloadRef | string | null | undefined): void {
      const key = refKey(ref);
      if (!key) return;
      const entry = entries.get(key);
      if (!entry) return;
      entry.refCount -= 1;
      if (entry.refCount <= 0) entries.delete(key);
    },

    clear(): void {
      entries.clear();
    },

    snapshot(): AiPayloadStoreSnapshot {
      const values = Array.from(entries.entries());
      const totalChars = values.reduce(
        (sum, [, entry]) => sum + entry.length,
        0,
      );
      return {
        entryCount: values.length,
        totalChars,
        totalEstimatedBytes: totalChars * 2,
        entries: values.map(([id, entry]) => ({
          id,
          kind: entry.kind,
          length: entry.length,
          hash: entry.hash,
          refCount: entry.refCount,
        })),
      };
    },
  };
}

const defaultAiPayloadStore = createAiPayloadStore();

export function getAiPayloadStore(): AiPayloadStore {
  return defaultAiPayloadStore;
}

export function projectTextForUi(
  store: AiPayloadStore,
  value: string,
  options?: {
    kind?: AiPayloadKind;
    maxInlineChars?: number;
    maxPreviewChars?: number;
  },
): ProjectedText {
  const maxInlineChars = options?.maxInlineChars ?? DEFAULT_INLINE_TEXT_CHARS;
  const maxPreviewChars =
    options?.maxPreviewChars ?? DEFAULT_PREVIEW_TEXT_CHARS;
  if (value.length <= maxInlineChars) {
    return { content: value };
  }
  const ref = store.putText(value, options?.kind ?? "generic");
  const preview = createPreviewWindow(value, maxPreviewChars);
  return {
    content: preview.content,
    payloadRef: {
      ...ref,
      preview: preview.content,
      omittedChars: preview.omittedChars,
    },
  };
}

export function restoreProjectedText(
  store: AiPayloadStore,
  projection: ProjectedText,
): string {
  if (!projection.payloadRef) return projection.content;
  return store.getText(projection.payloadRef) ?? projection.content;
}

export function resolvePayloadText(
  value: string,
  ref?: AiPayloadRef,
  store: AiPayloadStore = defaultAiPayloadStore,
): string {
  return ref ? (store.getText(ref) ?? value) : value;
}

export interface SanitizedContentRef {
  contentRef: Omit<AiPayloadRef, "preview">;
  preview: string;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    Object.getPrototypeOf(value) === Object.prototype
  );
}

export function sanitizePayloadForUi(
  store: AiPayloadStore,
  value: unknown,
  options?: {
    maxPreviewChars?: number;
    maxInlineChars?: number;
    depth?: number;
  },
): unknown {
  const depth = options?.depth ?? 0;
  const maxInlineChars = options?.maxInlineChars ?? DEFAULT_SANITIZE_TEXT_CHARS;
  const maxPreviewChars =
    options?.maxPreviewChars ?? DEFAULT_SANITIZE_TEXT_CHARS;

  if (typeof value === "string") {
    if (value.length <= maxInlineChars) return value;
    const projected = projectTextForUi(store, value, {
      kind: "artifact_payload",
      maxInlineChars,
      maxPreviewChars,
    });
    const ref = projected.payloadRef!;
    return {
      preview: projected.content,
      contentRef: {
        id: ref.id,
        kind: ref.kind,
        length: ref.length,
        hash: ref.hash,
        omittedChars: ref.omittedChars,
      },
    } satisfies SanitizedContentRef;
  }

  if (Array.isArray(value)) {
    if (depth >= MAX_SANITIZE_DEPTH) {
      return { omittedItems: value.length, reason: "depth_budget" };
    }
    const visible = value.slice(-MAX_SANITIZE_ARRAY_ITEMS).map((item) =>
      sanitizePayloadForUi(store, item, {
        maxInlineChars,
        maxPreviewChars,
        depth: depth + 1,
      }),
    );
    if (value.length <= MAX_SANITIZE_ARRAY_ITEMS) return visible;
    return {
      omittedItems: value.length - visible.length,
      items: visible,
    };
  }

  if (isPlainObject(value)) {
    if (depth >= MAX_SANITIZE_DEPTH) {
      return { omittedKeys: Object.keys(value).length, reason: "depth_budget" };
    }
    const next: Record<string, unknown> = {};
    for (const [key, item] of Object.entries(value)) {
      next[key] = sanitizePayloadForUi(store, item, {
        maxInlineChars,
        maxPreviewChars,
        depth: depth + 1,
      });
    }
    return next;
  }

  return value;
}

export interface ChatLineWithPayloadRef extends ChatLine {
  contentRef?: AiPayloadRef;
}

export function compactChatLineForState(
  store: AiPayloadStore,
  message: ChatLine,
): ChatLineWithPayloadRef {
  const projected = projectTextForUi(store, message.content, {
    kind: message.role === "assistant" ? "assistant_message" : "user_message",
  });
  if (!projected.payloadRef) {
    return message as ChatLineWithPayloadRef;
  }
  return {
    ...message,
    content: projected.content,
    contentRef: projected.payloadRef,
  };
}

function chatLinePayloadRef(message: ChatLine): AiPayloadRef | undefined {
  return (message as ChatLineWithPayloadRef).contentRef;
}

function payloadRefCounts(
  messages: readonly ChatLine[],
): Map<string, { count: number; ref: AiPayloadRef }> {
  const counts = new Map<string, { count: number; ref: AiPayloadRef }>();
  for (const message of messages) {
    const ref = chatLinePayloadRef(message);
    if (!ref) continue;
    const current = counts.get(ref.id);
    if (current) {
      current.count += 1;
    } else {
      counts.set(ref.id, { count: 1, ref });
    }
  }
  return counts;
}

export function releaseChatLinePayloadRefs(
  store: AiPayloadStore,
  messages: readonly ChatLine[],
): void {
  for (const { count, ref } of payloadRefCounts(messages).values()) {
    for (let index = 0; index < count; index += 1) {
      store.release(ref);
    }
  }
}

export function reconcileChatLinePayloadRefs(
  store: AiPayloadStore,
  previous: readonly ChatLine[],
  next: readonly ChatLine[],
): void {
  const previousCounts = payloadRefCounts(previous);
  const nextCounts = payloadRefCounts(next);
  for (const [id, previousEntry] of previousCounts) {
    const nextCount = nextCounts.get(id)?.count ?? 0;
    const releaseCount = Math.max(0, previousEntry.count - nextCount);
    for (let index = 0; index < releaseCount; index += 1) {
      store.release(previousEntry.ref);
    }
  }
}

export function compactChatLinesForState(
  store: AiPayloadStore,
  messages: ChatLine[],
  previous: readonly ChatLine[] = [],
): ChatLineWithPayloadRef[] {
  const compacted = messages.map((message) =>
    compactChatLineForState(store, message),
  );
  reconcileChatLinePayloadRefs(store, previous, compacted);
  return compacted;
}

export function restoreChatLineContent(
  message: ChatLine,
  store: AiPayloadStore = defaultAiPayloadStore,
): string {
  const maybeRef = (message as ChatLineWithPayloadRef).contentRef;
  return resolvePayloadText(message.content, maybeRef, store);
}

export function restoreChatLineForPersistence(
  message: ChatLine,
  store: AiPayloadStore = defaultAiPayloadStore,
): ChatLine {
  const content = restoreChatLineContent(message, store);
  return { ...message, content };
}

export function restoreChatLinesForPersistence(
  messages: ChatLine[],
  store: AiPayloadStore = defaultAiPayloadStore,
): ChatLine[] {
  return messages.map((message) =>
    restoreChatLineForPersistence(message, store),
  );
}
