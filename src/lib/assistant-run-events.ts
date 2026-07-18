import type {
  AssistantRunEvent,
  AssistantRunEventPayload,
  PendingConfirmation,
  ProviderSwitchReasonCode,
  RunState,
} from "@/types/ai";

/** Reducer state reconstructed exclusively from persisted or streamed Run events. */
export interface AssistantRunEventState {
  runId: string;
  lastSeq: number;
  stateVersion: number;
  state: RunState | null;
  stage: string | null;
  summary: string | null;
  capabilityDegradation: Extract<
    AssistantRunEventPayload,
    { kind: "capability_degraded" }
  > | null;
  webVerificationFailure: Extract<
    AssistantRunEventPayload,
    { kind: "web_verification_failed" }
  > | null;
  pendingConfirmation: PendingConfirmation | null;
  provider: {
    providerId: string;
    modelId: string | null;
    reasonCode: ProviderSwitchReasonCode | null;
  } | null;
  content: string;
  /** Monotonic local-only revision for transient, non-replayable UI content. */
  transientRevision: number;
  /** Whether `content` currently came from an uncommitted provider stream. */
  hasTransientContent: boolean;
  events: readonly AssistantRunEvent[];
  pendingEvents: readonly AssistantRunEvent[];
  /** The first missing sequence to request through `assistant_run_get`. */
  resyncFromSeq: number | null;
}

const TERMINAL_STATES = new Set<RunState>(["completed", "failed", "cancelled"]);

/** Create the empty replay state for exactly one Run. */
export function createAssistantRunEventState(
  runId: string,
): AssistantRunEventState {
  return {
    runId,
    lastSeq: 0,
    stateVersion: 0,
    state: null,
    stage: null,
    summary: null,
    capabilityDegradation: null,
    webVerificationFailure: null,
    pendingConfirmation: null,
    provider: null,
    content: "",
    transientRevision: 0,
    hasTransientContent: false,
    events: [],
    pendingEvents: [],
    resyncFromSeq: null,
  };
}

/** Reduce one Run event without side effects or assumptions about missing events. */
export function reduceAssistantRunEvent(
  state: AssistantRunEventState,
  event: AssistantRunEvent,
): AssistantRunEventState {
  if (
    event.runId === state.runId &&
    event.seq === 0 &&
    event.type === "content_delta" &&
    event.payload.kind === "content_delta" &&
    !isTerminal(state.state)
  ) {
    if (state.hasTransientContent && state.content === event.payload.delta) {
      return state;
    }
    return {
      ...state,
      content: event.payload.delta,
      transientRevision: state.transientRevision + 1,
      hasTransientContent: true,
    };
  }

  if (
    event.runId !== state.runId ||
    event.type !== event.payload.kind ||
    !Number.isSafeInteger(event.seq) ||
    event.seq < 1 ||
    event.seq <= state.lastSeq ||
    state.pendingEvents.some((pending) => pending.seq === event.seq) ||
    isTerminal(state.state)
  ) {
    return state;
  }

  if (event.seq > state.lastSeq + 1) {
    return {
      ...state,
      pendingEvents: sortBySeq([...state.pendingEvents, event]),
      resyncFromSeq: state.lastSeq + 1,
    };
  }

  return applyContiguousEvent(state, event);
}

/** Replay persisted events after reconnecting; their receive order is irrelevant. */
export function replayAssistantRunEvents(
  runId: string,
  events: readonly AssistantRunEvent[],
): AssistantRunEventState {
  return sortBySeq(events).reduce(
    reduceAssistantRunEvent,
    createAssistantRunEventState(runId),
  );
}

function applyContiguousEvent(
  state: AssistantRunEventState,
  event: AssistantRunEvent,
): AssistantRunEventState {
  const next = applyEvent(state, event);
  if (isTerminal(next.state)) {
    return {
      ...next,
      pendingEvents: [],
      resyncFromSeq: null,
    };
  }

  const pending = next.pendingEvents.find(
    (candidate) => candidate.seq === next.lastSeq + 1,
  );
  if (!pending) {
    return {
      ...next,
      resyncFromSeq: next.pendingEvents.length === 0 ? null : next.lastSeq + 1,
    };
  }

  return applyContiguousEvent(
    {
      ...next,
      pendingEvents: next.pendingEvents.filter(
        (candidate) => candidate.seq !== pending.seq,
      ),
    },
    pending,
  );
}

function applyEvent(
  state: AssistantRunEventState,
  event: AssistantRunEvent,
): AssistantRunEventState {
  const nextState = stateForEvent(event, state.state);
  const payload = event.payload;
  const clearsTransientContent =
    state.hasTransientContent &&
    (payload.kind === "failed" || payload.kind === "cancelled");
  const commitsTransientContent =
    state.hasTransientContent && payload.kind === "content_delta";

  return {
    ...state,
    lastSeq: event.seq,
    stateVersion: Math.max(state.stateVersion, event.stateVersion),
    state: nextState,
    stage: payload.kind === "stage_changed" ? payload.stage : state.stage,
    summary: summaryForPayload(payload) ?? state.summary,
    capabilityDegradation:
      payload.kind === "capability_degraded"
        ? payload
        : state.capabilityDegradation,
    webVerificationFailure:
      payload.kind === "web_verification_failed"
        ? payload
        : state.webVerificationFailure,
    pendingConfirmation:
      payload.kind === "confirmation_required"
        ? confirmationForPayload(payload)
        : payload.kind === "resumed" ||
            payload.kind === "completed" ||
            payload.kind === "failed" ||
            payload.kind === "cancelled"
          ? null
          : state.pendingConfirmation,
    provider:
      payload.kind === "provider_switched"
        ? {
            providerId: payload.providerId,
            modelId: payload.modelId ?? null,
            reasonCode: payload.reasonCode ?? null,
          }
        : state.provider,
    content:
      payload.kind === "content_delta"
        ? commitsTransientContent
          ? payload.delta
          : `${state.content}${payload.delta}`
        : clearsTransientContent
          ? ""
          : state.content,
    transientRevision:
      clearsTransientContent || commitsTransientContent
        ? state.transientRevision + 1
        : state.transientRevision,
    hasTransientContent:
      payload.kind === "content_delta" || clearsTransientContent
        ? false
        : state.hasTransientContent,
    events: [...state.events, event],
  };
}

function confirmationForPayload(
  payload: Extract<AssistantRunEventPayload, { kind: "confirmation_required" }>,
): PendingConfirmation {
  return {
    confirmationId: payload.confirmationId,
    planHash: payload.planHash,
    summary: payload.summary,
    ...(payload.effect ? { effect: payload.effect } : {}),
    ...(payload.targets ? { targets: payload.targets } : {}),
    ...(payload.expiresAt ? { expiresAt: payload.expiresAt } : {}),
  };
}

function stateForEvent(
  event: AssistantRunEvent,
  current: RunState | null,
): RunState | null {
  const requested = requestedState(event.type, event.payload);
  if (!requested || !canTransition(current, requested)) return current;
  return requested;
}

function requestedState(
  type: AssistantRunEvent["type"],
  payload: AssistantRunEventPayload,
): RunState | null {
  if (type !== payload.kind) return null;

  switch (type) {
    case "accepted":
      return "accepted";
    case "stage_changed":
      return payload.kind === "stage_changed" ? payload.state : null;
    case "confirmation_required":
      return "awaiting_confirmation";
    case "paused":
      return "paused";
    case "resumed":
      return "running";
    case "completed":
      return "completed";
    case "failed":
      return "failed";
    case "cancelled":
      return "cancelled";
    default:
      return null;
  }
}

function summaryForPayload(payload: AssistantRunEventPayload): string | null {
  switch (payload.kind) {
    case "tool_completed":
    case "confirmation_required":
      return payload.summary;
    case "capability_degraded":
      return payload.message;
    case "web_verification_failed":
      return "联网核实未取得可用证据";
    case "permission_denied":
    case "failed":
      return payload.message;
    case "paused":
    case "resumed":
    case "cancelled":
      return payload.reason;
    default:
      return null;
  }
}

function canTransition(current: RunState | null, next: RunState): boolean {
  if (current === next) return true;
  if (current === null) return next === "accepted";
  if (isTerminal(current)) return false;

  return (
    (current === "accepted" && next === "preparing") ||
    (current === "preparing" &&
      (next === "running" || next === "failed" || next === "cancelled")) ||
    (current === "running" &&
      (next === "awaiting_confirmation" ||
        next === "paused" ||
        next === "verifying" ||
        next === "completed" ||
        next === "failed" ||
        next === "cancelled")) ||
    (current === "awaiting_confirmation" && next === "running") ||
    (current === "paused" && next === "running") ||
    (current === "verifying" &&
      (next === "paused" ||
        next === "completed" ||
        next === "failed" ||
        next === "cancelled"))
  );
}

function isTerminal(state: RunState | null): boolean {
  return state !== null && TERMINAL_STATES.has(state);
}

function sortBySeq(events: readonly AssistantRunEvent[]): AssistantRunEvent[] {
  return [...events].sort((left, right) => left.seq - right.seq);
}
