import type {
  ArtifactTab,
  AssistantArtifactDraft,
  ArtifactKind,
} from "@/types/assistant-artifact";
import {
  getAiPayloadStore,
  sanitizePayloadForUi,
} from "@/lib/ai-payload-store";

export const ARTIFACT_TAB_STORAGE_KEY = "iris-ai-artifact-tabs-v1";
export const ARTIFACT_TAB_LIMIT = 10;

const SENSITIVE_KEY_PATTERNS = [
  "apikey",
  "api_key",
  "token",
  "password",
  "secret",
  "notecontent",
  "note_content",
  "checkpoint",
  "raw",
  "preview",
  "sha256",
  "request_id",
  "requestid",
];

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    Object.getPrototypeOf(value) === Object.prototype
  );
}

function asRecord(value: unknown): Record<string, unknown> {
  return isPlainObject(value) ? value : {};
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function nonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function positiveNumber(value: unknown): boolean {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function collectByKey(
  value: unknown,
  matches: (key: string) => boolean,
): unknown[] {
  if (Array.isArray(value)) {
    return value.flatMap((item) => collectByKey(item, matches));
  }
  if (!isPlainObject(value)) return [];
  const items: unknown[] = [];
  for (const [key, item] of Object.entries(value)) {
    if (matches(key)) items.push(item);
    items.push(...collectByKey(item, matches));
  }
  return items;
}

function collectionHasValue(value: unknown): boolean {
  if (Array.isArray(value)) return value.length > 0;
  if (isPlainObject(value)) return Object.keys(value).length > 0;
  return nonEmptyString(value) || positiveNumber(value) || value === true;
}

function mechanicalGapOnlyText(value: unknown): boolean {
  if (!nonEmptyString(value)) return false;
  const normalized = value.toLowerCase();
  return (
    /(未授权|未检索|未找到|没有|暂无|无可用|no evidence|no source|not generated|ordinary completion)/.test(
      normalized,
    ) && /(evidence|source|证据|来源|检索|联网)/.test(normalized)
  );
}

function hasRealEvidenceSourceValue(payload: unknown): boolean {
  const record = asRecord(payload);
  if (
    positiveNumber(record.evidence_count) ||
    positiveNumber(record.evidenceCount)
  ) {
    return true;
  }

  const matrix = asRecord(record.evidence_matrix);
  if (positiveNumber(matrix.total_evidence_count)) return true;

  const evidenceLike = collectByKey(payload, (key) =>
    /(evidence|sources?|conflicts?|freshness)/i.test(key),
  );
  const nonGapEvidence = evidenceLike.some((item) => {
    if (!collectionHasValue(item)) return false;
    if (Array.isArray(item) && item.every(mechanicalGapOnlyText)) return false;
    if (mechanicalGapOnlyText(item)) return false;
    return true;
  });
  if (nonGapEvidence) return true;

  const realGaps = collectByKey(payload, (key) => /gaps?/i.test(key)).some(
    (item) => {
      if (Array.isArray(item)) {
        return item.some(
          (gap) => collectionHasValue(gap) && !mechanicalGapOnlyText(gap),
        );
      }
      return collectionHasValue(item) && !mechanicalGapOnlyText(item);
    },
  );
  return realGaps;
}

function processStatus(payload: unknown): string {
  const record = asRecord(payload);
  const task = asRecord(record.task);
  const status = record.status ?? task.status;
  return typeof status === "string" ? status : "";
}

function hasCheckpoint(payload: unknown): boolean {
  const record = asRecord(payload);
  return (
    collectionHasValue(record.checkpoint) ||
    asArray(record.checkpoints).length > 0 ||
    asArray(record.diagnostic_checkpoints).length > 0
  );
}

function hasProcessValue(payload: unknown): boolean {
  const status = processStatus(payload);
  if (
    [
      "pending_confirmation",
      "awaiting_confirmation",
      "failed",
      "failed_safe",
      "paused_budget",
      "paused_recoverable",
    ].includes(status)
  ) {
    return true;
  }
  const record = asRecord(payload);
  if (
    asArray(record.evidenceGaps).length > 0 ||
    asArray(record.verificationFailures).length > 0
  ) {
    return true;
  }
  return (
    (record.long_task === true || record.longTask === true) &&
    hasCheckpoint(payload)
  );
}

function hasWritingChangeValue(payload: unknown): boolean {
  const record = asRecord(payload);
  if (asArray(record.patches).length > 0) return true;
  if (nonEmptyString(record.diff) || nonEmptyString(record.patch)) return true;
  return asArray(record.candidates).some((candidate) => {
    const type = asRecord(candidate).type;
    return (
      type === "patch" ||
      type === "diff" ||
      type === "insert" ||
      type === "replace"
    );
  });
}

function hasStructuredResultValue(payload: unknown): boolean {
  const record = asRecord(payload);
  const suggestions = asArray(record.suggestions);
  const issues = asArray(record.issues);
  if (suggestions.length > 0 || issues.length > 0) {
    return true;
  }
  if (collectionHasValue(record.coverage)) return true;
  const result = asRecord(record.result);
  const batch = asRecord(record.batch);
  return (
    collectionHasValue(result.coverage) ||
    asArray(result.claims).length > 0 ||
    asArray(result.evidence_used).length > 0 ||
    asArray(result.suggestions).length > 0 ||
    collectionHasValue(result.analysis_summary) ||
    nonEmptyString(record.summary) ||
    asArray(batch.suggestions).length > 0
  );
}

export function artifactPassesValueGate(
  draft: AssistantArtifactDraft,
): boolean {
  switch (draft.kind) {
    case "evidence_sources":
      return hasRealEvidenceSourceValue(draft.payload);
    case "writing_change":
      return hasWritingChangeValue(draft.payload);
    case "structured_result":
      return hasStructuredResultValue(draft.payload);
    case "task_process":
      return hasProcessValue(draft.payload);
    case "session_evidence_detail":
      return true;
  }
}

function shouldDropKey(key: string): boolean {
  const normalized = key.replace(/[-_\s]/g, "").toLowerCase();
  return SENSITIVE_KEY_PATTERNS.some((pattern) =>
    normalized.includes(pattern.replace(/[-_\s]/g, "")),
  );
}

function redactArtifactPayload(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map((item) => redactArtifactPayload(item));
  }
  if (!isPlainObject(value)) {
    return value;
  }
  const next: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(value)) {
    if (shouldDropKey(key)) continue;
    next[key] = redactArtifactPayload(item);
  }
  return next;
}

export function sanitizeArtifactPayload(value: unknown): unknown {
  return sanitizePayloadForUi(
    getAiPayloadStore(),
    redactArtifactPayload(value),
  );
}

export function buildArtifactTab(
  draft: AssistantArtifactDraft,
  createdAt = new Date().toISOString(),
): ArtifactTab {
  return {
    id: `artifact:${draft.kind}:${draft.sourceRequestId}`,
    kind: draft.kind,
    title: draft.title,
    sourceRequestId: draft.sourceRequestId,
    payload: sanitizeArtifactPayload(draft.payload),
    createdAt,
    readonly: true,
    persistent: draft.persistent,
  };
}

interface HarnessArtifactWireLike {
  kind: string;
  title?: string;
  status?: string;
  sourceTask?: string;
  source_task?: string;
  evidenceCount?: number;
  evidence_count?: number;
  payload?: unknown;
}

interface TaskResultLike {
  requestId?: string;
  request_id?: string;
  runStatus?: string;
  run_status?: string;
  kind?: string;
  payload?: unknown;
  artifacts?: HarnessArtifactWireLike[];
}

const ARTIFACT_KIND_TITLES: Record<ArtifactKind, string> = {
  evidence_sources: "证据来源",
  writing_change: "写作修改",
  structured_result: "结构化结果",
  task_process: "过程详情",
  session_evidence_detail: "证据详情",
};

function isArtifactKind(kind: string): kind is ArtifactKind {
  return (
    kind === "evidence_sources" ||
    kind === "writing_change" ||
    kind === "structured_result" ||
    kind === "task_process" ||
    kind === "session_evidence_detail"
  );
}

function requestIdForResult(
  result: TaskResultLike,
  wire?: HarnessArtifactWireLike,
): string {
  return (
    result.requestId ??
    result.request_id ??
    wire?.sourceTask ??
    wire?.source_task ??
    "assistant-task"
  );
}

function payloadWithWireMetadata(wire: HarnessArtifactWireLike): unknown {
  if (!isPlainObject(wire.payload)) return wire.payload;
  return {
    ...wire.payload,
    evidence_count:
      wire.evidenceCount ?? wire.evidence_count ?? wire.payload.evidence_count,
  };
}

function draftFromWire(
  wire: HarnessArtifactWireLike,
  result: TaskResultLike,
): AssistantArtifactDraft | null {
  if (!isArtifactKind(wire.kind)) return null;
  const draft: AssistantArtifactDraft = {
    kind: wire.kind,
    title: wire.title ?? ARTIFACT_KIND_TITLES[wire.kind],
    sourceRequestId: requestIdForResult(result, wire),
    payload: payloadWithWireMetadata(wire),
  };
  return artifactPassesValueGate(draft) ? draft : null;
}

export function buildArtifactDraftsFromTaskResult(
  result: TaskResultLike,
): AssistantArtifactDraft[] {
  const wireDrafts = (result.artifacts ?? [])
    .map((wire) => draftFromWire(wire, result))
    .filter((draft): draft is AssistantArtifactDraft => draft !== null);
  if (wireDrafts.length > 0 || (result.artifacts?.length ?? 0) > 0) {
    return wireDrafts;
  }
  return [];
}

function isArtifactTab(value: unknown): value is ArtifactTab {
  if (!isPlainObject(value)) return false;
  return (
    typeof value.id === "string" &&
    value.id.startsWith("artifact:") &&
    typeof value.kind === "string" &&
    isArtifactKind(value.kind) &&
    typeof value.title === "string" &&
    typeof value.sourceRequestId === "string" &&
    typeof value.createdAt === "string" &&
    value.readonly === true &&
    value.persistent !== false
  );
}

export function loadArtifactTabsSnapshot(storage: Storage): ArtifactTab[] {
  try {
    const raw = storage.getItem(ARTIFACT_TAB_STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isArtifactTab).slice(-ARTIFACT_TAB_LIMIT);
  } catch {
    return [];
  }
}

export function saveArtifactTabsSnapshot(
  storage: Storage,
  tabs: ArtifactTab[],
): void {
  const next = tabs
    .filter((tab) => tab.persistent !== false)
    .slice(-ARTIFACT_TAB_LIMIT);
  storage.setItem(ARTIFACT_TAB_STORAGE_KEY, JSON.stringify(next));
}
