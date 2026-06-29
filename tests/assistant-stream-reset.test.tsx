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
  let rootUnmounted: boolean;
  let rafCallbacks: Map<number, FrameRequestCallback>;
  let nextRafId: number;
  let requestAnimationFrameSpy: { mockRestore: () => void };
  let cancelAnimationFrameSpy: ReturnType<typeof vi.spyOn>;

  function flushRaf() {
    const callbacks = [...rafCallbacks.entries()];
    rafCallbacks.clear();
    callbacks.forEach(([, callback]) => callback(performance.now()));
  }

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
    rootUnmounted = false;
    messagesState = [];
    streamingState = false;
    panelSendActive = { current: true };
    requestId = { current: "req-1" };
    streamBuf = { current: "" };
    rafCallbacks = new Map();
    nextRafId = 1;
    requestAnimationFrameSpy = vi
      .spyOn(window, "requestAnimationFrame")
      .mockImplementation((callback) => {
        const id = nextRafId;
        nextRafId += 1;
        rafCallbacks.set(id, callback);
        return id;
      });
    cancelAnimationFrameSpy = vi
      .spyOn(window, "cancelAnimationFrame")
      .mockImplementation((id) => {
        rafCallbacks.delete(id);
      });
    Object.keys(handlers).forEach((k) => delete handlers[k]);
  });

  afterEach(() => {
    if (!rootUnmounted) {
      act(() => {
        root.unmount();
      });
    }
    container.remove();
    requestAnimationFrameSpy.mockRestore();
    cancelAnimationFrameSpy.mockRestore();
    vi.clearAllMocks();
  });

  it("streams tokens into the assistant slot", async () => {
    await mountHook();

    await act(async () => {
      handlers.token?.({ request_id: "req-1", token: "你好", index: 0 });
    });
    await act(async () => {
      handlers.token?.({ request_id: "req-1", token: "世界", index: 1 });
    });

    await act(async () => {
      flushRaf();
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
      flushRaf();
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

  it("llm:done does not replace the assistant message when the snapshot is unchanged", async () => {
    streamBuf.current = "最终文本";
    await mountHook();

    await act(async () => {
      handlers.done?.({ request_id: "req-1" });
    });
    const firstMessagesRef = messagesState;

    await act(async () => {
      handlers.done?.({ request_id: "req-1" });
    });

    expect(messagesState).toBe(firstMessagesRef);
    expect(messagesState[messagesState.length - 1]).toEqual({
      role: "assistant",
      content: "最终文本",
    });
  });

  it("llm:reset does not replace an already-empty assistant slot", async () => {
    messagesState = [{ role: "assistant", content: "" }];
    await mountHook();
    const firstMessagesRef = messagesState;

    await act(async () => {
      handlers.reset?.({ request_id: "req-1" });
    });

    expect(messagesState).toBe(firstMessagesRef);
    expect(messagesState[messagesState.length - 1]).toEqual({
      role: "assistant",
      content: "",
    });
  });

  it("llm:reset cancels a pending animation-frame flush before clearing", async () => {
    await mountHook();

    await act(async () => {
      handlers.token?.({ request_id: "req-1", token: "不应泄漏", index: 0 });
    });
    expect(rafCallbacks.size).toBe(1);

    await act(async () => {
      handlers.reset?.({ request_id: "req-1" });
    });
    await act(async () => {
      flushRaf();
    });

    expect(cancelAnimationFrameSpy).toHaveBeenCalled();
    expect(streamBuf.current).toBe("");
    const last = messagesState[messagesState.length - 1];
    expect(last?.role).toBe("assistant");
    expect(last?.content).toBe("");
  });

  it("llm:error cancels a pending animation-frame flush", async () => {
    await mountHook();

    await act(async () => {
      handlers.token?.({ request_id: "req-1", token: "不应保留", index: 0 });
    });
    expect(rafCallbacks.size).toBe(1);

    await act(async () => {
      handlers.error?.({ request_id: "req-1", error: "boom" });
    });
    await act(async () => {
      flushRaf();
    });

    expect(cancelAnimationFrameSpy).toHaveBeenCalled();
    expect(streamBuf.current).toBe("");
    expect(messagesState[messagesState.length - 1]).toEqual({
      role: "system",
      content: "错误: boom",
    });
  });

  it("unmount cancels a pending animation-frame flush", async () => {
    await mountHook();

    await act(async () => {
      handlers.token?.({ request_id: "req-1", token: "卸载前", index: 0 });
    });
    expect(rafCallbacks.size).toBe(1);

    act(() => {
      root.unmount();
    });
    rootUnmounted = true;

    expect(cancelAnimationFrameSpy).toHaveBeenCalled();
    expect(rafCallbacks.size).toBe(0);
  });
});
