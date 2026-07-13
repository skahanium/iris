import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";

import { useAssistantRunTranscript } from "@/components/ai/hooks/useAssistantRunTranscript";
import type { ChatLine } from "@/components/ai/AiMessageList";
import type { AssistantRunEvent } from "@/types/ai";

let root: Root | null = null;
let host: HTMLDivElement | null = null;
let messages: ChatLine[] = [];

function Probe({ event }: { event: AssistantRunEvent | null }) {
  useAssistantRunTranscript({
    event,
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
          event={{
            runId: "run-1",
            seq: 3,
            stateVersion: 2,
            timestamp: "2026-07-13T12:00:00.000Z",
            type: "content_delta",
            payload: { kind: "content_delta", delta: "世界" },
          }}
        />,
      ),
    );

    expect(messages).toEqual([
      { role: "user", content: "你好" },
      { role: "assistant", content: "世界" },
    ]);
  });
});
