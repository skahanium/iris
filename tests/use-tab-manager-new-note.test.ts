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

vi.mock("@/lib/markdown", () => ({
  extractFrontmatterYaml: () => null,
  stripLeadingBodyTitleHeading: (body: string) => body,
}));

import type { TabItem } from "@/components/layout/TabBar";
import { useTabManager } from "@/hooks/useTabManager";

const EMPTY_MD = '---\ntitle: "未命名文档"\n---\n\n';

function Harness({
  apiRef,
}: {
  apiRef: { current: ReturnType<typeof useTabManager> | null };
}) {
  const api = useTabManager();
  apiRef.current = api;
  return null;
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
    fileRead.mockResolvedValue(EMPTY_MD);
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

    await act(async () => {
      await apiRef.current!.handleNewNote();
    });

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

    await act(async () => {
      await apiRef.current!.openFile("未命名文档.md", "未命名文档");
    });
    expect(apiRef.current!.activePath).toBe("未命名文档.md");

    await act(async () => {
      await apiRef.current!.handleNewNote();
    });

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
        return "# A\n";
      }
      return EMPTY_MD;
    });

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await act(async () => {
      await apiRef.current!.openFile("a.md", "A");
      await apiRef.current!.openFile("b.md", "B");
    });

    await act(async () => {
      apiRef.current!.closeTab("b.md");
    });

    expect(apiRef.current!.activePath).toBe("a.md");
    expect(apiRef.current!.tabs.map((t: TabItem) => t.path)).toEqual([
      "a.md",
    ]);
  });
});
