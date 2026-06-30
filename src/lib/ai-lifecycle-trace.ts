export interface AiLifecycleContentSummary {
  empty: boolean;
  hash: string;
  length: number;
}

export interface AiLifecycleTraceEntry {
  candidateKind?: string;
  contentSummary?: AiLifecycleContentSummary;
  event: string;
  mutation?: string;
  nextSummary?: AiLifecycleContentSummary;
  phase: string;
  previousSummary?: AiLifecycleContentSummary;
  reasonKind?: string | null;
  reconcileReason?: string;
  requestId?: string | null;
  serverContentSummary?: AiLifecycleContentSummary;
  source?: string;
  streamBufferSummary?: AiLifecycleContentSummary;
  surface?: string;
  timestampMs: number;
}

export type AiLifecycleRecorder = (entry: AiLifecycleTraceEntry) => void;

export function summarizeLifecycleContent(
  value: string | null | undefined,
): AiLifecycleContentSummary {
  const text = value ?? "";
  let hash = 0x811c9dc5;

  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }

  return {
    empty: text.length === 0,
    hash: (hash >>> 0).toString(16).padStart(8, "0"),
    length: text.length,
  };
}

export function recordAiLifecycleEvent(
  recorder: AiLifecycleRecorder | undefined,
  entry: Omit<AiLifecycleTraceEntry, "timestampMs">,
) {
  if (!recorder) return;
  const timestampMs =
    typeof performance !== "undefined" ? performance.now() : Date.now();
  recorder({ ...entry, timestampMs });
}
