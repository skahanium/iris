import {
  act,
  createElement,
  type Dispatch,
  type RefObject,
  type SetStateAction,
} from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAssistantConversation } from "@/components/ai/hooks/useAssistantConversation";
import type { ChatLine, ImageAttachment } from "@/components/ai/AiMessageList";
import type { ContextPacket } from "@/types/ai";

type HookApi = ReturnType<typeof useAssistantConversation>;

function Harness({
  onReady,
  setInput,
  onInsertToEditor,
  selected,
  setPackets,
  setSelectedPacketIds,
  setActivityHint,
  textareaRef,
}: {
  onReady: (api: HookApi) => void;
  onInsertToEditor?: (content: string) => void;
  selected?: Set<number>;
  setActivityHint?: Dispatch<SetStateAction<string | null>>;
  setInput: (next: string | ((prev: string) => string)) => void;
  setPackets?: Dispatch<SetStateAction<ContextPacket[]>>;
  setSelectedPacketIds?: Dispatch<SetStateAction<string[]>>;
  textareaRef: RefObject<HTMLTextAreaElement | null>;
}) {
  const api = useAssistantConversation({
    actionIntent: "chat",
    bubbleSelection: {
      selected: selected ?? new Set<number>(),
      clear: vi.fn(),
    },
    clearCitationMiss: vi.fn(),
    clearContextReferences: vi.fn(),
    clearTaskSurfaces: vi.fn(),
    forceNewSessionRef: { current: false },
    onInsertToEditor: onInsertToEditor ?? vi.fn(),
    requestIdRef: { current: "req-1" },
    setActionState: vi.fn(),
    setActivityHint: setActivityHint ?? vi.fn(),
    setHarnessRequestId: vi.fn(),
    setInput,
    setPackets: setPackets ?? vi.fn(),
    setSelectedPacketIds: setSelectedPacketIds ?? vi.fn(),
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

  function render(
    overrides: {
      onInsertToEditor?: (content: string) => void;
      selected?: Set<number>;
      setPackets?: Dispatch<SetStateAction<ContextPacket[]>>;
      setSelectedPacketIds?: Dispatch<SetStateAction<string[]>>;
      setActivityHint?: Dispatch<SetStateAction<string | null>>;
    } = {},
  ) {
    root.render(
      createElement(Harness, {
        onReady: (value) => {
          api = value;
        },
        onInsertToEditor: overrides.onInsertToEditor,
        selected: overrides.selected,
        setInput: (next) => inputUpdates.push(next),
        setPackets: overrides.setPackets,
        setSelectedPacketIds: overrides.setSelectedPacketIds,
        setActivityHint: overrides.setActivityHint,
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

  it("appends user messages with readable inline mention labels", async () => {
    await act(async () => {
      api.appendUserMessage("summarize @[Research/Notes.md] please");
    });

    expect(api.messages).toEqual<ChatLine[]>([
      {
        role: "user",
        content: "summarize @Research/Notes.md please",
        mentions: [
          {
            raw: "@[Research/Notes.md]",
            value: "Research/Notes.md",
            kind: "file",
            label: "Research/Notes.md",
          },
        ],
      },
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

  it("restores evidence packets when loading a session", async () => {
    const packet: ContextPacket = {
      id: "packet-1",
      source_type: "note",
      source_path: "Sources/Case.md",
      title: "Case Source",
      heading_path: null,
      source_span: null,
      content_hash: "hash-1",
      excerpt: "重要证据",
      retrieval_reason: "semantic",
      score: 0.9,
      trust_level: "user_note",
      citation_label: "S1",
      stale: false,
    };
    const setPackets = vi.fn();
    const setSelectedPacketIds = vi.fn();

    await act(async () => {
      render({ setPackets, setSelectedPacketIds });
    });

    await act(async () => {
      api.handleLoadSession(42, [
        {
          role: "assistant",
          content: "answer with [S1]",
          evidencePackets: [packet],
        },
      ]);
    });

    expect(api.sessionId).toBe(42);
    expect(api.messages).toEqual([
      {
        role: "assistant",
        content: "answer with [S1]",
        evidencePackets: [packet],
      },
    ]);
    expect(setPackets).toHaveBeenLastCalledWith([packet]);
    expect(setSelectedPacketIds).toHaveBeenLastCalledWith([]);
  });
  it("prefers session ledger packets when loading a session", async () => {
    const messagePacket: ContextPacket = {
      id: "message-packet",
      source_type: "note",
      source_path: "Sources/Message.md",
      title: "Message Source",
      heading_path: null,
      source_span: null,
      content_hash: "hash-message",
      excerpt: "message evidence",
      retrieval_reason: "semantic",
      score: 0.5,
      trust_level: "user_note",
      citation_label: "[M1]",
      stale: false,
    };
    const ledgerPacket: ContextPacket = {
      ...messagePacket,
      id: "ledger-packet",
      source_path: "Sources/Ledger.md",
      title: "Ledger Source",
      citation_label: "[C1]",
    };
    const setPackets = vi.fn();

    await act(async () => {
      render({ setPackets });
    });

    await act(async () => {
      api.handleLoadSession(
        42,
        [
          {
            role: "assistant",
            content: "answer",
            evidencePackets: [messagePacket],
          },
        ],
        [ledgerPacket],
      );
    });

    expect(setPackets).toHaveBeenLastCalledWith([ledgerPacket]);
  });

  it("converts assistant citations when inserting selected messages", async () => {
    const packet: ContextPacket = {
      id: "packet-1",
      source_type: "note",
      source_path: "Sources/Case.md",
      title: "Case Source",
      heading_path: null,
      source_span: null,
      content_hash: "hash-1",
      excerpt: "important evidence",
      retrieval_reason: "semantic",
      score: 0.9,
      trust_level: "user_note",
      citation_label: "[C1]",
      stale: false,
    };
    const onInsertToEditor = vi.fn();

    await act(async () => {
      render({ onInsertToEditor, selected: new Set([0]) });
    });

    await act(async () => {
      api.setMessages([
        {
          role: "assistant",
          content: "answer [C1]",
          evidencePackets: [packet],
        },
      ]);
    });

    act(() => {
      api.handleInsertToEditor();
    });

    expect(onInsertToEditor).toHaveBeenCalledWith("answer [[Sources/Case]]");
  });

  it("warns when inserted assistant citations cannot be resolved", async () => {
    const onInsertToEditor = vi.fn();
    const setActivityHint = vi.fn();

    await act(async () => {
      render({
        onInsertToEditor,
        selected: new Set([0]),
        setActivityHint,
      });
    });

    await act(async () => {
      api.setMessages([
        {
          role: "assistant",
          content: "answer [C99]",
          evidencePackets: [],
        },
      ]);
    });

    act(() => {
      api.handleInsertToEditor();
    });

    expect(onInsertToEditor).toHaveBeenCalledWith("answer [C99]");
    expect(setActivityHint).toHaveBeenCalledWith("有引用未找到：[C99]");
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
