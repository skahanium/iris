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

function Probe() {
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
});
