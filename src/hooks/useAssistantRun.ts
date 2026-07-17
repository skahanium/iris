import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  assistantRunControl,
  assistantRunGet,
  assistantRunRetry,
  assistantRunStart,
  listenAssistantRunEvent,
} from "@/lib/ipc";
import {
  createAssistantRunEventState,
  reduceAssistantRunEvent,
  replayAssistantRunEvents,
  type AssistantRunEventState,
} from "@/lib/assistant-run-events";
import type {
  AssistantRunEvent,
  AssistantRunGetResponse,
  AssistantRunAccepted,
  AssistantRunStartRequest,
  AssistantSessionRef,
  PendingConfirmation,
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

export interface AssistantRunConfirmation extends PendingConfirmation {
  runId: string;
  stateVersion: number;
}

export function isAssistantRunBusy(runState: AssistantRunState): boolean {
  return ["accepted", "preparing", "running", "verifying"].includes(runState);
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
  const [runIdentity, setRunIdentity] = useState<ActiveAssistantRun | null>(
    null,
  );
  const [eventState, setEventState] = useState<AssistantRunEventState | null>(
    null,
  );
  const [latestEvent, setLatestEvent] = useState<AssistantRunEvent | null>(
    null,
  );
  const activeRunIdRef = useRef<string | null>(null);
  const currentRunRef = useRef<ActiveAssistantRun | null>(null);
  const earlyEventsRef = useRef(new Map<string, AssistantRunEvent[]>());
  const resyncingRef = useRef(new Set<string>());

  const currentRun = useMemo<ActiveAssistantRun | null>(() => {
    if (!runIdentity) return null;
    if (!eventState || eventState.runId !== runIdentity.runId)
      return runIdentity;
    return {
      ...runIdentity,
      state: eventState.state ?? runIdentity.state,
      stateVersion: Math.max(runIdentity.stateVersion, eventState.stateVersion),
    };
  }, [eventState, runIdentity]);
  activeRunIdRef.current = currentRun?.runId ?? null;
  currentRunRef.current = currentRun;

  const runState: AssistantRunState = currentRun?.state ?? "idle";
  const activityHint = eventState?.stage ?? null;
  const snapshot: AssistantRunSnapshot = useMemo(
    () => ({ runState, activityHint, currentRun }),
    [activityHint, currentRun, runState],
  );
  const isBusy = isAssistantRunBusy(runState);
  const pendingConfirmation = useMemo<AssistantRunConfirmation | null>(() => {
    if (!currentRun || runState !== "awaiting_confirmation") return null;
    const confirmation = eventState?.pendingConfirmation;
    if (!confirmation) return null;
    return {
      ...confirmation,
      runId: currentRun.runId,
      stateVersion: currentRun.stateVersion,
    };
  }, [currentRun, eventState?.pendingConfirmation, runState]);

  const replay = useCallback(async (run: ActiveAssistantRun) => {
    if (resyncingRef.current.has(run.runId)) return;
    resyncingRef.current.add(run.runId);
    try {
      const persisted = await assistantRunGet({
        session: run.session,
        runId: run.runId,
      });
      if (!persisted || activeRunIdRef.current !== run.runId) return;
      setEventState(replayAssistantRunEvents(run.runId, persisted.events));
      setLatestEvent(persisted.events.at(-1) ?? null);
    } finally {
      resyncingRef.current.delete(run.runId);
    }
  }, []);

  const reduceLiveEvent = useCallback(
    (event: AssistantRunEvent) => {
      if (activeRunIdRef.current !== event.runId) {
        const buffered = earlyEventsRef.current.get(event.runId) ?? [];
        earlyEventsRef.current.set(event.runId, [...buffered, event]);
        return;
      }
      setEventState((previous) => {
        if (!previous || previous.runId !== event.runId) return previous;
        const next = reduceAssistantRunEvent(previous, event);
        if (next.resyncFromSeq !== null) {
          const active = currentRunRef.current;
          if (active?.runId === event.runId) void replay(active);
        }
        return next;
      });
      setLatestEvent(event);
    },
    [replay],
  );

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listenAssistantRunEvent((event) => {
      if (!disposed) reduceLiveEvent(event);
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [reduceLiveEvent]);

  const activateAccepted = useCallback(
    (accepted: AssistantRunAccepted) => {
      const run = activeRunFromAccepted(accepted);
      activeRunIdRef.current = accepted.runId;
      setRunIdentity(run);
      let replayState = createAssistantRunEventState(accepted.runId);
      replayState = reduceAssistantRunEvent(replayState, {
        runId: accepted.runId,
        seq: 1,
        stateVersion: accepted.stateVersion,
        timestamp: new Date().toISOString(),
        type: "accepted",
        payload: {
          kind: "accepted",
          turnId: accepted.turnId,
          sessionKey: accepted.session.sessionKey,
        },
      });
      for (const event of earlyEventsRef.current.get(accepted.runId) ?? []) {
        replayState = reduceAssistantRunEvent(replayState, event);
      }
      earlyEventsRef.current.delete(accepted.runId);
      setEventState(replayState);
      setLatestEvent(replayState.events.at(-1) ?? null);
      void replay(run);
      return accepted;
    },
    [replay],
  );

  const start = useCallback(
    async (request: AssistantRunStartRequest) => {
      const accepted = await assistantRunStart(request);
      return activateAccepted(accepted);
    },
    [activateAccepted],
  );

  const retryWebVerification = useCallback(async () => {
    const run = currentRun;
    const failure = eventState?.webVerificationFailure;
    if (!run || !failure || !failure.retryable) return null;
    const accepted = await assistantRunRetry({
      session: run.session,
      sourceRunId: run.runId,
      clientRequestId: crypto.randomUUID(),
    });
    return activateAccepted(accepted);
  }, [activateAccepted, currentRun, eventState?.webVerificationFailure]);

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

  const approveChange = useCallback(async () => {
    const run = currentRun;
    const confirmation = pendingConfirmation;
    if (!run || !confirmation) return;
    await assistantRunControl({
      session: run.session,
      runId: run.runId,
      expectedStateVersion: run.stateVersion,
      action: {
        type: "approve_change",
        confirmationId: confirmation.confirmationId,
        planHash: confirmation.planHash,
      },
    });
  }, [currentRun, pendingConfirmation]);

  const rejectChange = useCallback(async () => {
    const run = currentRun;
    const confirmation = pendingConfirmation;
    if (!run || !confirmation) return;
    await assistantRunControl({
      session: run.session,
      runId: run.runId,
      expectedStateVersion: run.stateVersion,
      action: {
        type: "reject_change",
        confirmationId: confirmation.confirmationId,
      },
    });
  }, [currentRun, pendingConfirmation]);

  const recover = useCallback((persisted: AssistantRunGetResponse) => {
    const run: ActiveAssistantRun = {
      runId: persisted.run.runId,
      turnId: persisted.run.turnId,
      session: persisted.run.session,
      state: persisted.run.state,
      stateVersion: persisted.run.stateVersion,
    };
    activeRunIdRef.current = run.runId;
    setRunIdentity(run);
    const replayed = replayAssistantRunEvents(run.runId, persisted.events);
    setEventState(replayed);
    setLatestEvent(replayed.events.at(-1) ?? null);
  }, []);

  const reset = useCallback(() => {
    activeRunIdRef.current = null;
    setRunIdentity(null);
    setEventState(null);
    setLatestEvent(null);
  }, []);

  return {
    runState,
    snapshot,
    isBusy,
    activityHint,
    currentRun,
    latestEvent,
    eventState,
    pendingConfirmation,
    start,
    retryWebVerification,
    cancel,
    approveChange,
    rejectChange,
    recover,
    reset,
  };
}
