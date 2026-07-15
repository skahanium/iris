import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiSourceHighlightExtension } from "@/components/editor/extensions/AiSourceHighlightExtension";
import { AiStreamExtension } from "@/components/editor/extensions/AiStreamExtension";
import { getActiveAiStreamAttrs, useInlineAi } from "@/hooks/useInlineAi";
import {
  assistantRunControl,
  assistantRunStart,
  listenAssistantRunEvent,
} from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({
  assistantRunControl: vi.fn(),
  assistantRunStart: vi.fn(),
  listenAssistantRunEvent: vi.fn(),
}));

const mockAssistantRunControl = vi.mocked(assistantRunControl);
const mockAssistantRunStart = vi.mocked(assistantRunStart);
const mockListenAssistantRunEvent = vi.mocked(listenAssistantRunEvent);

const editorExtensions = [
  StarterKit.configure({ codeBlock: false }),
  AiSourceHighlightExtension,
  AiStreamExtension,
];

function createEditor(text = "selected source text"): Editor {
  return new Editor({
    extensions: editorExtensions,
    content: {
      type: "doc",
      content: [{ type: "paragraph", content: [{ type: "text", text }] }],
    },
  });
}

function acceptedRun() {
  return {
    runId: "run-inline-1",
    turnId: "turn-inline-1",
    session: { domain: "classified" as const, sessionKey: "session-inline-1" },
    state: "accepted" as const,
    stateVersion: 1,
  };
}

describe("AiStreamExtension", () => {
  let editor: Editor;

  beforeEach(() => {
    editor = createEditor();
  });

  afterEach(() => {
    editor.destroy();
  });

  it("keeps the selected source visible and adds an ai stream below it", () => {
    editor.commands.setTextSelection({ from: 1, to: 9 });

    expect(
      editor.commands.insertAiStreamBelowSelection({
        originalText: "selected",
        action: "rewrite",
        sourceFrom: 1,
        sourceTo: 9,
      }),
    ).toBe(true);
    expect(editor.getText()).toContain("selected source text");
    expect(editor.state.doc.content.lastChild?.type.name).toBe("aiStream");
  });

  it("exposes ai stream metadata for a retry", () => {
    editor.commands.setTextSelection({ from: 1, to: 9 });
    editor.commands.insertAiStreamBelowSelection({
      originalText: "selected",
      action: "rewrite",
      sourceFrom: 1,
      sourceTo: 9,
    });

    expect(getActiveAiStreamAttrs(editor)).toEqual({
      action: "rewrite",
      originalText: "selected",
    });
  });
});

describe("useInlineAi", () => {
  let root: Root;
  let container: HTMLDivElement;
  let api: ReturnType<typeof useInlineAi>;
  let mutationBlocked = false;
  let emit:
    | ((
        event: Parameters<Parameters<typeof listenAssistantRunEvent>[0]>[0],
      ) => void)
    | null;

  function Host() {
    api = useInlineAi({
      domain: "classified",
      isMutationBlocked: () => mutationBlocked,
    });
    return null;
  }

  beforeEach(() => {
    mockAssistantRunControl.mockReset();
    mockAssistantRunStart.mockReset();
    mockListenAssistantRunEvent.mockReset();
    emit = null;
    mutationBlocked = false;
    mockAssistantRunStart.mockResolvedValue(acceptedRun());
    mockListenAssistantRunEvent.mockImplementation(async (handler) => {
      emit = handler;
      return () => undefined;
    });
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    act(() => {
      root.render(createElement(Host));
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
  });

  it("starts one Run with an explicit selection, never the legacy note path", async () => {
    const editor = createEditor(
      "selected source text and unrelated document body",
    );
    editor.commands.setTextSelection({ from: 1, to: 9 });

    await act(async () => {
      await api.run(editor, "rewrite");
    });

    expect(mockAssistantRunStart).toHaveBeenCalledTimes(1);
    expect(mockAssistantRunStart).toHaveBeenCalledWith(
      expect.objectContaining({
        message: expect.stringContaining("selected"),
        securityDomain: "classified",
        webEnabled: false,
        explicitAction: expect.objectContaining({ effect: "draft" }),
        explicitReferences: [
          expect.objectContaining({
            kind: "selection",
            filePath: null,
            editorRange: null,
          }),
        ],
      }),
    );
    const request = mockAssistantRunStart.mock.calls[0]?.[0];
    expect(request?.message).not.toContain("unrelated document body");
    expect(JSON.stringify(request)).not.toContain(".classified/secret.md");
    editor.destroy();
  });

  it("renders only unified Run events into the visible ai stream", async () => {
    const editor = createEditor();
    editor.commands.setTextSelection({ from: 1, to: 9 });

    await act(async () => {
      await api.run(editor, "rewrite");
    });
    act(() => {
      emit?.({
        runId: "run-inline-1",
        seq: 2,
        stateVersion: 2,
        timestamp: "2026-07-13T12:00:00.000Z",
        type: "content_delta",
        payload: { kind: "content_delta", delta: "rewritten" },
      });
    });
    await act(async () => {
      await new Promise((resolve) => requestAnimationFrame(resolve));
    });
    act(() => {
      emit?.({
        runId: "run-inline-1",
        seq: 3,
        stateVersion: 3,
        timestamp: "2026-07-13T12:00:01.000Z",
        type: "completed",
        payload: { kind: "completed", messageId: "message-1" },
      });
    });

    expect(editor.getText()).toContain("rewritten");
    let status = "";
    editor.state.doc.descendants((node) => {
      if (node.type.name === "aiStream") status = node.attrs.status as string;
    });
    expect(status).toBe("ready");
    editor.destroy();
  });

  it("starts slash commands as a Run without passing editor markdown", async () => {
    const editor = createEditor("document body must stay out of slash Run");

    await act(async () => {
      await api.runSlash(editor, "summarize");
    });

    expect(mockAssistantRunStart).toHaveBeenCalledWith(
      expect.objectContaining({
        explicitReferences: [],
        explicitAction: { effect: "draft" },
        securityDomain: "classified",
      }),
    );
    const request = mockAssistantRunStart.mock.calls[0]?.[0];
    expect(request?.message).not.toContain("document body must stay out");
    expect(JSON.stringify(request)).not.toContain(
      "document body must stay out",
    );
    editor.destroy();
  });

  it("cancels an active Run through assistantRunControl", async () => {
    const editor = createEditor();
    editor.commands.setTextSelection({ from: 1, to: 9 });
    await act(async () => {
      await api.run(editor, "rewrite");
      await api.abort();
    });

    expect(mockAssistantRunControl).toHaveBeenCalledWith({
      session: { domain: "classified", sessionKey: "session-inline-1" },
      runId: "run-inline-1",
      expectedStateVersion: 1,
      action: { type: "cancel" },
    });
    editor.destroy();
  });

  it("aborts and detaches an active stream when the persistence barrier starts", async () => {
    const editor = createEditor();
    editor.commands.setTextSelection({ from: 1, to: 9 });

    await act(async () => {
      await api.run(editor, "rewrite");
    });
    const beforeBarrier = editor.getText();

    mutationBlocked = true;
    act(() => {
      api.abortAndDetach();
      emit?.({
        runId: "run-inline-1",
        seq: 2,
        stateVersion: 2,
        timestamp: "2026-07-13T12:00:00.000Z",
        type: "content_delta",
        payload: { kind: "content_delta", delta: "must not enter editor" },
      });
    });
    await act(async () => {
      await new Promise((resolve) => requestAnimationFrame(resolve));
    });

    expect(editor.getText()).toBe(beforeBarrier);
    expect(mockAssistantRunControl).toHaveBeenCalledWith({
      session: { domain: "classified", sessionKey: "session-inline-1" },
      runId: "run-inline-1",
      expectedStateVersion: 1,
      action: { type: "cancel" },
    });
    editor.destroy();
  });
});
