import { readFileSync } from "node:fs";

import type { Editor } from "@tiptap/react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { TipTapEditor } from "@/components/editor/TipTapEditor";
import { createEditorContextReference } from "@/lib/context-reference";
import type { FileSignatureResult } from "@/types/ipc";

async function signatureFor(content: string): Promise<FileSignatureResult> {
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

describe("TipTap committed source projection", () => {
  let root: Root | null = null;
  let container: HTMLDivElement | null = null;

  afterEach(() => {
    if (root) act(() => root?.unmount());
    container?.remove();
    root = null;
    container = null;
  });

  it("installs the full-note projection when hydrated", async () => {
    const markdown = "---\ntitle: 投影\n---\n正文包含中文范围";
    let editor: Editor | null = null;
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);

    await act(async () => {
      root?.render(
        <TipTapEditor
          initialBodyMarkdown="正文包含中文范围"
          committedSourceMarkdown={markdown}
          contentCacheKey="notes/projection.md"
          onContentReady={(ready) => {
            editor = ready;
          }}
        />,
      );
      await Promise.resolve();
    });
    expect(editor).not.toBeNull();
    editor!.commands.setTextSelection({ from: 3, to: 7 });

    const result = await createEditorContextReference({
      editor: editor!,
      kind: "selection",
      getFileSignature: vi.fn(() => signatureFor(markdown)),
    });

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.reference.filePath).toBe("notes/projection.md");
      expect(result.reference.utf8Range).not.toBeNull();
    }
  });

  it("rebuilds the projection from the saved Markdown after an edit", async () => {
    const initialMarkdown = "---\ntitle: 投影\n---\n\n保存前正文\n";
    const savedMarkdown = "---\ntitle: 投影\n---\n\n保存后的新正文\n";
    let editor: Editor | null = null;
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);

    await act(async () => {
      root?.render(
        <TipTapEditor
          initialBodyMarkdown="保存前正文"
          committedSourceMarkdown={initialMarkdown}
          contentCacheKey="notes/projection-save.md"
          onContentReady={(ready) => {
            editor = ready;
          }}
        />,
      );
      await Promise.resolve();
    });
    editor!.commands.setContent("<p>保存后的新正文</p>");
    await act(async () => {
      root?.render(
        <TipTapEditor
          initialBodyMarkdown="保存前正文"
          committedSourceMarkdown={savedMarkdown}
          contentCacheKey="notes/projection-save.md"
          onContentReady={(ready) => {
            editor = ready;
          }}
        />,
      );
      await Promise.resolve();
    });
    editor!.commands.setTextSelection({ from: 1, to: 5 });

    const result = await createEditorContextReference({
      editor: editor!,
      kind: "selection",
      getFileSignature: vi.fn(() => signatureFor(savedMarkdown)),
    });

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.reference.utf8Range).toEqual({ start: 23, end: 35 });
    }
  });

  it("plumbs the committed full note through the workspace surface", () => {
    const app = readFileSync("src/App.impl.tsx", "utf8");
    const workspace = readFileSync(
      "src/components/layout/AppEditorWorkspace.tsx",
      "utf8",
    );

    expect(app).toContain("committedSourceMarkdown={markdown}");
    expect(workspace).toContain(
      "pendingNoteOpen?.content ?? committedSourceMarkdown",
    );
    expect(workspace).toContain(
      "committedSourceMarkdown={snapshot.committedSourceMarkdown}",
    );
  });
});
