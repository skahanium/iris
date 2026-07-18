import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAssistantConversation } from "@/components/ai/hooks/useAssistantConversation";
import type { ChatLine } from "@/components/ai/AiMessageList";

const COPY_SELECTED_SUCCESS_TOAST =
  "\u5df2\u590d\u5236\u9009\u4e2d\u6d88\u606f";

const { retract, toast } = vi.hoisted(() => ({
  retract: vi.fn(),
  toast: vi.fn(),
}));
vi.mock("@/lib/ipc", () => ({ assistantSessionRetract: retract }));
vi.mock("@/components/ui/use-toast", () => ({ useToast: () => toast }));

let api: ReturnType<typeof useAssistantConversation> | null = null;
let host: HTMLDivElement | null = null;
let root: Root | null = null;
let bubbleSelectionClear = vi.fn();
let bubbleSelectionSelected = new Set<number>();

function Probe() {
  api = useAssistantConversation({
    bubbleSelection: {
      selected: bubbleSelectionSelected,
      clear: bubbleSelectionClear,
    },
    clearContextReferences: vi.fn(),
    clearTaskSurfaces: vi.fn(),
    setInput: vi.fn(),
    setStreaming: vi.fn(),
    textareaRef: { current: null },
  });
  return null;
}

beforeEach(() => {
  bubbleSelectionClear = vi.fn();
  bubbleSelectionSelected = new Set<number>();
  toast.mockReset();
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
});

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
  host = null;
  root = null;
  api = null;
  retract.mockReset();
});

function mountProbe() {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => root?.render(<Probe />));
}

describe("useAssistantConversation", () => {
  it("loads only an opaque unified session reference", () => {
    mountProbe();

    const messages: ChatLine[] = [{ role: "user", content: "已保存", seq: 1 }];
    act(() => {
      api?.handleLoadSession(
        { domain: "normal", sessionKey: "session-1" },
        messages,
      );
    });

    expect(api?.runSession).toEqual({
      domain: "normal",
      sessionKey: "session-1",
    });
    expect(api?.messages).toHaveLength(1);
  });

  it("retracts persisted messages through assistant_session_retract", async () => {
    retract.mockResolvedValue(undefined);
    mountProbe();

    act(() => {
      api?.handleLoadSession({ domain: "classified", sessionKey: "cef-1" }, [
        { role: "user", content: "第一句", seq: 1 },
        { role: "assistant", content: "回答", seq: 2 },
      ]);
    });
    await act(async () => {
      await api?.handleRetract(1);
    });

    expect(retract).toHaveBeenCalledWith({
      session: { domain: "classified", sessionKey: "cef-1" },
      fromSeq: 2,
    });
    expect(api?.messages).toHaveLength(1);
  });

  it("shows readable copy-success toast when copying selected messages", async () => {
    bubbleSelectionSelected = new Set([0]);
    mountProbe();

    act(() => {
      api?.handleLoadSession({ domain: "normal", sessionKey: "session-1" }, [
        {
          role: "assistant",
          content: "\u590d\u5236\u8fd9\u4e00\u6bb5\u6587\u5b57",
        },
      ]);
    });
    await act(async () => {
      await api?.handleCopySelected();
    });

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
      "\u590d\u5236\u8fd9\u4e00\u6bb5\u6587\u5b57",
    );
    expect(toast).toHaveBeenCalledWith(COPY_SELECTED_SUCCESS_TOAST, {
      tone: "success",
    });
    expect(bubbleSelectionClear).toHaveBeenCalled();
  });
});
