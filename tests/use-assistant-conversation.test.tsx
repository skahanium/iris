import { act, createElement, type RefObject } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAssistantConversation } from "@/components/ai/hooks/useAssistantConversation";
import type { ChatLine, ImageAttachment } from "@/components/ai/AiMessageList";

type HookApi = ReturnType<typeof useAssistantConversation>;

function Harness({
  onReady,
  setInput,
  textareaRef,
}: {
  onReady: (api: HookApi) => void;
  setInput: (next: string | ((prev: string) => string)) => void;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
}) {
  const api = useAssistantConversation({
    actionIntent: "chat",
    bubbleSelection: {
      selected: new Set<number>(),
      clear: vi.fn(),
    },
    clearCitationMiss: vi.fn(),
    clearTaskSurfaces: vi.fn(),
    forceNewSessionRef: { current: false },
    onInsertToEditor: vi.fn(),
    requestIdRef: { current: "req-1" },
    setActionState: vi.fn(),
    setActivityHint: vi.fn(),
    setHarnessRequestId: vi.fn(),
    setInput,
    setPackets: vi.fn(),
    setSelectedPacketIds: vi.fn(),
    setStreaming: vi.fn(),
    streamBufRef: { current: "buffer" },
    textareaRef,
  });
  onReady(api);
  return null;
}

describe("useAssistantConversation", () => {
  let container: HTMLDivElement;
  let root: Root;
  let textarea: HTMLTextAreaElement;
  let api!: HookApi;
  let inputUpdates: Array<string | ((prev: string) => string)>;

  function render() {
    root.render(
      createElement(Harness, {
        onReady: (value) => {
          api = value;
        },
        setInput: (next) => inputUpdates.push(next),
        textareaRef: { current: textarea },
      }),
    );
  }

  beforeEach(async () => {
    container = document.createElement("div");
    document.body.appendChild(container);
    textarea = document.createElement("textarea");
    inputUpdates = [];
    root = createRoot(container);
    await act(async () => {
      render();
    });
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("appends user messages with readable mention labels instead of raw tokens", async () => {
    await act(async () => {
      api.appendUserMessage("summarize @[Research/Notes.md] please");
    });

    expect(api.messages).toEqual<ChatLine[]>([
      { role: "user", content: "summarize 「Research/Notes.md」 please" },
    ]);
  });

  it("does not prefix image attachment messages with a redundant image marker", async () => {
    const image: ImageAttachment = {
      id: "img-1",
      dataBase64: "abc123",
      mimeType: "image/png",
      fileName: "sand.png",
      sizeBytes: 123,
    };

    await act(async () => {
      api.appendUserMessage("这是一张什么样的图片？", [image]);
    });

    expect(api.messages).toEqual<ChatLine[]>([
      {
        role: "user",
        content: "这是一张什么样的图片？",
        images: [image],
      },
    ]);
  });

  it("quotes selected text into the composer input and focuses the textarea", () => {
    const focus = vi.spyOn(textarea, "focus");

    act(() => {
      api.handleQuoteToInput("alpha\nbeta");
    });

    expect(typeof inputUpdates[0]).toBe("function");
    expect((inputUpdates[0] as (prev: string) => string)("draft")).toBe(
      "draft\n\n> alpha\n> beta\n\n",
    );
    expect(focus).toHaveBeenCalled();
  });

  it("resets conversation state for a new chat", async () => {
    await act(async () => {
      api.setMessages([{ role: "assistant", content: "old" }]);
      api.setSessionId(42);
      api.setSessionTokenUsage({
        prompt_tokens: 1,
        completion_tokens: 1,
        total_tokens: 2,
      });
    });

    await act(async () => {
      api.handleNewChat();
    });

    expect(api.messages).toEqual([]);
    expect(api.sessionId).toBeNull();
    expect(api.sessionTokenUsage).toBeNull();
  });
});
