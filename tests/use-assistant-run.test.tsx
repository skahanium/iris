import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useAssistantRun } from "@/hooks/useAssistantRun";
import {
  assistantRunControl,
  assistantRunGet,
  assistantRunRetry,
  assistantRunStart,
  listenAssistantRunEvent,
  listenAssistantRunPresentation,
} from "@/lib/ipc";
import type { AssistantRunStartRequest } from "@/types/ai";

vi.mock("@/lib/ipc", () => ({
  assistantRunControl: vi.fn(),
  assistantRunGet: vi.fn(),
  assistantRunRetry: vi.fn(),
  assistantRunStart: vi.fn(),
  listenAssistantRunEvent: vi.fn(),
  listenAssistantRunPresentation: vi.fn(),
}));

const mockAssistantRunControl = vi.mocked(assistantRunControl);
const mockAssistantRunGet = vi.mocked(assistantRunGet);
const mockAssistantRunRetry = vi.mocked(assistantRunRetry);
const mockAssistantRunStart = vi.mocked(assistantRunStart);
const mockListenAssistantRunEvent = vi.mocked(listenAssistantRunEvent);
const mockListenAssistantRunPresentation = vi.mocked(
  listenAssistantRunPresentation,
);

let root: Root | null = null;
let host: HTMLDivElement | null = null;
let runApi: ReturnType<typeof useAssistantRun> | null = null;
let probeRenderCount = 0;

function Probe() {
  probeRenderCount += 1;
  runApi = useAssistantRun();
  return null;
}

function request(): AssistantRunStartRequest {
  return {
    clientRequestId: "client-run-1",
    message: "请总结这段对话",
    explicitReferences: [],
    webEnabled: false,
    securityDomain: "normal",
  };
}

function mountProbe(): void {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => root?.render(<Probe />));
}

afterEach(() => {
  if (root) {
    act(() => root?.unmount());
  }
  host?.remove();
  root = null;
  host = null;
  runApi = null;
  probeRenderCount = 0;
  mockAssistantRunControl.mockReset();
  mockAssistantRunGet.mockReset();
  mockAssistantRunRetry.mockReset();
  mockAssistantRunStart.mockReset();
  mockListenAssistantRunEvent.mockReset();
  mockListenAssistantRunPresentation.mockReset();
});

describe("useAssistantRun", () => {
  it("starts one unified Run and exposes its persisted accepted state", async () => {
    mockAssistantRunStart.mockResolvedValue({
      runId: "run-1",
      turnId: "turn-1",
      session: { domain: "normal", sessionKey: "session-1" },
      state: "accepted",
      stateVersion: 1,
    });
    mockAssistantRunGet.mockResolvedValue(null);
    mockListenAssistantRunEvent.mockResolvedValue(() => undefined);
    mockListenAssistantRunPresentation.mockResolvedValue(() => undefined);
    mountProbe();

    await act(async () => {
      await runApi?.start(request());
    });

    expect(mockAssistantRunStart).toHaveBeenCalledWith(request());
    expect(runApi?.runState).toBe("accepted");
    expect(runApi?.currentRun).toMatchObject({
      runId: "run-1",
      state: "accepted",
      stateVersion: 1,
      session: { domain: "normal", sessionKey: "session-1" },
    });
  });

  it("retries a terminal Web-verification failure as a distinct Run", async () => {
    let emit:
      | ((
          event: Parameters<Parameters<typeof listenAssistantRunEvent>[0]>[0],
        ) => void)
      | null = null;
    mockAssistantRunStart.mockResolvedValue({
      runId: "run-web-1",
      turnId: "turn-web-1",
      session: { domain: "normal", sessionKey: "session-web" },
      state: "accepted",
      stateVersion: 0,
    });
    mockAssistantRunRetry.mockResolvedValue({
      runId: "run-web-2",
      turnId: "turn-web-1",
      session: { domain: "normal", sessionKey: "session-web" },
      state: "accepted",
      stateVersion: 0,
    });
    mockAssistantRunGet.mockResolvedValue(null);
    mockListenAssistantRunEvent.mockImplementation(async (handler) => {
      emit = handler;
      return () => undefined;
    });
    mockListenAssistantRunPresentation.mockResolvedValue(() => undefined);
    mountProbe();
    await act(async () => {
      await runApi?.start({ ...request(), webEnabled: true });
    });
    act(() => {
      emit?.({
        runId: "run-web-1",
        seq: 2,
        stateVersion: 1,
        timestamp: "2026-07-17T00:00:00Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "preparing",
          stage: "Preparing",
        },
      });
      emit?.({
        runId: "run-web-1",
        seq: 3,
        stateVersion: 2,
        timestamp: "2026-07-17T00:00:01Z",
        type: "stage_changed",
        payload: { kind: "stage_changed", state: "running", stage: "Running" },
      });
      emit?.({
        runId: "run-web-1",
        seq: 4,
        stateVersion: 2,
        timestamp: "2026-07-17T00:00:02Z",
        type: "web_verification_failed",
        payload: {
          kind: "web_verification_failed",
          code: "agent_run_web_provider_timeout",
          failureReason: "provider_timeout",
          retryable: true,
          attemptCount: 4,
          durationBucket: "budget_exhausted",
          diagnosticId: "run-web-1",
        },
      });
      emit?.({
        runId: "run-web-1",
        seq: 5,
        stateVersion: 3,
        timestamp: "2026-07-17T00:00:03Z",
        type: "failed",
        payload: {
          kind: "failed",
          code: "agent_run_web_provider_timeout",
          message: "Timed out",
        },
      });
    });
    await act(async () => {
      await runApi?.retryWebVerification();
    });
    expect(mockAssistantRunRetry).toHaveBeenCalledWith(
      expect.objectContaining({
        sourceRunId: "run-web-1",
        session: { domain: "normal", sessionKey: "session-web" },
      }),
    );
    expect(mockAssistantRunStart).toHaveBeenCalledTimes(1);
    expect(runApi?.currentRun?.runId).toBe("run-web-2");
  });

  it("keeps one event subscription while the active Run changes", async () => {
    mockAssistantRunStart.mockResolvedValue({
      runId: "run-subscription",
      turnId: "turn-subscription",
      session: { domain: "normal", sessionKey: "session-subscription" },
      state: "accepted",
      stateVersion: 1,
    });
    mockAssistantRunGet.mockResolvedValue(null);
    mockListenAssistantRunEvent.mockResolvedValue(() => undefined);
    mockListenAssistantRunPresentation.mockResolvedValue(() => undefined);
    mountProbe();

    await act(async () => {
      await runApi?.start(request());
    });

    expect(mockListenAssistantRunEvent).toHaveBeenCalledTimes(1);
    expect(mockListenAssistantRunPresentation).toHaveBeenCalledTimes(1);
  });

  it("submits the persisted confirmation identity and optimistic state version", async () => {
    let emit:
      | ((
          event: Parameters<Parameters<typeof listenAssistantRunEvent>[0]>[0],
        ) => void)
      | null = null;
    mockAssistantRunStart.mockResolvedValue({
      runId: "run-confirmation",
      turnId: "turn-confirmation",
      session: { domain: "normal", sessionKey: "session-confirmation" },
      state: "accepted",
      stateVersion: 1,
    });
    mockAssistantRunGet.mockResolvedValue(null);
    mockListenAssistantRunEvent.mockImplementation(async (handler) => {
      emit = handler;
      return () => undefined;
    });
    mockListenAssistantRunPresentation.mockResolvedValue(() => undefined);
    mountProbe();

    await act(async () => {
      await runApi?.start(request());
    });
    act(() => {
      emit?.({
        runId: "run-confirmation",
        seq: 2,
        stateVersion: 2,
        timestamp: "2026-07-14T00:00:00.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "preparing",
          stage: "Preparing",
        },
      });
      emit?.({
        runId: "run-confirmation",
        seq: 3,
        stateVersion: 3,
        timestamp: "2026-07-14T00:00:00.000Z",
        type: "stage_changed",
        payload: { kind: "stage_changed", state: "running", stage: "Running" },
      });
      emit?.({
        runId: "run-confirmation",
        seq: 4,
        stateVersion: 4,
        timestamp: "2026-07-14T00:00:00.000Z",
        type: "confirmation_required",
        payload: {
          kind: "confirmation_required",
          confirmationId: "confirmation-001",
          planHash: "sha256:plan",
          summary: "Update one note",
          effect: "apply",
          targets: [
            { kind: "note", label: "notes/agent.md", risk: "bounded_write" },
          ],
          expiresAt: "2026-07-15T00:00:00.000Z",
        },
      });
    });

    expect(runApi?.pendingConfirmation?.confirmationId).toBe(
      "confirmation-001",
    );
    await act(async () => {
      await runApi?.approveChange();
    });
    expect(mockAssistantRunControl).toHaveBeenCalledWith({
      session: { domain: "normal", sessionKey: "session-confirmation" },
      runId: "run-confirmation",
      expectedStateVersion: 4,
      action: {
        type: "approve_change",
        confirmationId: "confirmation-001",
        planHash: "sha256:plan",
      },
    });
  });

  it("replays durable recovery kind and resumes with the persisted state version", async () => {
    mockAssistantRunGet.mockResolvedValue(null);
    mockListenAssistantRunEvent.mockResolvedValue(() => undefined);
    mockListenAssistantRunPresentation.mockResolvedValue(() => undefined);
    mountProbe();

    act(() => {
      runApi?.recover({
        run: {
          runId: "run-recovery",
          turnId: "turn-recovery",
          session: { domain: "normal", sessionKey: "session-recovery" },
          state: "paused",
          stateVersion: 8,
          recovery: "resume_available",
        },
        events: [
          {
            runId: "run-recovery",
            seq: 1,
            stateVersion: 8,
            timestamp: "2026-07-29T00:00:00.000Z",
            type: "paused",
            payload: {
              kind: "paused",
              reason: "可安全继续",
              recovery: "resume_available",
            },
          },
        ],
      });
    });

    expect(runApi?.recovery).toBe("resume_available");
    await act(async () => {
      await runApi?.resume();
    });
    expect(mockAssistantRunControl).toHaveBeenCalledWith({
      session: { domain: "normal", sessionKey: "session-recovery" },
      runId: "run-recovery",
      expectedStateVersion: 8,
      action: { type: "resume" },
    });
  });

  it("reduces replayable events to the authoritative Run state and version", async () => {
    let emit:
      | ((
          event: Parameters<Parameters<typeof listenAssistantRunEvent>[0]>[0],
        ) => void)
      | null = null;
    mockAssistantRunStart.mockResolvedValue({
      runId: "run-2",
      turnId: "turn-2",
      session: { domain: "normal", sessionKey: "session-2" },
      state: "accepted",
      stateVersion: 1,
    });
    mockAssistantRunGet.mockResolvedValue(null);
    mockListenAssistantRunEvent.mockImplementation(async (handler) => {
      emit = handler;
      return () => undefined;
    });
    mockListenAssistantRunPresentation.mockResolvedValue(() => undefined);
    mountProbe();

    await act(async () => {
      await runApi?.start({ ...request(), clientRequestId: "client-run-2" });
    });
    act(() => {
      emit?.({
        runId: "run-2",
        seq: 2,
        stateVersion: 2,
        timestamp: "2026-07-13T12:00:00.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "preparing",
          stage: "正在准备",
        },
      });
      emit?.({
        runId: "run-2",
        seq: 3,
        stateVersion: 3,
        timestamp: "2026-07-13T12:00:01.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "running",
          stage: "正在处理",
        },
      });
      emit?.({
        runId: "run-2",
        seq: 4,
        stateVersion: 4,
        timestamp: "2026-07-13T12:00:02.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "awaiting_confirmation",
          stage: "等待确认",
        },
      });
    });

    expect(runApi?.runState).toBe("awaiting_confirmation");
    expect(runApi?.currentRun).toMatchObject({
      state: "awaiting_confirmation",
      stateVersion: 4,
    });
    expect(runApi?.latestEvent).toMatchObject({
      runId: "run-2",
      payload: { kind: "stage_changed", stage: "等待确认" },
    });
  });

  it("cancel recovers from state_version_conflict by replaying then retrying", async () => {
    let emit:
      | ((
          event: Parameters<Parameters<typeof listenAssistantRunEvent>[0]>[0],
        ) => void)
      | null = null;
    mockAssistantRunStart.mockResolvedValue({
      runId: "run-cancel",
      turnId: "turn-cancel",
      session: { domain: "normal", sessionKey: "session-cancel" },
      state: "accepted",
      stateVersion: 1,
    });
    mockAssistantRunGet.mockResolvedValue({
      run: {
        runId: "run-cancel",
        turnId: "turn-cancel",
        session: { domain: "normal", sessionKey: "session-cancel" },
        state: "running",
        stateVersion: 5,
      },
      events: [
        {
          runId: "run-cancel",
          seq: 1,
          stateVersion: 0,
          timestamp: "2026-07-22T08:00:00.000Z",
          type: "accepted",
          payload: {
            kind: "accepted",
            turnId: "turn-cancel",
            sessionKey: "session-cancel",
          },
        },
        {
          runId: "run-cancel",
          seq: 2,
          stateVersion: 5,
          timestamp: "2026-07-22T08:00:05.000Z",
          type: "stage_changed",
          payload: {
            kind: "stage_changed",
            state: "running",
            stage: "正在生成答复",
          },
        },
      ],
    });
    mockListenAssistantRunEvent.mockImplementation(async (handler) => {
      emit = handler;
      return () => undefined;
    });
    mockListenAssistantRunPresentation.mockResolvedValue(() => undefined);
    mockAssistantRunControl
      .mockRejectedValueOnce(new Error("agent_run_state_version_conflict"))
      .mockResolvedValueOnce(undefined);
    mountProbe();

    await act(async () => {
      await runApi?.start({
        ...request(),
        clientRequestId: "client-run-cancel",
      });
    });
    act(() => {
      emit?.({
        runId: "run-cancel",
        seq: 2,
        stateVersion: 2,
        timestamp: "2026-07-22T08:00:01.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "running",
          stage: "正在生成答复",
        },
      });
    });

    let cancelResult: string | null | undefined;
    await act(async () => {
      cancelResult = await runApi?.cancel();
    });

    expect(cancelResult).toBeNull();
    expect(mockAssistantRunControl).toHaveBeenCalledTimes(2);
    expect(mockAssistantRunControl).toHaveBeenLastCalledWith({
      session: { domain: "normal", sessionKey: "session-cancel" },
      runId: "run-cancel",
      expectedStateVersion: 5,
      action: { type: "cancel" },
    });
    expect(mockAssistantRunGet).toHaveBeenCalled();
  });

  it("answerComplete 使 isBusy 变为 false，即使 durable 仍是 running", async () => {
    let emitPresentation:
      | ((
          event: Parameters<
            Parameters<typeof listenAssistantRunPresentation>[0]
          >[0],
        ) => void)
      | null = null;
    let emit:
      | ((
          event: Parameters<Parameters<typeof listenAssistantRunEvent>[0]>[0],
        ) => void)
      | null = null;
    mockAssistantRunStart.mockResolvedValue({
      runId: "run-complete-ui",
      turnId: "turn-complete-ui",
      session: { domain: "normal", sessionKey: "session-complete-ui" },
      state: "accepted",
      stateVersion: 1,
    });
    mockAssistantRunGet.mockResolvedValue(null);
    mockListenAssistantRunEvent.mockImplementation(async (handler) => {
      emit = handler;
      return () => undefined;
    });
    mockListenAssistantRunPresentation.mockImplementation(async (handler) => {
      emitPresentation = handler;
      return () => undefined;
    });
    mountProbe();

    await act(async () => {
      await runApi?.start({
        ...request(),
        clientRequestId: "client-run-complete-ui",
      });
    });
    act(() => {
      emit?.({
        runId: "run-complete-ui",
        seq: 2,
        stateVersion: 2,
        timestamp: "2026-07-22T08:00:01.000Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "running",
          stage: "正在生成答复",
        },
      });
    });
    expect(runApi?.isBusy).toBe(true);

    act(() => {
      emitPresentation?.({
        runId: "run-complete-ui",
        presentationSeq: 1,
        elapsedMs: 10,
        type: "answer_delta",
        payload: { kind: "answer_delta", delta: "答复正文" },
      });
      emitPresentation?.({
        runId: "run-complete-ui",
        presentationSeq: 2,
        elapsedMs: 20,
        type: "answer_complete",
        payload: { kind: "answer_complete" },
      });
    });

    expect(runApi?.presentationState?.answerComplete).toBe(true);
    expect(runApi?.isBusy).toBe(false);
    expect(["accepted", "preparing", "running", "verifying"]).toContain(
      runApi?.runState,
    );
  });

  it("窗口重新获得焦点时重放仍显示为非终态的 Run", async () => {
    mockAssistantRunStart.mockResolvedValue({
      runId: "run-focus-replay",
      turnId: "turn-focus-replay",
      session: { domain: "normal", sessionKey: "session-focus-replay" },
      state: "accepted",
      stateVersion: 1,
    });
    mockAssistantRunGet.mockResolvedValueOnce(null);
    mockListenAssistantRunEvent.mockResolvedValue(() => undefined);
    mockListenAssistantRunPresentation.mockResolvedValue(() => undefined);
    mountProbe();

    await act(async () => {
      await runApi?.start({
        ...request(),
        clientRequestId: "client-run-focus-replay",
      });
    });
    mockAssistantRunGet.mockClear();
    mockAssistantRunGet.mockResolvedValue({
      run: {
        runId: "run-focus-replay",
        turnId: "turn-focus-replay",
        session: { domain: "normal", sessionKey: "session-focus-replay" },
        state: "completed",
        stateVersion: 2,
        finalMessageId: 7,
      },
      events: [
        {
          runId: "run-focus-replay",
          seq: 1,
          stateVersion: 1,
          timestamp: "2026-08-18T08:00:00.000Z",
          type: "accepted",
          payload: {
            kind: "accepted",
            turnId: "turn-focus-replay",
            sessionKey: "session-focus-replay",
          },
        },
        {
          runId: "run-focus-replay",
          seq: 2,
          stateVersion: 2,
          timestamp: "2026-08-18T08:00:01.000Z",
          type: "completed",
          payload: { kind: "completed", finalMessageId: 7 },
        },
      ],
    });

    await act(async () => {
      window.dispatchEvent(new Event("focus"));
      await vi.waitFor(() => {
        expect(mockAssistantRunGet).toHaveBeenCalledTimes(1);
      });
    });

    expect(runApi?.runState).toBe("completed");
  });

  it("batches streaming deltas into one animation-frame render and flushes completion immediately", async () => {
    let emitPresentation:
      | ((
          event: Parameters<
            Parameters<typeof listenAssistantRunPresentation>[0]
          >[0],
        ) => void)
      | null = null;
    const frameCallbacks = new Map<number, FrameRequestCallback>();
    let nextFrame = 1;
    const requestFrame = vi
      .spyOn(window, "requestAnimationFrame")
      .mockImplementation((callback) => {
        const frame = nextFrame;
        nextFrame += 1;
        frameCallbacks.set(frame, callback);
        return frame;
      });
    const cancelFrame = vi
      .spyOn(window, "cancelAnimationFrame")
      .mockImplementation((frame) => {
        frameCallbacks.delete(frame);
      });
    mockAssistantRunStart.mockResolvedValue({
      runId: "run-batched-stream",
      turnId: "turn-batched-stream",
      session: { domain: "normal", sessionKey: "session-batched-stream" },
      state: "accepted",
      stateVersion: 1,
    });
    mockAssistantRunGet.mockResolvedValue(null);
    mockListenAssistantRunEvent.mockResolvedValue(() => undefined);
    mockListenAssistantRunPresentation.mockImplementation(async (handler) => {
      emitPresentation = handler;
      return () => undefined;
    });
    mountProbe();

    try {
      await act(async () => {
        await runApi?.start({
          ...request(),
          clientRequestId: "client-run-batched-stream",
        });
      });
      const rendersBeforeDeltas = probeRenderCount;

      act(() => {
        for (const [presentationSeq, delta] of [
          [1, "one "],
          [2, "two "],
          [3, "three"],
        ] as const) {
          emitPresentation?.({
            runId: "run-batched-stream",
            presentationSeq,
            elapsedMs: presentationSeq * 10,
            type: "answer_delta",
            payload: { kind: "answer_delta", delta },
          });
        }
      });

      expect(requestFrame).toHaveBeenCalledTimes(1);
      expect(probeRenderCount).toBe(rendersBeforeDeltas);
      expect(runApi?.presentationState?.answer).toBe("");

      act(() => {
        frameCallbacks.get(1)?.(16);
      });
      expect(runApi?.presentationState?.answer).toBe("one two three");
      expect(probeRenderCount).toBe(rendersBeforeDeltas + 1);

      act(() => {
        emitPresentation?.({
          runId: "run-batched-stream",
          presentationSeq: 4,
          elapsedMs: 40,
          type: "answer_delta",
          payload: { kind: "answer_delta", delta: " done" },
        });
        emitPresentation?.({
          runId: "run-batched-stream",
          presentationSeq: 5,
          elapsedMs: 50,
          type: "answer_complete",
          payload: { kind: "answer_complete" },
        });
      });

      expect(cancelFrame).toHaveBeenCalledWith(2);
      expect(runApi?.presentationState).toMatchObject({
        answer: "one two three done",
        answerComplete: true,
      });
    } finally {
      requestFrame.mockRestore();
      cancelFrame.mockRestore();
    }
  });

  it("queued_previous_run_frame_cannot_patch_new_run", async () => {
    let emitPresentation:
      | ((
          event: Parameters<
            Parameters<typeof listenAssistantRunPresentation>[0]
          >[0],
        ) => void)
      | null = null;
    const frameCallbacks = new Map<number, FrameRequestCallback>();
    let nextFrame = 1;
    const requestFrame = vi
      .spyOn(window, "requestAnimationFrame")
      .mockImplementation((callback) => {
        const frame = nextFrame;
        nextFrame += 1;
        frameCallbacks.set(frame, callback);
        return frame;
      });
    const cancelFrame = vi
      .spyOn(window, "cancelAnimationFrame")
      .mockImplementation((frame) => {
        frameCallbacks.delete(frame);
      });

    mockAssistantRunStart
      .mockResolvedValueOnce({
        runId: "run-old",
        turnId: "turn-old",
        session: { domain: "normal", sessionKey: "session-1" },
        state: "accepted",
        stateVersion: 1,
      })
      .mockResolvedValueOnce({
        runId: "run-new",
        turnId: "turn-new",
        session: { domain: "normal", sessionKey: "session-1" },
        state: "accepted",
        stateVersion: 1,
      });
    mockAssistantRunGet.mockResolvedValue(null);
    mockListenAssistantRunEvent.mockResolvedValue(() => undefined);
    mockListenAssistantRunPresentation.mockImplementation(async (handler) => {
      emitPresentation = handler;
      return () => undefined;
    });
    mountProbe();

    try {
      await act(async () => {
        await runApi?.start({
          ...request(),
          clientRequestId: "client-run-old",
        });
      });

      act(() => {
        emitPresentation?.({
          runId: "run-old",
          presentationSeq: 1,
          elapsedMs: 10,
          type: "answer_delta",
          payload: { kind: "answer_delta", delta: "old" },
        });
      });

      const oldFrame = frameCallbacks.get(1);
      expect(oldFrame).toBeTypeOf("function");

      await act(async () => {
        await runApi?.start({
          ...request(),
          clientRequestId: "client-run-new",
        });
      });

      expect(runApi?.presentationState).toMatchObject({
        runId: "run-new",
        answer: "",
        lastSeq: 0,
      });
      expect(cancelFrame).toHaveBeenCalledWith(1);

      act(() => {
        oldFrame?.(16);
      });

      expect(runApi?.presentationState).toMatchObject({
        runId: "run-new",
        answer: "",
        lastSeq: 0,
      });
    } finally {
      requestFrame.mockRestore();
      cancelFrame.mockRestore();
    }
  });
});
