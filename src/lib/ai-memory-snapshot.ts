import { assistantContentHash } from "@/lib/assistant-stream-buffer";
import type { ChatLine } from "@/components/ai/AiMessageList";
import type { AiPayloadStoreSnapshot } from "@/lib/ai-payload-store";

export type AiMemorySnapshotPhase =
  | "stream_start"
  | "streaming"
  | "stream_done"
  | "idle_resume"
  | "stress_probe";

export interface AiMemorySnapshotInput {
  phase: AiMemorySnapshotPhase;
  messages: Pick<ChatLine, "role" | "content">[];
  streamLength: number;
  renderWindowLength: number;
  markdownCache: { entryCount: number; estimatedBytes: number };
  workerInFlightBytes: number;
  domTextLength: number;
  payloadStore?: AiPayloadStoreSnapshot;
  artifacts?: unknown[];
  packets?: unknown[];
  taskEvents?: unknown[];
  docSummaryLength?: number;
  researchResult?: unknown;
}

export interface AiMemorySnapshot {
  phase: AiMemorySnapshotPhase;
  timestampMs: number;
  messageCount: number;
  maxMessageLength: number;
  messageHashes: string[];
  streamLength: number;
  renderWindowLength: number;
  markdownCacheEntryCount: number;
  markdownCacheEstimatedBytes: number;
  workerInFlightBytes: number;
  domTextLength: number;
  payloadStoreEntryCount: number;
  payloadStoreEstimatedBytes: number;
  artifactCount: number;
  artifactEstimatedBytes: number;
  packetCount: number;
  packetEstimatedBytes: number;
  taskEventCount: number;
  taskEventEstimatedBytes: number;
  docSummaryLength: number;
  researchResultEstimatedBytes: number;
}

function estimateJsonBytes(value: unknown): number {
  try {
    return JSON.stringify(value)?.length ?? 0;
  } catch {
    return 0;
  }
}

export function createAiMemorySnapshot(
  input: AiMemorySnapshotInput,
): AiMemorySnapshot {
  const lengths = input.messages.map((message) => message.content.length);
  const artifacts = input.artifacts ?? [];
  const packets = input.packets ?? [];
  const taskEvents = input.taskEvents ?? [];
  return {
    phase: input.phase,
    timestampMs:
      typeof performance !== "undefined" ? performance.now() : Date.now(),
    messageCount: input.messages.length,
    maxMessageLength: lengths.length > 0 ? Math.max(...lengths) : 0,
    messageHashes: input.messages.map(
      (message) => `${message.role}:${assistantContentHash(message.content)}`,
    ),
    streamLength: input.streamLength,
    renderWindowLength: input.renderWindowLength,
    markdownCacheEntryCount: input.markdownCache.entryCount,
    markdownCacheEstimatedBytes: input.markdownCache.estimatedBytes,
    workerInFlightBytes: input.workerInFlightBytes,
    domTextLength: input.domTextLength,
    payloadStoreEntryCount: input.payloadStore?.entryCount ?? 0,
    payloadStoreEstimatedBytes: input.payloadStore?.totalEstimatedBytes ?? 0,
    artifactCount: artifacts.length,
    artifactEstimatedBytes: estimateJsonBytes(artifacts),
    packetCount: packets.length,
    packetEstimatedBytes: estimateJsonBytes(packets),
    taskEventCount: taskEvents.length,
    taskEventEstimatedBytes: estimateJsonBytes(taskEvents),
    docSummaryLength: input.docSummaryLength ?? 0,
    researchResultEstimatedBytes: estimateJsonBytes(input.researchResult),
  };
}

export function createAiStressPayload(sizeBytes: number, marker = "stress") {
  const size = Math.max(0, Math.floor(sizeBytes));
  const body = "X".repeat(size);
  return {
    assistantText: `${marker}:assistant:${body}`,
    evidencePacket: { id: `${marker}:packet`, excerpt: body },
    artifact: { kind: "structured_result", payload: { body } },
    taskEvent: { event_type: "progress", message: body },
  };
}
