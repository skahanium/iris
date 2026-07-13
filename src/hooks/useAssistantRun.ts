import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  assistantRunControl,
  assistantRunStart,
  listenAssistantRunEvent,
} from "@/lib/ipc";
import type {
  AssistantRunEvent,
  AssistantRunStartRequest,
  AssistantSessionRef,
  RunState,
} from "@/types/ai";

/** UI state is the persisted Run state, with `idle` before a Run exists. */
export type AssistantRunState = RunState | "idle";

export interface ActiveAssistantRun {
  runId: string;
  turnId: string;
  session: AssistantSessionRef;
  state: RunState;
  stateVersion: number;
}

export interface AssistantRunSnapshot {
  runState: AssistantRunState;
  activityHint: string | null;
  currentRun: ActiveAssistantRun | null;
}

export function isAssistantRunBusy(runState: AssistantRunState): boolean {
  return ["accepted", "preparing", "running", "verifying"].includes(runState);
}

function stateFromEvent(
  event: AssistantRunEvent,
  previous: RunState,
): RunState {
  if (event.payload.kind === "stage_changed") return event.payload.state;
  if (event.type === "completed") return "completed";
  if (event.type === "failed") return "failed";
  if (event.type === "cancelled") return "cancelled";
  if (event.type === "paused") return "paused";
  if (event.type === "resumed") return "running";
  return previous;
}

function activityHintFromEvent(event: AssistantRunEvent): string | null {
  return event.payload.kind === "stage_changed" ? event.payload.stage : null;
}

function activeRunFromAccepted(
  accepted: Awaited<ReturnType<typeof assistantRunStart>>,
): ActiveAssistantRun {
  return {
    runId: accepted.runId,
    turnId: accepted.turnId,
    session: accepted.session,
    state: accepted.state,
    stateVersion: accepted.stateVersion,
  };
}

/** One reducer-backed controller for the unified Agent Run lifecycle. */
export function useAssistantRun() {
  const [currentRun, setCurrentRun] = useState<ActiveAssistantRun | null>(null);
  const [latestEvent, setLatestEvent] = useState<AssistantRunEvent | null>(
    null,
  );
  const [activityHint, setActivityHint] = useState<string | null>(null);
  const activeRunIdRef = useRef<string | null>(null);
  activeRunIdRef.current = currentRun?.runId ?? activeRunIdRef.current;

  const runState: AssistantRunState = currentRun?.state ?? "idle";
  const snapshot: AssistantRunSnapshot = useMemo(
    () => ({ runState, activityHint, currentRun }),
    [activityHint, currentRun, runState],
  );
  const isBusy = isAssistantRunBusy(runState);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listenAssistantRunEvent((event) => {
      if (disposed || activeRunIdRef.current !== event.runId) return;
      setLatestEvent(event);
      setCurrentRun((previous) => {
        if (!previous || previous.runId !== event.runId) return previous;
        const state = stateFromEvent(event, previous.state);
        return {
          ...previous,
          state,
          stateVersion: event.stateVersion,
        };
      });

      const hint = activityHintFromEvent(event);
      if (hint !== null) setActivityHint(hint);
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const start = useCallback(async (request: AssistantRunStartRequest) => {
    const accepted = await assistantRunStart(request);
    activeRunIdRef.current = accepted.runId;
    setCurrentRun(activeRunFromAccepted(accepted));
    setLatestEvent(null);
    setActivityHint(null);
    return accepted;
  }, []);

  const cancel = useCallback(async () => {
    const run = currentRun;
    if (!run) return;
    await assistantRunControl({
      session: run.session,
      runId: run.runId,
      expectedStateVersion: run.stateVersion,
      action: { type: "cancel" },
    });
  }, [currentRun]);

  const reset = useCallback(() => {
    setActivityHint(null);
    activeRunIdRef.current = null;
    setCurrentRun(null);
    setLatestEvent(null);
  }, []);

  return {
    runState,
    snapshot,
    isBusy,
    activityHint,
    currentRun,
    latestEvent,
    start,
    cancel,
    reset,
  };
}
