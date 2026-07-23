import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  assistantRunControl,
  assistantRunGet,
  assistantRunRetry,
  assistantRunStart,
  listenAssistantRunEvent,
  listenAssistantRunPresentation,
} from "@/lib/ipc";
import {
  createAssistantPresentationState,
  reduceAssistantPresentationEvent,
  type AssistantPresentationEvent,
  type AssistantPresentationState,
} from "@/lib/assistant-presentation";
import {
  deriveRunOutputting,
  isTerminalRunState,
} from "@/lib/assistant-run-activity";
import {
  createAssistantRunEventState,
  reduceAssistantRunEvent,
  replayAssistantRunEvents,
  type AssistantRunEventState,
} from "@/lib/assistant-run-events";
import { invokeErrorMessage } from "@/lib/credentials";
import type {
  AssistantRunEvent,
  AssistantRunGetResponse,
  AssistantRunAccepted,
  AssistantRunStartRequest,
  AssistantSessionRef,
  PendingConfirmation,
  RunState,
} from "@/types/ai";

function isStateVersionConflict(error: unknown): boolean {
  const message = invokeErrorMessage(error);
  return message.includes("agent_run_state_version_conflict");
}

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
  const [presentationState, setPresentationState] =
    useState<AssistantPresentationState | null>(null);
  const activeRunIdRef = useRef<string | null>(null);
  const currentRunRef = useRef<ActiveAssistantRun | null>(null);
  const earlyEventsRef = useRef(new Map<string, AssistantRunEvent[]>());
  const earlyPresentationRef = useRef(
    new Map<string, AssistantPresentationEvent[]>(),
  );
  const resyncingRef = useRef(new Set<string>());
  const answerCompleteResyncRef = useRef<string | null>(null);

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
  const isBusy = deriveRunOutputting(
    currentRun ? { runId: currentRun.runId, state: currentRun.state } : null,
    presentationState,
  );
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

  const reducePresentationEvent = useCallback(
    (event: AssistantPresentationEvent) => {
      if (activeRunIdRef.current !== event.runId) {
        const buffered = earlyPresentationRef.current.get(event.runId) ?? [];
        earlyPresentationRef.current.set(event.runId, [...buffered, event]);
        return;
      }
      setPresentationState((previous) => {
        if (!previous || previous.runId !== event.runId) return previous;
        return reduceAssistantPresentationEvent(previous, event);
      });
    },
    [],
  );

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listenAssistantRunPresentation((event) => {
      if (!disposed) reducePresentationEvent(event);
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [reducePresentationEvent]);

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
      let presentation = createAssistantPresentationState(accepted.runId);
      for (const event of earlyPresentationRef.current.get(accepted.runId) ??
        []) {
        presentation = reduceAssistantPresentationEvent(presentation, event);
      }
      earlyPresentationRef.current.delete(accepted.runId);
      setPresentationState(presentation);
      setLatestEvent(replayState.events.at(-1) ?? null);
      answerCompleteResyncRef.current = null;
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

  const cancel = useCallback(async (): Promise<string | null> => {
    const run = currentRunRef.current;
    if (!run) return null;

    const submitCancel = async (target: ActiveAssistantRun) => {
      await assistantRunControl({
        session: target.session,
        runId: target.runId,
        expectedStateVersion: target.stateVersion,
        action: { type: "cancel" },
      });
    };

    const resyncFromPersisted = async (
      target: ActiveAssistantRun,
    ): Promise<ActiveAssistantRun | null> => {
      const persisted = await assistantRunGet({
        session: target.session,
        runId: target.runId,
      });
      if (!persisted || activeRunIdRef.current !== target.runId) return null;
      const refreshed: ActiveAssistantRun = {
        runId: persisted.run.runId,
        turnId: persisted.run.turnId,
        session: persisted.run.session,
        state: persisted.run.state,
        stateVersion: persisted.run.stateVersion,
      };
      setRunIdentity(refreshed);
      setEventState(
        replayAssistantRunEvents(refreshed.runId, persisted.events),
      );
      setLatestEvent(persisted.events.at(-1) ?? null);
      currentRunRef.current = refreshed;
      return refreshed;
    };

    try {
      await submitCancel(run);
      return null;
    } catch (error) {
      if (!isStateVersionConflict(error)) throw error;
      const refreshed = await resyncFromPersisted(run);
      if (!refreshed) {
        return "停止请求已过期，请刷新后再试。";
      }
      if (isTerminalRunState(refreshed.state)) {
        return null;
      }
      try {
        await submitCancel(refreshed);
        return null;
      } catch (retryError) {
        if (isStateVersionConflict(retryError)) {
          const settled = await resyncFromPersisted(refreshed);
          if (settled && isTerminalRunState(settled.state)) return null;
          return "停止失败：运行状态已变化，请稍后重试。";
        }
        throw retryError;
      }
    }
  }, []);

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
    setPresentationState(createAssistantPresentationState(run.runId));
    setLatestEvent(replayed.events.at(-1) ?? null);
  }, []);

  const reset = useCallback(() => {
    activeRunIdRef.current = null;
    answerCompleteResyncRef.current = null;
    setRunIdentity(null);
    setEventState(null);
    setLatestEvent(null);
    setPresentationState(null);
  }, []);

  // Presentation finished but durable completed may be missing — resync once.
  useEffect(() => {
    if (!currentRun || !presentationState) return;
    if (presentationState.runId !== currentRun.runId) return;
    if (!presentationState.answerComplete) return;
    if (isTerminalRunState(currentRun.state)) return;
    const key = `${currentRun.runId}:${presentationState.lastSeq}`;
    if (answerCompleteResyncRef.current === key) return;
    answerCompleteResyncRef.current = key;
    void replay(currentRun);
  }, [currentRun, presentationState, replay]);

  return {
    runState,
    snapshot,
    isBusy,
    activityHint,
    currentRun,
    latestEvent,
    eventState,
    presentationState,
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
