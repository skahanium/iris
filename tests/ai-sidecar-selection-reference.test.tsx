import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { readFileSync } from "node:fs";
import { act, createElement, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAiSidecarBridge } from "@/hooks/useAiSidecarBridge";
import {
  EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
  installEditorMarkdownSourceProjection,
} from "@/lib/context-reference";
import { fileSignature } from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({
  fileSignature: vi.fn(),
  settingsGet: vi.fn(async () => false),
  settingsSet: vi.fn(async () => undefined),
  webEvidenceProvidersList: vi.fn(async () => []),
}));

const mockFileSignature = vi.mocked(fileSignature);

describe("assistant sidecar selection reference bridge", () => {
  let root: Root;
  let container: HTMLDivElement;
  let editor: Editor;
  let dirty = false;
  let status = "";
  let api: ReturnType<typeof useAiSidecarBridge>;
  const markdown = "侧栏共享精确选区";

  function Host() {
    const editorRef = useRef<Editor | null>(editor);
    api = useAiSidecarBridge({
      editorRef,
      isDocumentDirty: () => dirty,
      setAiStatus: (message) => {
        status = message;
      },
    });
    return null;
  }

  beforeEach(async () => {
    dirty = false;
    status = "";
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
    mockFileSignature.mockResolvedValue({
      byteLength: new TextEncoder().encode(markdown).length,
      contentHash: Array.from(new Uint8Array(digest), (byte) =>
        byte.toString(16).padStart(2, "0"),
      ).join(""),
      isLocked: false,
      modifiedMs: 1,
    });
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    await act(async () => {
      root.render(createElement(Host));
      await Promise.resolve();
      await Promise.resolve();
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    editor.destroy();
    container.remove();
  });

  it("exposes the same disk-verified ContextReference for a sidecar consumer", async () => {
    await act(async () => {
      await api.sendSelectionToAi({ prefill: "解释选区" });
    });

    expect(api.editorSelectionReference).toMatchObject({
      kind: "selection",
      filePath: "notes/sidecar.md",
      contentHash: expect.stringMatching(/^[0-9a-f]{64}$/u),
      utf8Range: { start: 0, end: 12 },
      excerpt: "",
    });
    expect(api.prefillMessage).toBe("解释选区");
  });

  it("keeps the sidecar closed and exposes no body when the note is dirty", async () => {
    dirty = true;

    await act(async () => {
      await api.sendSelectionToAi();
    });

    expect(api.editorSelectionReference).toBeNull();
    expect(status).toBe(EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE);
    expect(JSON.stringify(api)).not.toContain(markdown);
  });

  it("plumbs the one-shot reference from App into the unified sender", () => {
    const app = readFileSync("src/App.impl.tsx", "utf8");
    const slot = readFileSync(
      "src/components/layout/AppAiPanelSlot.tsx",
      "utf8",
    );
    const panel = readFileSync(
      "src/components/ai/UnifiedAssistantPanel.impl.tsx",
      "utf8",
    );

    expect(app).toContain(
      "editorSelectionReference={editorSelectionReference}",
    );
    expect(app).toContain(
      "consumeEditorSelectionReference={consumeEditorSelectionReference}",
    );
    expect(slot).toContain(
      "oneShotContextReference={editorSelectionReference}",
    );
    expect(panel).toContain("oneShotContextReference");
    expect(panel).toContain("consumeOneShotContextReference");
  });
});
