import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { act } from "react";
import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { IrisDocument } from "@/components/editor/extensions/IrisDocument";
import { useOpenNote } from "@/hooks/useOpenNote";
import type { DocumentPersistenceMoveResult } from "@/lib/document-persistence-coordinator";
import { markdownBodyToEditorHtml } from "@/lib/markdown";

const pathSyncSuggest = vi.fn();
const fileRename = vi.fn();

type ReplaceOpenTabPath = (
  oldPath: string,
  newPath: string,
  title?: string,
  markdownOverride?: string,
) => void;

vi.mock("@/lib/ipc", () => ({
  pathSyncSuggest: (...args: unknown[]) => pathSyncSuggest(...args),
  fileRename: (...args: unknown[]) => fileRename(...args),
}));

function bodyEditor(markdown: string): Editor {
  return new Editor({
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

function Harness({
  activePath,
  markdown,
  editorContentTick,
  outRef,
  editor,
  editorReady,
  dirty,
  titleFallback,
  renamePersistedPath,
  replaceOpenTabPath,
}: {
  activePath: string | null;
  markdown: string;
  editorContentTick: number;
  editor?: Editor | null;
  editorReady?: boolean;
  dirty?: boolean;
  titleFallback?: string;
  renamePersistedPath?: (
    path: string,
    newPath: string,
    markdown: string,
    move: () => Promise<DocumentPersistenceMoveResult>,
  ) => Promise<string>;
  replaceOpenTabPath?: ReplaceOpenTabPath;
  outRef: {
    current: {
      editorBodyMarkdown: string;
      bodyMarkdown: string;
      getLiveMarkdown: () => string;
      schedulePathSync: (path: string, title: string) => void;
    } | null;
  };
}) {
  const editorReadyRef = { current: editorReady ?? true };
  const api = useOpenNote({
    activePath,
    editorContentTick,
    activePathRef: { current: activePath },
    markdownRef: { current: markdown },
    frontmatterYamlRef: { current: null },
    editorRef: { current: editor ?? null },
    editorReadyRef,
    dirtyRef: { current: dirty ?? false },
    renamePersistedPath,
    updateTabTitle: vi.fn(),
    replaceOpenTabPath: replaceOpenTabPath ?? vi.fn<ReplaceOpenTabPath>(),
    titleFallback,
  });
  outRef.current = {
    editorBodyMarkdown: api.editorBodyMarkdown,
    bodyMarkdown: api.bodyMarkdown,
    getLiveMarkdown: api.getLiveMarkdown,
    schedulePathSync: api.schedulePathSync,
  };
  return null;
}

describe("useOpenNote editorBodyMarkdown", () => {
  let container: HTMLDivElement;
  let root: Root;
  let editor: Editor | undefined;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    pathSyncSuggest.mockReset();
    fileRename.mockReset();
  });

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
    vi.useRealTimers();
    vi.restoreAllMocks();
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("derives editor body on first render when markdown is already loaded", async () => {
    const md = '---\ntitle: "Note"\n---\n\nHello body';
    const outRef: {
      current: {
        editorBodyMarkdown: string;
        bodyMarkdown: string;
        getLiveMarkdown: () => string;
        schedulePathSync: (path: string, title: string) => void;
      } | null;
    } = { current: null };

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "note.md",
          markdown: md,
          editorContentTick: 1,
          outRef,
        }),
      );
    });

    expect(outRef.current?.editorBodyMarkdown.trim()).toBe("Hello body");
    expect(outRef.current?.bodyMarkdown.trim()).toBe("Hello body");
  });

  it("keeps the committed tab title when legacy markdown has no frontmatter title", async () => {
    const outRef = { current: null } as {
      current: ReturnType<typeof useOpenNote> | null;
    };

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "untitled-9.md",
          markdown: "Existing body without a frontmatter title.",
          editorContentTick: 1,
          outRef,
          titleFallback: "Already indexed title",
        }),
      );
    });

    expect(outRef.current?.getLiveMarkdown()).toContain(
      'title: "Already indexed title"',
    );
  });

  it("uses markdownRef body while the editor exists but is not ready and not dirty", async () => {
    const md = '---\ntitle: "Note"\n---\n\nOriginal loaded body';
    editor = bodyEditor("");
    const outRef: {
      current: {
        editorBodyMarkdown: string;
        bodyMarkdown: string;
        getLiveMarkdown: () => string;
        schedulePathSync: (path: string, title: string) => void;
      } | null;
    } = { current: null };

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "note.md",
          markdown: md,
          editor,
          editorReady: false,
          dirty: false,
          editorContentTick: 1,
          outRef,
        }),
      );
    });

    expect(outRef.current?.getLiveMarkdown()).toContain("Original loaded body");
  });

  it("serializes dirty editor body even before the editor is marked persistence-ready", async () => {
    const md = '---\ntitle: "Doc A"\n---\n\n';
    editor = bodyEditor("Body that only exists in TipTap.");
    const outRef: {
      current: {
        editorBodyMarkdown: string;
        bodyMarkdown: string;
        getLiveMarkdown: () => string;
        schedulePathSync: (path: string, title: string) => void;
      } | null;
    } = { current: null };

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "untitled.md",
          markdown: md,
          editor,
          editorReady: false,
          dirty: true,
          editorContentTick: 1,
          outRef,
        }),
      );
    });

    const live = outRef.current?.getLiveMarkdown() ?? "";
    expect(live).toContain('title: "Doc A"');
    expect(live).toContain("Body that only exists in TipTap.");
  });

  it("keeps dirty editor body in markdownOverride when title sync renames the path before ready", async () => {
    vi.useFakeTimers();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    pathSyncSuggest.mockResolvedValue({
      needs_sync: true,
      suggested_path: "doc-a.md",
      conflict_resolved: false,
    });
    fileRename.mockResolvedValue({
      entry: {
        id: 1,
        path: "doc-a.md",
        title: "Doc A",
        updated_at: "",
        word_count: 7,
      },
      contentHash: "doc-a",
      indexStatus: "synced",
    });

    const replaceOpenTabPath = vi.fn<ReplaceOpenTabPath>();
    const md = '---\ntitle: "Doc A"\n---\n\n';
    editor = bodyEditor("Rename must not drop this body.");
    const outRef: {
      current: {
        editorBodyMarkdown: string;
        bodyMarkdown: string;
        getLiveMarkdown: () => string;
        schedulePathSync: (path: string, title: string) => void;
      } | null;
    } = { current: null };

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "untitled.md",
          markdown: md,
          editor,
          editorReady: false,
          dirty: true,
          editorContentTick: 1,
          replaceOpenTabPath,
          outRef,
        }),
      );
    });

    act(() => {
      outRef.current?.schedulePathSync("untitled.md", "Doc A");
      vi.advanceTimersByTime(800);
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(replaceOpenTabPath).toHaveBeenCalledTimes(1);
    const markdownOverride = replaceOpenTabPath.mock.calls[0]?.[3] as string;
    expect(markdownOverride).toContain('title: "Doc A"');
    expect(markdownOverride).toContain("Rename must not drop this body.");
  });

  it("persists the complete live markdown before title sync renames the file", async () => {
    vi.useFakeTimers();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    pathSyncSuggest.mockResolvedValue({
      needs_sync: true,
      suggested_path: "renamed.md",
      conflict_resolved: false,
    });
    fileRename.mockResolvedValue({
      entry: {
        id: 1,
        path: "renamed.md",
        title: "Renamed",
        updated_at: "",
        word_count: 3,
      },
      contentHash: "renamed",
      indexStatus: "synced",
    });
    const renamePersistedPath = vi.fn<
      (
        path: string,
        newPath: string,
        markdown: string,
        move: () => Promise<DocumentPersistenceMoveResult>,
      ) => Promise<string>
    >(async (_path, _newPath, markdown, move) => {
      await move();
      return markdown;
    });
    editor = bodyEditor("Body must reach disk before rename.");
    const outRef = { current: null } as {
      current: ReturnType<typeof useOpenNote> | null;
    };

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "untitled.md",
          markdown: '---\ntitle: "Untitled"\n---\n\n',
          editor,
          editorReady: true,
          dirty: true,
          editorContentTick: 1,
          renamePersistedPath,
          outRef,
        }),
      );
    });

    act(() => {
      outRef.current?.schedulePathSync("untitled.md", "Renamed");
      vi.advanceTimersByTime(800);
    });
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(renamePersistedPath).toHaveBeenCalledTimes(1);
    expect(renamePersistedPath.mock.calls[0]?.[0]).toBe("untitled.md");
    expect(renamePersistedPath.mock.calls[0]?.[2]).toContain(
      "Body must reach disk before rename.",
    );
    expect(renamePersistedPath.mock.invocationCallOrder[0]).toBeLessThan(
      fileRename.mock.invocationCallOrder[0]!,
    );
  });
});
