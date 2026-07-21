import StarterKit from "@tiptap/starter-kit";
import { act } from "react";
import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type Editor } from "@tiptap/react";
import { Editor as TipTapEditor } from "@tiptap/core";

import { IrisDocument } from "@/components/editor/extensions/IrisDocument";
import { useOpenNote } from "@/hooks/useOpenNote";
import type { DocumentPersistenceMoveResult } from "@/lib/document-persistence-coordinator";
import { markdownBodyToEditorHtml } from "@/lib/markdown";

const documentRenameByTitle = vi.fn();

vi.mock("@/lib/ipc", () => ({
  documentRenameByTitle: (...args: unknown[]) => documentRenameByTitle(...args),
}));

function bodyEditor(markdown: string): Editor {
  return new TipTapEditor({
    extensions: [
      IrisDocument,
      StarterKit.configure({
        document: false,
        codeBlock: false,
        heading: { levels: [1, 2, 3, 4, 5, 6] },
      }),
    ],
    content: markdownBodyToEditorHtml(markdown),
  });
}

interface HarnessResult {
  getLiveMarkdown: () => string;
  noteTitle: string;
  onTitleBlur: (title?: string) => void;
}

type ReplaceOpenTabPath = (
  oldPath: string,
  newPath: string,
  title?: string,
  markdownOverride?: string,
) => void;

function Harness({
  markdown,
  editor,
  renamePersistedPath,
  outRef,
  replaceOpenTabPath,
}: {
  markdown: string;
  editor?: Editor | null;
  renamePersistedPath?: (
    oldPath: string,
    migrationPath: string,
    snapshot: string,
    move: () => Promise<DocumentPersistenceMoveResult>,
  ) => Promise<string>;
  outRef: { current: HarnessResult | null };
  replaceOpenTabPath?: ReplaceOpenTabPath;
}) {
  const api = useOpenNote({
    activePath: "untitled.md",
    editorContentTick: 1,
    activePathRef: { current: "untitled.md" },
    markdownRef: { current: markdown },
    frontmatterYamlRef: { current: null },
    editorRef: { current: editor ?? null },
    renamePersistedPath,
    updateTabTitle: vi.fn(),
    replaceOpenTabPath: replaceOpenTabPath ?? (() => undefined),
  });
  outRef.current = {
    getLiveMarkdown: api.getLiveMarkdown,
    noteTitle: api.noteTitle,
    onTitleBlur: api.onTitleBlur,
  };
  return null;
}

describe("useOpenNote single filename title", () => {
  let container: HTMLDivElement;
  let root: Root;
  let editor: Editor | undefined;

  beforeEach(() => {
    documentRenameByTitle.mockReset();
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(() => {
    editor?.destroy();
    act(() => root.unmount());
    container.remove();
  });

  it("removes legacy title from the snapshot while preserving the latest editor body", async () => {
    editor = bodyEditor("Latest body");
    const outRef: { current: HarnessResult | null } = { current: null };
    await act(async () => {
      root.render(
        createElement(Harness, {
          editor,
          markdown: "---\ntitle: Legacy\ntags: [work]\n---\nOld body",
          outRef,
        }),
      );
    });

    expect(outRef.current?.getLiveMarkdown()).toBe("Latest body\n");
  });

  it("uses the save barrier before atomically moving the filename", async () => {
    const order: string[] = [];
    const replaceOpenTabPath = vi.fn<ReplaceOpenTabPath>();
    documentRenameByTitle.mockImplementation(async () => {
      order.push("move");
      return {
        entry: { path: "Renamed.md" },
        indexStatus: "synced",
      };
    });
    const renamePersistedPath = vi.fn(
      async (
        oldPath: string,
        migrationPath: string,
        snapshot: string,
        move: () => Promise<DocumentPersistenceMoveResult>,
      ) => {
        expect(oldPath).toBe("untitled.md");
        expect(migrationPath).toBe("untitled.md");
        expect(snapshot).not.toContain("title:");
        order.push("barrier");
        await move();
        return snapshot;
      },
    );
    const outRef: { current: HarnessResult | null } = { current: null };
    await act(async () => {
      root.render(
        createElement(Harness, {
          markdown: "Body",
          outRef,
          renamePersistedPath,
          replaceOpenTabPath,
        }),
      );
    });

    act(() => outRef.current?.onTitleBlur("Renamed"));
    await vi.waitFor(() => expect(replaceOpenTabPath).toHaveBeenCalledTimes(1));
    expect(order).toEqual(["barrier", "move"]);
    expect(documentRenameByTitle).toHaveBeenCalledWith(
      "untitled.md",
      "Renamed",
    );
  });

  it("restores the existing filename for an empty title without moving", async () => {
    const outRef: { current: HarnessResult | null } = { current: null };
    await act(async () => {
      root.render(createElement(Harness, { markdown: "Body", outRef }));
    });

    act(() => outRef.current?.onTitleBlur(""));
    await vi.waitFor(() => expect(outRef.current?.noteTitle).toBe("untitled"));
    expect(documentRenameByTitle).not.toHaveBeenCalled();
  });

  it("skips rename IPC when the blurred title already matches the path stem", async () => {
    const renamePersistedPath = vi.fn();
    const outRef: { current: HarnessResult | null } = { current: null };
    await act(async () => {
      root.render(
        createElement(Harness, {
          markdown: "Body",
          outRef,
          renamePersistedPath,
        }),
      );
    });

    act(() => outRef.current?.onTitleBlur("untitled"));
    expect(outRef.current?.noteTitle).toBe("untitled");
    expect(documentRenameByTitle).not.toHaveBeenCalled();
    expect(renamePersistedPath).not.toHaveBeenCalled();
  });
});
