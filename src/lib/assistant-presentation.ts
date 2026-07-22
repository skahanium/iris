/**
 * Ephemeral, ordered presentation protocol for one live Assistant Run.
 *
 * These events are intentionally separate from durable Run facts: a dropped
 * presentation event may affect motion, but must never affect the final answer
 * reconstructed from `assistant_run_get`.
 */
export type AssistantPresentationPayload =
  | {
      kind: "process_started";
      itemId: string;
      itemKind: "stage" | "reasoning_summary" | "tool";
      label: string;
    }
  | { kind: "process_updated"; itemId: string; label: string }
  | {
      kind: "process_finished";
      itemId: string;
      status: "completed" | "failed";
      durationMs?: number;
    }
  | { kind: "answer_delta"; delta: string }
  | { kind: "answer_reset" }
  | { kind: "answer_complete" };

export interface AssistantPresentationEvent {
  runId: string;
  presentationSeq: number;
  elapsedMs: number;
  type: AssistantPresentationPayload["kind"];
  payload: AssistantPresentationPayload;
}

export interface AssistantPresentationItem {
  id: string;
  kind: "stage" | "reasoning_summary" | "tool";
  label: string;
  status: "running" | "completed" | "failed";
  elapsedMs: number;
  durationMs?: number;
}

export interface AssistantPresentationState {
  runId: string;
  lastSeq: number;
  resyncFromSeq: number | null;
  pendingEvents: readonly AssistantPresentationEvent[];
  processItems: readonly AssistantPresentationItem[];
  answer: string;
  answerComplete: boolean;
}

/** Create isolated transient state for exactly one live Run. */
export function createAssistantPresentationState(
  runId: string,
): AssistantPresentationState {
  return {
    runId,
    lastSeq: 0,
    resyncFromSeq: null,
    pendingEvents: [],
    processItems: [],
    answer: "",
    answerComplete: false,
  };
}

/**
 * Consume only contiguous presentation events. Gaps are buffered so a late IPC
 * delivery cannot splice text out of order into the visual answer.
 */
export function reduceAssistantPresentationEvent(
  state: AssistantPresentationState,
  event: AssistantPresentationEvent,
): AssistantPresentationState {
  if (
    event.runId !== state.runId ||
    !Number.isSafeInteger(event.presentationSeq) ||
    event.presentationSeq < 1 ||
    event.presentationSeq <= state.lastSeq ||
    state.pendingEvents.some(
      (candidate) => candidate.presentationSeq === event.presentationSeq,
    )
  ) {
    return state;
  }
  if (event.presentationSeq > state.lastSeq + 1) {
    return {
      ...state,
      pendingEvents: [...state.pendingEvents, event].sort(
        (left, right) => left.presentationSeq - right.presentationSeq,
      ),
      resyncFromSeq: state.lastSeq + 1,
    };
  }
  return applyContiguous(state, event);
}

function applyContiguous(
  state: AssistantPresentationState,
  event: AssistantPresentationEvent,
): AssistantPresentationState {
  const next = applyEvent(state, event);
  const pending = next.pendingEvents.find(
    (candidate) => candidate.presentationSeq === next.lastSeq + 1,
  );
  if (!pending) {
    return {
      ...next,
      resyncFromSeq: next.pendingEvents.length === 0 ? null : next.lastSeq + 1,
    };
  }
  return applyContiguous(
    {
      ...next,
      pendingEvents: next.pendingEvents.filter(
        (candidate) => candidate.presentationSeq !== pending.presentationSeq,
      ),
    },
    pending,
  );
}

function applyEvent(
  state: AssistantPresentationState,
  event: AssistantPresentationEvent,
): AssistantPresentationState {
  const payload = event.payload;
  switch (payload.kind) {
    case "process_started": {
      const existing = state.processItems.find(
        (item) => item.id === payload.itemId,
      );
      if (existing) {
        return {
          ...state,
          lastSeq: event.presentationSeq,
          processItems: state.processItems.map((item) =>
            item.id === payload.itemId
              ? { ...item, label: payload.label, status: "running" }
              : item,
          ),
        };
      }
      const completedPreviousStages =
        payload.itemKind === "stage"
          ? state.processItems.map((item) =>
              item.kind === "stage" && item.status === "running"
                ? { ...item, status: "completed" as const }
                : item,
            )
          : state.processItems;
      return {
        ...state,
        lastSeq: event.presentationSeq,
        processItems: [
          ...completedPreviousStages,
          {
            id: payload.itemId,
            kind: payload.itemKind,
            label: payload.label,
            status: "running",
            elapsedMs: event.elapsedMs,
          },
        ],
      };
    }
    case "process_updated":
      return {
        ...state,
        lastSeq: event.presentationSeq,
        processItems: state.processItems.map((item) =>
          item.id === payload.itemId
            ? { ...item, label: payload.label, status: "running" }
            : item,
        ),
      };
    case "process_finished":
      return {
        ...state,
        lastSeq: event.presentationSeq,
        processItems: state.processItems.map((item) =>
          item.id === payload.itemId
            ? {
                ...item,
                status: payload.status,
                ...(typeof payload.durationMs === "number"
                  ? { durationMs: payload.durationMs }
                  : {}),
              }
            : item,
        ),
      };
    case "answer_delta":
      return {
        ...state,
        lastSeq: event.presentationSeq,
        processItems: completeRunningNonTools(state.processItems),
        answer: `${state.answer}${payload.delta}`,
      };
    case "answer_reset":
      return {
        ...state,
        lastSeq: event.presentationSeq,
        answer: "",
        answerComplete: false,
      };
    case "answer_complete":
      return {
        ...state,
        lastSeq: event.presentationSeq,
        processItems: completeAllRunning(state.processItems),
        answerComplete: true,
      };
  }
}

function completeRunningNonTools(
  items: readonly AssistantPresentationItem[],
): AssistantPresentationItem[] {
  return items.map((item) =>
    item.kind !== "tool" && item.status === "running"
      ? { ...item, status: "completed" }
      : item,
  );
}

function completeAllRunning(
  items: readonly AssistantPresentationItem[],
): AssistantPresentationItem[] {
  return items.map((item) =>
    item.status === "running" ? { ...item, status: "completed" } : item,
  );
}
