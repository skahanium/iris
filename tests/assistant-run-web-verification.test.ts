import { describe, expect, it } from "vitest";

import {
  createAssistantRunEventState,
  reduceAssistantRunEvent,
} from "@/lib/assistant-run-events";
import type { AssistantRunEvent } from "@/types/ai";

const runId = "web-failure-run";

describe("required Web verification events", () => {
  it("keeps the safe diagnostic payload through the terminal failure", () => {
    const events: AssistantRunEvent[] = [
      {
        runId,
        seq: 1,
        stateVersion: 0,
        timestamp: "2026-07-17T00:00:00Z",
        type: "accepted",
        payload: {
          kind: "accepted",
          turnId: "turn-1",
          sessionKey: "session-1",
        },
      },
      {
        runId,
        seq: 2,
        stateVersion: 1,
        timestamp: "2026-07-17T00:00:01Z",
        type: "stage_changed",
        payload: {
          kind: "stage_changed",
          state: "preparing",
          stage: "Preparing",
        },
      },
      {
        runId,
        seq: 3,
        stateVersion: 2,
        timestamp: "2026-07-17T00:00:02Z",
        type: "stage_changed",
        payload: { kind: "stage_changed", state: "running", stage: "Running" },
      },
      {
        runId,
        seq: 4,
        stateVersion: 2,
        timestamp: "2026-07-17T00:00:03Z",
        type: "web_verification_failed",
        payload: {
          kind: "web_verification_failed",
          code: "agent_run_web_provider_timeout",
          retryable: true,
          attemptCount: 4,
          durationBucket: "budget_exhausted",
          diagnosticId: runId,
        },
      },
      {
        runId,
        seq: 5,
        stateVersion: 3,
        timestamp: "2026-07-17T00:00:04Z",
        type: "failed",
        payload: {
          kind: "failed",
          code: "agent_run_web_provider_timeout",
          message: "Timed out",
        },
      },
    ];
    const state = events.reduce(
      reduceAssistantRunEvent,
      createAssistantRunEventState(runId),
    );
    expect(state.state).toBe("failed");
    expect(state.webVerificationFailure).toMatchObject({
      diagnosticId: runId,
      attemptCount: 4,
      retryable: true,
    });
    expect(state.content).toBe("");
  });
});
