import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useUnifiedAssistantSend } from "@/components/ai/hooks/useUnifiedAssistantSend";

const start = vi.fn();
let api: ReturnType<typeof useUnifiedAssistantSend> | null = null;
let root: Root | null = null;
let host: HTMLDivElement | null = null;

function Probe() {
  api = useUnifiedAssistantSend({
    aiDomain: "normal",
    input: "请总结这段资料",
    images: [],
    composerDisabled: false,
    session: { domain: "normal", sessionKey: "session-1" },
    contextReferences: [
      {
        id: "ref-1",
        kind: "note",
        filePath: "notes/brief.md",
        contentHash: "hash",
        utf8Range: null,
        editorRange: null,
        excerpt: "",
        stale: false,
      },
    ],
    webSearch: false,
    start,
    appendUserMessage: vi.fn(),
    ensureAssistantStreamSlot: vi.fn(),
    clearContextReferences: vi.fn(),
    setInput: vi.fn(),
    setImages: vi.fn(),
    setSession: vi.fn(),
    setStreaming: vi.fn(),
    setActivityHint: vi.fn(),
    setError: vi.fn(),
  });
  return null;
}

function ClassifiedProbe({ include = true }: { include?: boolean }) {
  api = useUnifiedAssistantSend({
    aiDomain: "classified",
    classifiedContextRef: "opaque-current-document-context",
    includeCurrentClassifiedDocument: include,
    input: "请分析当前文档",
    images: [],
    composerDisabled: false,
    session: null,
    contextReferences: [
      {
        id: "must-not-leave-classified-domain",
        kind: "note",
        filePath: "notes/normal.md",
        contentHash: "hash",
        utf8Range: null,
        editorRange: null,
        excerpt: "",
        stale: false,
      },
    ],
    webSearch: false,
    start,
    appendUserMessage: vi.fn(),
    ensureAssistantStreamSlot: vi.fn(),
    clearContextReferences: vi.fn(),
    setInput: vi.fn(),
    setImages: vi.fn(),
    setSession: vi.fn(),
    setStreaming: vi.fn(),
    setActivityHint: vi.fn(),
    setError: vi.fn(),
  });
  return null;
}

afterEach(() => {
  act(() => root?.unmount());
  host?.remove();
  root = null;
  host = null;
  api = null;
  start.mockReset();
});

describe("useUnifiedAssistantSend", () => {
  it("only starts a scene-free unified Run with explicit references", async () => {
    start.mockResolvedValue({
      runId: "run-1",
      turnId: "turn-1",
      session: { domain: "normal", sessionKey: "session-1" },
      state: "accepted",
      stateVersion: 1,
    });
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root?.render(<Probe />));

    await act(async () => {
      await api?.send();
    });

    expect(start).toHaveBeenCalledWith({
      clientRequestId: expect.any(String),
      session: { domain: "normal", sessionKey: "session-1" },
      message: "请总结这段资料",
      explicitReferences: [expect.objectContaining({ id: "ref-1" })],
      webEnabled: false,
      securityDomain: "normal",
    });
  });

  it("requires a one-request classified attachment before dispatch", async () => {
    const setError = vi.fn();
    function MissingConsentProbe() {
      api = useUnifiedAssistantSend({
        aiDomain: "classified",
        classifiedContextRef: "opaque-current-document-context",
        includeCurrentClassifiedDocument: false,
        input: "分析当前文档",
        images: [],
        composerDisabled: false,
        session: null,
        contextReferences: [],
        webSearch: false,
        start,
        appendUserMessage: vi.fn(),
        ensureAssistantStreamSlot: vi.fn(),
        clearContextReferences: vi.fn(),
        setInput: vi.fn(),
        setImages: vi.fn(),
        setSession: vi.fn(),
        setStreaming: vi.fn(),
        setActivityHint: vi.fn(),
        setError,
      });
      return null;
    }
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root?.render(<MissingConsentProbe />));
    await act(async () => {
      await api?.send();
    });
    expect(start).not.toHaveBeenCalled();
    expect(setError).toHaveBeenCalledWith(
      expect.stringContaining("引用当前涉密文档"),
    );
  });

  it("rejects cross-domain references instead of sending them with a classified request", async () => {
    start.mockResolvedValue({
      runId: "run-classified",
      turnId: "turn-classified",
      session: { domain: "classified", sessionKey: "ephemeral" },
      state: "accepted",
      stateVersion: 0,
    });
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root?.render(<ClassifiedProbe />));
    await act(async () => {
      await api?.send();
    });
    expect(start).not.toHaveBeenCalled();
  });
});
