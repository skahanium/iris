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
    hint?.trim() || "无标题1",
}));

vi.mock("@/lib/markdown", () => ({
  extractFrontmatterYaml: () => null,
  stripLeadingBodyTitleHeading: (body: string) => body,
}));

import type { TabItem } from "@/components/layout/TabBar";
import { useTabManager } from "@/hooks/useTabManager";

const EMPTY_MD = '---\ntitle: "无标题1"\n---\n\n';

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
      path: "无标题1.md",
      title: "无标题1",
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
    expect(apiRef.current!.activePath).toBe("无标题1.md");
  });

  it("keeps an empty tab open and adds the next note on +", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    createDefaultNote.mockResolvedValueOnce({
      path: "无标题2.md",
      title: "无标题2",
    });

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await act(async () => {
      await apiRef.current!.openFile("无标题1.md", "无标题1");
    });
    expect(apiRef.current!.activePath).toBe("无标题1.md");

    await act(async () => {
      await apiRef.current!.handleNewNote();
    });

    expect(fileDiscard).not.toHaveBeenCalled();
    expect(createDefaultNote).toHaveBeenCalledWith({
      extraTakenTitles: ["无标题1"],
    });
    expect(apiRef.current!.activePath).toBe("无标题2.md");
    expect(
      apiRef.current!.tabs.some((t: TabItem) => t.path === "无标题1.md"),
    ).toBe(true);
    expect(
      apiRef.current!.tabs.some((t: TabItem) => t.path === "无标题2.md"),
    ).toBe(true);
  });

  it("closes a tab and switches to the neighbor when closing the active tab", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    fileRead.mockImplementation(async (path: string) => {
      if (path === "a.md") {
        return '---\ntitle: "A"\n---\n\nbody';
      }
      return '---\ntitle: "B"\n---\n\nbody';
    });

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await act(async () => {
      await apiRef.current!.openFile("a.md", "A");
      await apiRef.current!.openFile("b.md", "B");
    });
    expect(apiRef.current!.activePath).toBe("b.md");

    await act(async () => {
      await apiRef.current!.closeTab("b.md");
    });

    expect(apiRef.current!.tabs.map((t) => t.path)).toEqual(["a.md"]);
    expect(apiRef.current!.activePath).toBe("a.md");
    expect(fileDiscard).not.toHaveBeenCalled();
  });
});
