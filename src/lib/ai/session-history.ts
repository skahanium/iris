import type { ChatLine } from "@/components/ai/AiMessageList";
import type { ContextPacket } from "@/types/ai";
import type { SessionEvidenceRecord, SessionMessageRecord } from "@/types/ipc";

const SOURCE_TYPES = new Set<ContextPacket["source_type"]>([
  "note",
  "anchor",
  "regulation",
  "template",
  "session",
  "web",
]);

const TRUST_LEVELS = new Set<ContextPacket["trust_level"]>([
  "user_note",
  "derived_cache",
  "external_web",
  "model_generated",
]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringField(
  record: Record<string, unknown>,
  ...keys: string[]
): string | null {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string" && value.trim()) return value;
  }
  return null;
}

function nullableStringField(
  record: Record<string, unknown>,
  ...keys: string[]
): string | null {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string") return value;
    if (value === null) return null;
  }
  return null;
}

function numberField(
  record: Record<string, unknown>,
  key: string,
  fallback: number,
): number {
  const value = record[key];
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function booleanField(
  record: Record<string, unknown>,
  key: string,
  fallback: boolean,
): boolean {
  const value = record[key];
  return typeof value === "boolean" ? value : fallback;
}

function sourceTypeField(
  record: Record<string, unknown>,
): ContextPacket["source_type"] {
  const value = stringField(record, "source_type", "sourceType");
  return value && SOURCE_TYPES.has(value as ContextPacket["source_type"])
    ? (value as ContextPacket["source_type"])
    : "note";
}

function trustLevelField(
  record: Record<string, unknown>,
  sourceType: ContextPacket["source_type"],
): ContextPacket["trust_level"] {
  const value = stringField(record, "trust_level", "trustLevel");
  if (value && TRUST_LEVELS.has(value as ContextPacket["trust_level"])) {
    return value as ContextPacket["trust_level"];
  }
  return sourceType === "web" ? "external_web" : "user_note";
}

function sourceSpanField(
  record: Record<string, unknown>,
): ContextPacket["source_span"] {
  const value = record.source_span ?? record.sourceSpan;
  if (!isRecord(value)) return null;
  const start = value.start;
  const end = value.end;
  if (typeof start !== "number" || typeof end !== "number") return null;
  return { start, end };
}

function webField(record: Record<string, unknown>): ContextPacket["web"] {
  const value = record.web;
  if (!isRecord(value)) return undefined;
  const sourceRank = stringField(value, "source_rank", "sourceRank");
  const fallbackFrom = nullableStringField(
    value,
    "fallback_from",
    "fallbackFrom",
  );
  return {
    url: nullableStringField(value, "url"),
    domain: nullableStringField(value, "domain"),
    published_at: nullableStringField(value, "published_at", "publishedAt"),
    fetched_at:
      stringField(value, "fetched_at", "fetchedAt") ??
      new Date(0).toISOString(),
    search_backend: "provider",
    source_rank:
      sourceRank === "official" ||
      sourceRank === "academic" ||
      sourceRank === "media" ||
      sourceRank === "community"
        ? sourceRank
        : "unknown",
    failure_reason: nullableStringField(
      value,
      "failure_reason",
      "failureReason",
    ),
    fallback_from: fallbackFrom === "provider" ? fallbackFrom : null,
  };
}

function normalizeContextPacket(
  value: unknown,
  index: number,
): ContextPacket | null {
  if (!isRecord(value)) return null;
  const citationLabel =
    stringField(value, "citation_label", "citationLabel") ?? `[C${index + 1}]`;
  const sourceType = sourceTypeField(value);
  return {
    id: stringField(value, "id") ?? citationLabel,
    source_type: sourceType,
    source_path: nullableStringField(value, "source_path", "sourcePath"),
    title:
      stringField(value, "title") ??
      nullableStringField(value, "source_path", "sourcePath") ??
      citationLabel,
    heading_path: nullableStringField(value, "heading_path", "headingPath"),
    source_span: sourceSpanField(value),
    content_hash: stringField(value, "content_hash", "contentHash") ?? "",
    excerpt: nullableStringField(value, "excerpt") ?? "",
    retrieval_reason:
      stringField(value, "retrieval_reason", "retrievalReason") ??
      "session_history",
    score: numberField(value, "score", 0),
    trust_level: trustLevelField(value, sourceType),
    citation_label: citationLabel,
    stale: booleanField(value, "stale", false),
    web: webField(value),
  };
}

export function normalizeEvidencePackets(
  value: unknown,
): ContextPacket[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const packets = value
    .map((packet, index) => normalizeContextPacket(packet, index))
    .filter((packet): packet is ContextPacket => packet !== null);
  return packets.length > 0 ? packets : undefined;
}
export function toChatLines(records: SessionMessageRecord[]): ChatLine[] {
  return records
    .filter(
      (message) =>
        message.role === "user" ||
        message.role === "assistant" ||
        message.role === "system",
    )
    .map((message) => ({
      role: message.role as ChatLine["role"],
      content: message.content,
      evidencePackets: normalizeEvidencePackets(message.evidence_packets),
      seq: message.seq,
      created_at: message.created_at,
    }));
}

export function evidenceRecordsToContextPackets(
  records: SessionEvidenceRecord[],
): ContextPacket[] {
  return records.map((record) => ({
    id: record.packetKey,
    source_type: record.sourceType === "web" ? "web" : "note",
    source_path:
      record.sourceType === "web"
        ? (record.url ?? null)
        : (record.sourcePath ?? null),
    title: record.title,
    heading_path: record.headingPath ?? null,
    source_span:
      record.sourceSpanStart != null && record.sourceSpanEnd != null
        ? { start: record.sourceSpanStart, end: record.sourceSpanEnd }
        : null,
    content_hash: record.contentHash ?? "",
    excerpt: "",
    retrieval_reason: record.retrievalReason ?? "session_evidence",
    score: record.score ?? 0,
    trust_level: record.sourceType === "web" ? "external_web" : "user_note",
    citation_label: record.citationLabel,
    stale: false,
    web:
      record.sourceType === "web"
        ? {
            url: record.url ?? null,
            domain: record.domain ?? null,
            published_at: null,
            fetched_at: record.retrievedAt ?? record.createdAt,
            search_backend: "provider",
            source_rank: "unknown",
            failure_reason: record.failureReason ?? null,
          }
        : undefined,
  }));
}
