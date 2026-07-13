import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";

import { useAssistantRunTranscript } from "@/components/ai/hooks/useAssistantRunTranscript";
import type { ChatLine } from "@/components/ai/AiMessageList";
import { replayAssistantRunEvents } from "@/lib/assistant-run-events";
import type { AssistantRunEvent } from "@/types/ai";

let root: Root | null = null;
let host: HTMLDivElement | null = null;
let messages: ChatLine[] = [];

function Probe({ run }: { run: ReturnType<typeof replayAssistantRunEvents> }) {
  useAssistantRunTranscript({
    run,
    setMessages: (updater) => {
      messages = typeof updater === "function" ? updater(messages) : updater;
    },
    setStreaming: () => undefined,
    setActivityHint: () => undefined,
    setError: () => undefined,
  });
  return null;
}

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
  root = null;
  host = null;
  messages = [];
});

describe("useAssistantRunTranscript", () => {
  it("adds a durable content delta only to the active assistant placeholder", () => {
    messages = [
      { role: "user", content: "你好" },
      { role: "assistant", content: "" },
    ];
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(
        <Probe
          run={replayAssistantRunEvents("run-1", [
            {
              runId: "run-1",
              seq: 1,
              stateVersion: 0,
              timestamp: "2026-07-13T12:00:00.000Z",
              type: "accepted",
              payload: {
                kind: "accepted",
                turnId: "turn-1",
                sessionKey: "session-1",
              },
            },
            {
              runId: "run-1",
              seq: 2,
              stateVersion: 1,
              timestamp: "2026-07-13T12:00:01.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "preparing",
                stage: "正在准备",
              },
            },
            {
              runId: "run-1",
              seq: 3,
              stateVersion: 2,
              timestamp: "2026-07-13T12:00:02.000Z",
              type: "stage_changed",
              payload: {
                kind: "stage_changed",
                state: "running",
                stage: "正在生成答复",
              },
            },
            {
              runId: "run-1",
              seq: 4,
              stateVersion: 2,
              timestamp: "2026-07-13T12:00:03.000Z",
              type: "content_delta",
              payload: { kind: "content_delta", delta: "世界" },
            },
          ] satisfies AssistantRunEvent[])}
        />,
      ),
    );

    expect(messages).toEqual([
      { role: "user", content: "你好" },
      { role: "assistant", content: "世界" },
    ]);
  });
});
