import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ChatLine } from "@/components/ai/AiMessageList";

const handlers: Record<string, ((payload: unknown) => void) | undefined> = {};

vi.mock("@/lib/ipc", () => ({
  listenLlmToken: vi.fn().mockImplementation((h: (p: unknown) => void) => {
    handlers.token = h;
    return Promise.resolve(() => {});
  }),
  listenLlmDone: vi.fn().mockImplementation((h: (p: unknown) => void) => {
    handlers.done = h;
    return Promise.resolve(() => {});
  }),
  listenLlmError: vi.fn().mockImplementation((h: (p: unknown) => void) => {
    handlers.error = h;
    return Promise.resolve(() => {});
  }),
  listenLlmReset: vi.fn().mockImplementation((h: (p: unknown) => void) => {
    handlers.reset = h;
    return Promise.resolve(() => {});
  }),
  listenAiRetryStatus: vi.fn().mockImplementation((h: (p: unknown) => void) => {
    handlers.retry = h;
    return Promise.resolve(() => {});
  }),
}));

describe("useAssistantLlmStream reset + done behavior", () => {
  let root: Root;
  let container: HTMLDivElement;
  let messagesState: ChatLine[];
  let streamingState: boolean;
  let panelSendActive: { current: boolean };
  let requestId: { current: string | null };
  let streamBuf: { current: string };

  async function mountHook() {
    const { useAssistantLlmStream } =
      await import("@/hooks/useAssistantLlmStream");
    function Host() {
      useAssistantLlmStream({
        panelSendActiveRef: panelSendActive,
        requestIdRef: requestId,
        streamBufRef: streamBuf,
        setMessages: (updater) => {
          messagesState =
            typeof updater === "function"
              ? (updater as (p: ChatLine[]) => ChatLine[])(messagesState)
              : (updater as ChatLine[]);
        },
        setStreaming: (v) => {
          streamingState =
            typeof v === "function"
              ? (v as (p: boolean) => boolean)(streamingState)
              : (v as boolean);
        },
      });
      return null;
    }
    await act(async () => {
      root.render(createElement(Host));
    });
  }

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    messagesState = [];
    streamingState = false;
    panelSendActive = { current: true };
    requestId = { current: "req-1" };
    streamBuf = { current: "" };
    Object.keys(handlers).forEach((k) => delete handlers[k]);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("streams tokens into the assistant slot", async () => {
    await mountHook();

    await act(async () => {
      handlers.token?.({ request_id: "req-1", token: "你好", index: 0 });
    });
    await act(async () => {
      handlers.token?.({ request_id: "req-1", token: "世界", index: 1 });
    });
    // flush RAF timeout (50ms throttle)
    await act(async () => {
      await new Promise((r) => setTimeout(r, 80));
    });

    expect(streamBuf.current).toBe("你好世界");
    const last = messagesState[messagesState.length - 1];
    expect(last?.role).toBe("assistant");
    expect(last?.content).toContain("你好世界");
  });

  it("llm:reset clears the stream buffer and empties the assistant slot", async () => {
    await mountHook();

    // seed some streamed content first
    await act(async () => {
      handlers.token?.({ request_id: "req-1", token: "前导文本", index: 0 });
    });
    await act(async () => {
      await new Promise((r) => setTimeout(r, 80));
    });
    expect(streamBuf.current).toBe("前导文本");

    // fire reset
    await act(async () => {
      handlers.reset?.({ request_id: "req-1" });
    });

    expect(streamBuf.current).toBe("");
    const last = messagesState[messagesState.length - 1];
    expect(last?.role).toBe("assistant");
    expect(last?.content).toBe("");
  });

  it("llm:done does not flip streaming to false (task runner owns streaming state)", async () => {
    streamingState = true;
    await mountHook();

    await act(async () => {
      handlers.done?.({ request_id: "req-1" });
    });

    expect(streamingState).toBe(true);
  });
});
