import { act } from "react";
import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const fileDiscard = vi.fn();
const fileRead = vi.fn();
const createDefaultNote = vi.fn();

vi.mock("@/lib/ipc", () => ({
  fileDiscard: (...args: unknown[]) => fileDiscard(...args),
  fileRead: (...args: unknown[]) => fileRead(...args),
}));

vi.mock("@/lib/note-create", () => ({
  createDefaultNote: (options: unknown) => createDefaultNote(options),
}));

vi.mock("@/lib/document-title", () => ({
  displayTitleFromMarkdown: (_md: string, fallback: string) => fallback,
  resolveDocumentTitle: async (_path: string, hint?: string) =>
    hint?.trim() || "未命名文档",
}));

vi.mock("@/lib/markdown", async () => {
  const actual =
    await vi.importActual<typeof import("@/lib/markdown")>("@/lib/markdown");
  return {
    ...actual,
    parseNoteForEditor: (markdown: string) => {
      const match = markdown.match(/^---\n([\s\S]*?)\n---\n?([\s\S]*)$/);
      const yaml = match?.[1] ?? null;
      const bodyMd = match?.[2] ?? markdown;
      const titleMatch = yaml?.match(/title:\s*"?([^"\n]+)"?/);
      return {
        bodyMarkdown: bodyMd,
        bodyMd,
        frontmatterYaml: yaml,
        title: titleMatch?.[1] ?? null,
        yaml,
      };
    },
    stripLeadingBodyTitleHeading: (body: string) => body,
  };
});

import type { TabItem } from "@/components/layout/TabBar";
import { useTabManager } from "@/hooks/useTabManager";

const EMPTY_MD = '---\ntitle: "未命名文档"\n---\n\n';

function fileReadResult(content: string, isLocked = false) {
  return { content, isLocked };
}

function Harness({
  apiRef,
}: {
  apiRef: { current: ReturnType<typeof useTabManager> | null };
}) {
  const api = useTabManager();
  apiRef.current = api;
  return null;
}

async function runAndWait(
  apiRef: { current: ReturnType<typeof useTabManager> | null },
  path: string,
  action: () => Promise<void>,
) {
  await act(async () => {
    await action();
  });
  expect(apiRef.current!.activePath).toBe(path);
  expect(apiRef.current!.pendingNoteOpen).toBeNull();
}

async function openAndCommit(
  apiRef: { current: ReturnType<typeof useTabManager> | null },
  path: string,
  titleHint?: string,
) {
  await runAndWait(apiRef, path, () =>
    apiRef.current!.openFile(path, titleHint),
  );
}

describe("useTabManager handleNewNote", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    fileDiscard.mockReset();
    fileRead.mockReset();
    createDefaultNote.mockReset();
    fileDiscard.mockResolvedValue(undefined);
    fileRead.mockResolvedValue(fileReadResult(EMPTY_MD));
    createDefaultNote.mockResolvedValue({
      path: "未命名文档.md",
      title: "未命名文档",
    });
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("creates a note when no tab is active", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await runAndWait(apiRef, "未命名文档.md", () =>
      apiRef.current!.handleNewNote(),
    );

    expect(createDefaultNote).toHaveBeenCalledTimes(1);
    expect(apiRef.current!.activePath).toBe("未命名文档.md");
  });

  it("discards an empty active tab before creating the next note", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    createDefaultNote.mockResolvedValueOnce({
      path: "未命名文档（1）.md",
      title: "未命名文档（1）",
    });

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndCommit(apiRef, "未命名文档.md", "未命名文档");
    expect(apiRef.current!.activePath).toBe("未命名文档.md");

    await runAndWait(apiRef, "未命名文档（1）.md", () =>
      apiRef.current!.handleNewNote(),
    );

    expect(fileDiscard).toHaveBeenCalledWith("未命名文档.md");
    expect(createDefaultNote).toHaveBeenCalledWith({
      extraTakenTitles: [],
    });
    expect(apiRef.current!.activePath).toBe("未命名文档（1）.md");
    expect(
      apiRef.current!.tabs.some((t: TabItem) => t.path === "未命名文档.md"),
    ).toBe(false);
    expect(
      apiRef.current!.tabs.some(
        (t: TabItem) => t.path === "未命名文档（1）.md",
      ),
    ).toBe(true);
  });

  it("closes a tab and switches to the neighbor when closing the active tab", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    fileRead.mockImplementation(async (path: string) => {
      if (path === "a.md") {
        return fileReadResult("# A\n");
      }
      return fileReadResult(EMPTY_MD);
    });

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndCommit(apiRef, "a.md", "A");
    await openAndCommit(apiRef, "b.md", "B");

    await runAndWait(apiRef, "a.md", () => apiRef.current!.closeTab("b.md"));

    expect(apiRef.current!.activePath).toBe("a.md");
    expect(apiRef.current!.tabs.map((t: TabItem) => t.path)).toEqual(["a.md"]);
  });

  it("keeps unsaved live markdown when a placeholder note path is renamed", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };
    const liveMarkdown =
      '---\ntitle: "调度优化"\n---\n\n# 调度优化\n\n从网页粘贴进来的正文';

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await openAndCommit(apiRef, "未命名文档.md", "未命名文档");

    await act(async () => {
      apiRef.current!.replaceOpenTabPath(
        "未命名文档.md",
        "调度优化.md",
        "调度优化",
        liveMarkdown,
      );
    });

    expect(apiRef.current!.activePath).toBe("调度优化.md");
    expect(apiRef.current!.markdown).toBe(liveMarkdown);
    expect(apiRef.current!.getEditorMarkdown()).toBe(liveMarkdown);
    expect(apiRef.current!.getTabMarkdownCached("调度优化.md")).toBe(
      liveMarkdown,
    );
  });
});
