import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { waitFor } from "@testing-library/react";
import { act, createElement, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAiSidecarBridge } from "@/hooks/useAiSidecarBridge";
import {
  EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
  installEditorMarkdownSourceProjection,
} from "@/lib/context-reference";
import { fileSignature } from "@/lib/ipc";
import type { FileSignatureResult } from "@/types/ipc";

vi.mock("@/lib/ipc", () => ({
  fileSignature: vi.fn(),
  settingsGet: vi.fn(async () => false),
  settingsSet: vi.fn(async () => undefined),
  webEvidenceProvidersList: vi.fn(async () => []),
  webSearchRouteGet: vi.fn(async () => ({ candidateProviderIds: [] })),
}));

const mockFileSignature = vi.mocked(fileSignature);

describe("assistant sidecar selection reference bridge", () => {
  let root: Root | null;
  let container: HTMLDivElement;
  let editor: Editor;
  let dirty = false;
  let visible = true;
  let documentKey = "notes/sidecar.md";
  let api: ReturnType<typeof useAiSidecarBridge>;
  const markdown = "侧边栏选区引用测试";
  let validSignature: FileSignatureResult;

  function Host() {
    const editorRef = useRef<Editor | null>(editor);
    api = useAiSidecarBridge({
      editorRef,
      editor,
      documentKey,
      assistantVisible: visible,
      selectionEnabled: true,
      isDocumentDirty: () => dirty,
    });
    return null;
  }

  async function flushValidation() {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 260));
    });
  }

  beforeEach(async () => {
    dirty = false;
    visible = true;
    documentKey = "notes/sidecar.md";
    editor = new Editor({
      extensions: [StarterKit],
      content: `<p>${markdown}</p>`,
    });
    installEditorMarkdownSourceProjection(editor, {
      filePath: "notes/sidecar.md",
      committedMarkdown: markdown,
      bodyMarkdown: markdown,
    });
    editor.commands.setTextSelection({ from: 1, to: 5 });
    mockFileSignature.mockReset();
    const digest = await crypto.subtle.digest(
      "SHA-256",
      new TextEncoder().encode(markdown),
    );
    validSignature = {
      byteLength: new TextEncoder().encode(markdown).length,
      contentHash: Array.from(new Uint8Array(digest), (byte) =>
        byte.toString(16).padStart(2, "0"),
      ).join(""),
      isLocked: false,
      modifiedMs: 1,
    };
    mockFileSignature.mockResolvedValue(validSignature);
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    await act(async () => {
      root?.render(createElement(Host));
      await flushValidation();
    });
  });

  afterEach(() => {
    if (root) act(() => root?.unmount());
    root = null;
    editor.destroy();
    container.remove();
  });

  it("creates a disk-verified candidate as soon as a selection exists", async () => {
    await waitFor(() =>
      expect(api.editorSelectionCandidate?.status).toBe("ready"),
    );
    expect(api.editorSelectionCandidate).toMatchObject({
      status: "ready",
      preview: "侧边栏选区引用测试".slice(0, 4),
      reference: {
        kind: "selection",
        filePath: "notes/sidecar.md",
        contentHash: expect.stringMatching(/^[0-9a-f]{64}$/u),
      },
    });
  });

  it("clears the candidate when the selection is collapsed", () => {
    act(() => editor.commands.setTextSelection({ from: 3, to: 3 }));
    expect(api.editorSelectionCandidate).toBeNull();
    expect(api.editorSelectionReference).toBeNull();
  });

  it("shows save-required state for dirty documents without exposing content", async () => {
    dirty = true;
    act(() => editor.commands.setTextSelection({ from: 1, to: 6 }));
    await flushValidation();
    expect(api.editorSelectionCandidate).toMatchObject({
      status: "save_required",
      reference: null,
      message: EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
    });
    expect(JSON.stringify(api)).not.toContain(markdown);
  });

  it("ignores an older disk verification when the selection changes", async () => {
    let resolveFirst!: (value: FileSignatureResult) => void;
    let resolveSecond!: (value: FileSignatureResult) => void;
    mockFileSignature
      .mockImplementationOnce(
        () =>
          new Promise<FileSignatureResult>((resolve) => {
            resolveFirst = resolve;
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise<FileSignatureResult>((resolve) => {
            resolveSecond = resolve;
          }),
      );

    act(() => editor.commands.setTextSelection({ from: 1, to: 3 }));
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 220));
    });
    act(() => editor.commands.setTextSelection({ from: 5, to: 7 }));
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 220));
    });
    await waitFor(() => expect(mockFileSignature).toHaveBeenCalledTimes(2));

    await act(async () => {
      resolveSecond(validSignature);
      await Promise.resolve();
    });
    await waitFor(() =>
      expect(api.editorSelectionCandidate?.status).toBe("ready"),
    );
    await act(async () => {
      resolveFirst(validSignature);
      await Promise.resolve();
    });

    expect(api.editorSelectionReference?.editorRange).toEqual({
      from: 5,
      to: 7,
    });
  });

  it("removes the candidate while Agent is hidden and restores it on reopen", async () => {
    visible = false;
    await act(async () => {
      root?.render(createElement(Host));
    });
    expect(api.editorSelectionCandidate).toBeNull();

    visible = true;
    await act(async () => {
      root?.render(createElement(Host));
      await flushValidation();
    });
    await waitFor(() =>
      expect(api.editorSelectionCandidate?.status).toBe("ready"),
    );
  });

  it("clears the candidate immediately when the active document changes", async () => {
    await waitFor(() =>
      expect(api.editorSelectionCandidate?.status).toBe("ready"),
    );
    documentKey = "notes/other.md";
    await act(async () => {
      root?.render(createElement(Host));
    });
    expect(api.editorSelectionCandidate).toBeNull();
  });

  it("suppresses the current selection after dismissing it", () => {
    act(() => api.dismissEditorSelectionReference());
    expect(api.editorSelectionCandidate).toBeNull();
    act(() => editor.commands.setTextSelection({ from: 1, to: 5 }));
    expect(api.editorSelectionCandidate).toBeNull();
  });

  it("allows a dismissed selection to re-establish after Agent is reopened", async () => {
    await waitFor(() =>
      expect(api.editorSelectionCandidate?.status).toBe("ready"),
    );
    act(() => api.dismissEditorSelectionReference());
    visible = false;
    await act(async () => {
      root?.render(createElement(Host));
    });
    expect(api.editorSelectionCandidate).toBeNull();

    visible = true;
    await act(async () => {
      root?.render(createElement(Host));
    });
    await waitFor(() =>
      expect(api.editorSelectionCandidate?.status).toBe("ready"),
    );
  });
});
