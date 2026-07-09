import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  AssistantProcessEvent,
  ChatLine,
} from "@/components/ai/AiMessageList";

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
  listenHarnessTrace: vi.fn().mockImplementation((h: (p: unknown) => void) => {
    handlers.trace = h;
    return Promise.resolve(() => {});
  }),
  listenAiThinking: vi.fn().mockImplementation((h: (p: unknown) => void) => {
    handlers.thinking = h;
    return Promise.resolve(() => {});
  }),
}));

describe("useAssistantLlmStream reset + done behavior", () => {
  let root: Root;
  let container: HTMLDivElement;
  let messagesState: ChatLine[];
  let streamingState: boolean;
  let activityHintState: string | null;
  let processEventsState: AssistantProcessEvent[];
  let panelSendActive: { current: boolean };
  let requestId: { current: string | null };
  let streamBuf: { current: string };
  let lifecycleEvents: unknown[];
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
      const lifecycleRecorder = (entry: unknown) => {
        lifecycleEvents.push(entry);
      };
      useAssistantLlmStream({
        panelSendActiveRef: panelSendActive,
        requestIdRef: requestId,
        streamBufRef: streamBuf,
        lifecycleRecorder,
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
        setActivityHint: (v) => {
          activityHintState =
            typeof v === "function"
              ? (v as (p: string | null) => string | null)(activityHintState)
              : (v as string | null);
        },
        setProcessEvents: (v) => {
          processEventsState =
            typeof v === "function"
              ? (v as (p: AssistantProcessEvent[]) => AssistantProcessEvent[])(
                  processEventsState,
                )
              : (v as AssistantProcessEvent[]);
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
    activityHintState = null;
    processEventsState = [];
    lifecycleEvents = [];
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
      handlers.token?.({
        request_id: "req-1",
        token: "你好",
        index: 0,
        surface: "visible_answer",
      });
    });
    await act(async () => {
      handlers.token?.({
        request_id: "req-1",
        token: "世界",
        index: 1,
        surface: "visible_answer",
      });
    });

    await act(async () => {
      flushRaf();
    });

    expect(streamBuf.current).toBe("你好世界");
    const last = messagesState[messagesState.length - 1];
    expect(last?.role).toBe("assistant");
    expect(last?.content).toContain("你好世界");
  });

  it("internal candidate tokens, done, and reset do not touch the visible assistant slot", async () => {
    messagesState = [{ role: "user", content: "问题" }];
    await mountHook();
    const firstMessagesRef = messagesState;

    await act(async () => {
      handlers.token?.({
        request_id: "req-1",
        token: "内部前导",
        index: 0,
        surface: "internal_candidate",
        candidate_kind: "internal_candidate",
      });
    });
    await act(async () => {
      flushRaf();
    });
    await act(async () => {
      handlers.done?.({
        request_id: "req-1",
        surface: "internal_candidate",
        candidate_kind: "internal_candidate",
      });
    });
    await act(async () => {
      handlers.reset?.({
        request_id: "req-1",
        reason_kind: "tool_round",
        surface: "internal_candidate",
        candidate_kind: "internal_candidate",
      });
    });

    expect(streamBuf.current).toBe("");
    expect(messagesState).toBe(firstMessagesRef);
    expect(messagesState).toEqual([{ role: "user", content: "问题" }]);
    expect(lifecycleEvents).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          event: "llm_token",
          source: "llm:token",
          surface: "internal_candidate",
        }),
        expect.objectContaining({
          event: "llm_reset",
          reasonKind: "tool_round",
          source: "llm:reset",
          surface: "internal_candidate",
        }),
      ]),
    );
  });

  it("visible llm:reset clears the stream buffer and empties the assistant slot", async () => {
    await mountHook();

    // seed some streamed content first
    await act(async () => {
      handlers.token?.({
        request_id: "req-1",
        token: "前导文本",
        index: 0,
        surface: "visible_answer",
      });
    });
    await act(async () => {
      flushRaf();
    });
    expect(streamBuf.current).toBe("前导文本");

    // fire reset
    await act(async () => {
      handlers.reset?.({
        request_id: "req-1",
        surface: "visible_answer",
      });
    });

    expect(streamBuf.current).toBe("");
    const last = messagesState[messagesState.length - 1];
    expect(last?.role).toBe("assistant");
    expect(last?.content).toBe("");
  });

  it("records safe lifecycle entries for token, done, and reset message mutations", async () => {
    await mountHook();

    await act(async () => {
      handlers.token?.({
        request_id: "req-1",
        token: "不应泄漏",
        index: 0,
        surface: "visible_answer",
      });
    });
    await act(async () => {
      flushRaf();
    });
    await act(async () => {
      handlers.done?.({ request_id: "req-1" });
    });
    await act(async () => {
      handlers.reset?.({
        request_id: "req-1",
        reason_kind: "tool_round",
        surface: "visible_answer",
      });
    });

    expect(lifecycleEvents).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          event: "llm_token",
          phase: "frontend_stream",
          requestId: "req-1",
          source: "llm:token",
        }),
        expect.objectContaining({
          event: "message_mutation",
          mutation: "push_assistant",
          phase: "frontend_stream",
          source: "llm_token_raf",
        }),
        expect.objectContaining({
          event: "message_mutation",
          mutation: "clear_assistant",
          phase: "frontend_stream",
          reasonKind: "tool_round",
          source: "llm_reset",
        }),
      ]),
    );
    const serialized = JSON.stringify(lifecycleEvents);
    expect(serialized).not.toContain("不应泄漏");
    expect(serialized).toContain("contentSummary");
    expect(serialized).toContain("hash");
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

  it("ai:retry_status updates activity hint without adding transcript messages", async () => {
    messagesState = [{ role: "user", content: "你好" }];
    await mountHook();
    const firstMessagesRef = messagesState;

    await act(async () => {
      handlers.retry?.({
        request_id: "req-1",
        attempt: 1,
        max_attempts: 3,
        delay_ms: 1000,
        reason_kind: "http_503",
        status_code: 503,
      });
    });

    expect(messagesState).toBe(firstMessagesRef);
    expect(messagesState).toEqual([{ role: "user", content: "你好" }]);
    expect(activityHintState).toBe("重试中（1/3），约 1 秒后继续。");
  });

  it("ai:harness_trace surfaces phase progress with durations", async () => {
    await mountHook();

    await act(async () => {
      handlers.trace?.({
        request_id: "req-1",
        round: 2,
        phase: "tool_complete",
        tool_name: "web_search",
        status: "ok",
        duration_ms: 1530,
      });
    });

    expect(activityHintState).toBe("联网检索完成，用时 1.5 秒。");
    expect(messagesState).toEqual([]);
    expect(processEventsState).toHaveLength(1);
    expect(processEventsState[0]).toMatchObject({
      kind: "trace",
      label: "联网检索完成，用时 1.5 秒。",
      requestId: "req-1",
      round: 2,
      durationMs: 1530,
    });
  });

  it("coalesces reset and tool trace events for the same round and tool", async () => {
    await mountHook();

    await act(async () => {
      handlers.reset?.({
        request_id: "req-1",
        reason_kind: "tool_round",
        round: 2,
        surface: "internal_candidate",
      });
      handlers.trace?.({
        request_id: "req-1",
        round: 2,
        phase: "tool_start",
        tool_name: "web_search",
        status: "running",
      });
      handlers.trace?.({
        request_id: "req-1",
        round: 2,
        phase: "tool_complete",
        tool_name: "web_search",
        status: "ok",
        duration_ms: 1530,
      });
    });

    expect(messagesState).toEqual([]);
    expect(processEventsState).toHaveLength(1);
    expect(processEventsState[0]).toMatchObject({
      kind: "trace",
      requestId: "req-1",
      round: 2,
      status: "ok",
      durationMs: 1530,
    });
    expect(processEventsState[0]?.label).toContain("1.5");
  });

  it("ai:thinking records safe process metadata without transcript content", async () => {
    await mountHook();

    await act(async () => {
      handlers.thinking?.({
        request_id: "req-1",
        round: 3,
        has_internal_thinking: true,
        content_chars: 128,
      });
    });

    expect(messagesState).toEqual([]);
    expect(processEventsState).toHaveLength(1);
    expect(processEventsState[0]).toMatchObject({
      kind: "thinking",
      label: "模型正在推理",
      requestId: "req-1",
      round: 3,
    });
    expect(JSON.stringify(processEventsState)).not.toContain("content");
  });

  it("ai:retry_status ignores mismatched request ids", async () => {
    await mountHook();

    await act(async () => {
      handlers.retry?.({
        request_id: "other-req",
        attempt: 1,
        max_attempts: 3,
        delay_ms: 1000,
        reason_kind: "request_failed",
      });
    });

    expect(messagesState).toEqual([]);
    expect(activityHintState).toBeNull();
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
      handlers.token?.({
        request_id: "req-1",
        token: "不应泄漏",
        index: 0,
        surface: "visible_answer",
      });
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
      handlers.token?.({
        request_id: "req-1",
        token: "不应保留",
        index: 0,
        surface: "visible_answer",
      });
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

  it("non-final llm:error keeps the stream listener active for retry tokens", async () => {
    streamingState = true;
    await mountHook();

    await act(async () => {
      handlers.error?.({
        request_id: "req-1",
        error: "Stream read error: error decoding response body",
        final: false,
      });
    });

    expect(panelSendActive.current).toBe(true);
    expect(streamingState).toBe(true);
    expect(requestId.current).toBe("req-1");
    expect(messagesState).toEqual([]);
    expect(activityHintState).toBe("连接中断，正在重试流式响应…");

    await act(async () => {
      handlers.token?.({
        request_id: "req-1",
        token: "重试后继续",
        index: 0,
        surface: "visible_answer",
      });
    });
    await act(async () => {
      flushRaf();
    });

    expect(streamBuf.current).toBe("重试后继续");
    expect(messagesState[messagesState.length - 1]).toEqual({
      role: "assistant",
      content: "重试后继续",
    });
  });

  it("final llm:error removes an empty assistant placeholder before showing the error", async () => {
    messagesState = [
      { role: "user", content: "question" },
      { role: "assistant", content: "" },
    ];
    streamingState = true;
    await mountHook();

    await act(async () => {
      handlers.error?.({
        request_id: "req-1",
        error: "boom",
        final: true,
      });
    });

    expect(panelSendActive.current).toBe(false);
    expect(streamingState).toBe(false);
    expect(streamBuf.current).toBe("");
    expect(messagesState).toEqual([
      { role: "user", content: "question" },
      { role: "system", content: "错误: boom" },
    ]);
  });

  it("final llm:error preserves visible partial content before stopping", async () => {
    streamBuf.current = "已经生成的内容";
    messagesState = [{ role: "assistant", content: "已经生成的内容" }];
    streamingState = true;
    await mountHook();

    await act(async () => {
      handlers.error?.({
        request_id: "req-1",
        error: "Stream read error: error decoding response body",
        final: true,
      });
    });

    expect(panelSendActive.current).toBe(false);
    expect(streamingState).toBe(false);
    expect(streamBuf.current).toBe("已经生成的内容");
    expect(messagesState).toEqual([
      { role: "assistant", content: "已经生成的内容" },
      {
        role: "system",
        content:
          "错误: 模型流式连接中断，请稍后重试或切换模型。（已保留部分输出）",
      },
    ]);
    expect(processEventsState).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: "error",
          label: "stream body read failed",
          requestId: "req-1",
          status: "stream_body_read_failed",
        }),
      ]),
    );
  });

  it("unmount cancels a pending animation-frame flush", async () => {
    await mountHook();

    await act(async () => {
      handlers.token?.({
        request_id: "req-1",
        token: "卸载前",
        index: 0,
        surface: "visible_answer",
      });
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
