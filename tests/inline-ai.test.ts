import { readFileSync } from "node:fs";

import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiSourceHighlightExtension } from "@/components/editor/extensions/AiSourceHighlightExtension";
import { AiStreamExtension } from "@/components/editor/extensions/AiStreamExtension";
import { getActiveAiStreamAttrs, useInlineAi } from "@/hooks/useInlineAi";
import {
  EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
  installEditorMarkdownSourceProjection,
} from "@/lib/context-reference";
import {
  assistantRunControl,
  assistantRunStart,
  fileSignature,
  listenAssistantRunEvent,
} from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({
  assistantRunControl: vi.fn(),
  assistantRunStart: vi.fn(),
  fileSignature: vi.fn(),
  listenAssistantRunEvent: vi.fn(),
}));

const mockAssistantRunControl = vi.mocked(assistantRunControl);
const mockAssistantRunStart = vi.mocked(assistantRunStart);
const mockFileSignature = vi.mocked(fileSignature);
const mockListenAssistantRunEvent = vi.mocked(listenAssistantRunEvent);

const editorExtensions = [
  StarterKit.configure({ codeBlock: false }),
  AiSourceHighlightExtension,
  AiStreamExtension,
];

let diskMarkdown = "";

async function signatureFor(content: string) {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(content),
  );
  return {
    byteLength: new TextEncoder().encode(content).length,
    contentHash: Array.from(new Uint8Array(digest), (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join(""),
    isLocked: false,
    modifiedMs: 1,
  };
}

async function sha256ForExpectation(content: string): Promise<string> {
  return (await signatureFor(content)).contentHash;
}

function createEditor(text = "selected source text"): Editor {
  const editor = new Editor({
    extensions: editorExtensions,
    content: {
      type: "doc",
      content: [{ type: "paragraph", content: [{ type: "text", text }] }],
    },
  });
  diskMarkdown = text;
  installEditorMarkdownSourceProjection(editor, {
    filePath: "notes/inline.md",
    committedMarkdown: text,
    bodyMarkdown: text,
  });
  return editor;
}

function acceptedRun() {
  return {
    runId: "run-inline-1",
    turnId: "turn-inline-1",
    session: { domain: "normal" as const, sessionKey: "session-inline-1" },
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
  let documentDirty = false;
  let status = "";
  let emit:
    | ((
        event: Parameters<Parameters<typeof listenAssistantRunEvent>[0]>[0],
      ) => void)
    | null;

  function Host() {
    api = useInlineAi({
      domain: "normal",
      isDocumentDirty: () => documentDirty,
      isMutationBlocked: () => mutationBlocked,
      onStatus: (message) => {
        status = message;
      },
    });
    return null;
  }

  beforeEach(() => {
    mockAssistantRunControl.mockReset();
    mockAssistantRunStart.mockReset();
    mockFileSignature.mockReset();
    mockListenAssistantRunEvent.mockReset();
    emit = null;
    mutationBlocked = false;
    documentDirty = false;
    status = "";
    mockFileSignature.mockImplementation(() => signatureFor(diskMarkdown));
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
        securityDomain: "normal",
        webEnabled: false,
        explicitAction: expect.objectContaining({ effect: "draft" }),
        turn: expect.objectContaining({
          message: "请改写当前选区的文字，保持原意。",
          explicitReferences: [
            expect.objectContaining({
              kind: "selection",
              filePath: "notes/inline.md",
              contentHash: await sha256ForExpectation(diskMarkdown),
              utf8Range: expect.objectContaining({ start: 0 }),
            }),
          ],
          retrievalScope: { paths: [], pathPrefixes: [] },
          displayMentions: [],
        }),
      }),
    );
    const request = mockAssistantRunStart.mock.calls[0]?.[0];
    expect(JSON.stringify(request)).not.toContain("selected source text");
    expect(JSON.stringify(request)).not.toContain("unrelated document body");
    expect(JSON.stringify(request)).not.toContain("selectionSnapshot");
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
        explicitAction: { effect: "draft" },
        securityDomain: "normal",
        turn: expect.objectContaining({
          explicitReferences: [],
          retrievalScope: { paths: [], pathPrefixes: [] },
          displayMentions: [],
        }),
      }),
    );
    const request = mockAssistantRunStart.mock.calls[0]?.[0];
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
      session: { domain: "normal", sessionKey: "session-inline-1" },
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
      session: { domain: "normal", sessionKey: "session-inline-1" },
      runId: "run-inline-1",
      expectedStateVersion: 1,
      action: { type: "cancel" },
    });
    editor.destroy();
  });

  it("refuses a dirty selection before inserting a stream or starting a Run", async () => {
    const editor = createEditor("selected source text");
    editor.commands.setTextSelection({ from: 1, to: 9 });
    documentDirty = true;

    await act(async () => {
      await api.run(editor, "rewrite");
    });

    expect(mockAssistantRunStart).not.toHaveBeenCalled();
    expect(editor.state.doc.content.lastChild?.type.name).not.toBe("aiStream");
    expect(status).toBe(EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE);
    editor.destroy();
  });

  it("refuses to reuse a selection reference after the document becomes dirty", async () => {
    const editor = createEditor("selected source text");
    editor.commands.setTextSelection({ from: 1, to: 9 });

    await act(async () => {
      await api.run(editor, "rewrite");
    });
    documentDirty = true;
    await act(async () => {
      await api.retry(editor);
    });

    expect(mockAssistantRunStart).toHaveBeenCalledTimes(1);
    expect(status).toBe(EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE);
    editor.destroy();
  });

  it("receives the document-level dirty state from the app", () => {
    const app = readFileSync("src/App.impl.tsx", "utf8");
    expect(app).toContain("isDocumentDirty: () => dirtyRef.current");
  });
});
