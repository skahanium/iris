import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useAssistantConversation } from "@/components/ai/hooks/useAssistantConversation";
import type { ChatLine } from "@/components/ai/AiMessageList";

const { retract } = vi.hoisted(() => ({ retract: vi.fn() }));
vi.mock("@/lib/ipc", () => ({ assistantSessionRetract: retract }));
vi.mock("@/components/ui/use-toast", () => ({ useToast: () => vi.fn() }));

let api: ReturnType<typeof useAssistantConversation> | null = null;
let host: HTMLDivElement | null = null;
let root: Root | null = null;

function Probe() {
  api = useAssistantConversation({
    bubbleSelection: { selected: new Set(), clear: vi.fn() },
    clearContextReferences: vi.fn(),
    clearTaskSurfaces: vi.fn(),
    setInput: vi.fn(),
    setStreaming: vi.fn(),
    textareaRef: { current: null },
  });
  return null;
}

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
  host = null;
  root = null;
  api = null;
  retract.mockReset();
});

describe("useAssistantConversation", () => {
  it("loads only an opaque unified session reference", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root?.render(<Probe />));

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
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root?.render(<Probe />));

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
});
