import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useAssistantRun } from "@/hooks/useAssistantRun";
import {
  assistantRunControl,
  assistantRunStart,
  listenAssistantRunEvent,
} from "@/lib/ipc";
import type { AssistantRunStartRequest } from "@/types/ai";

vi.mock("@/lib/ipc", () => ({
  assistantRunControl: vi.fn(),
  assistantRunStart: vi.fn(),
  listenAssistantRunEvent: vi.fn(),
}));

const mockAssistantRunControl = vi.mocked(assistantRunControl);
const mockAssistantRunStart = vi.mocked(assistantRunStart);
const mockListenAssistantRunEvent = vi.mocked(listenAssistantRunEvent);

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
  mockAssistantRunStart.mockReset();
  mockListenAssistantRunEvent.mockReset();
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
    mockListenAssistantRunEvent.mockResolvedValue(() => undefined);
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

  it("keeps one event subscription while the active Run changes", async () => {
    mockAssistantRunStart.mockResolvedValue({
      runId: "run-subscription",
      turnId: "turn-subscription",
      session: { domain: "normal", sessionKey: "session-subscription" },
      state: "accepted",
      stateVersion: 1,
    });
    mockListenAssistantRunEvent.mockResolvedValue(() => undefined);
    mountProbe();

    await act(async () => {
      await runApi?.start(request());
    });

    expect(mockListenAssistantRunEvent).toHaveBeenCalledTimes(1);
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
    mockListenAssistantRunEvent.mockImplementation(async (handler) => {
      emit = handler;
      return () => undefined;
    });
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
          state: "awaiting_confirmation",
          stage: "等待确认",
        },
      });
    });

    expect(runApi?.runState).toBe("awaiting_confirmation");
    expect(runApi?.currentRun).toMatchObject({
      state: "awaiting_confirmation",
      stateVersion: 2,
    });
    expect(runApi?.latestEvent).toMatchObject({
      runId: "run-2",
      payload: { kind: "stage_changed", stage: "等待确认" },
    });
  });
});
