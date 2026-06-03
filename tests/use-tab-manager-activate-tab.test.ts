import { act } from "react";
import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const fileDiscard = vi.fn();
const fileRead = vi.fn();

vi.mock("@/lib/ipc", () => ({
  fileDiscard: (...args: unknown[]) => fileDiscard(...args),
  fileRead: (...args: unknown[]) => fileRead(...args),
}));

vi.mock("@/lib/note-create", () => ({
  createDefaultNote: vi.fn(),
}));

vi.mock("@/lib/document-title", () => ({
  displayTitleFromMarkdown: (_md: string, fallback: string) => fallback,
  resolveDocumentTitle: async (_path: string, hint?: string) =>
    hint?.trim() || "Title",
}));

vi.mock("@/lib/markdown", () => ({
  extractFrontmatterYaml: () => null,
  stripLeadingBodyTitleHeading: (body: string) => body,
}));

vi.mock("@/lib/editor-html-cache", () => ({
  clearCachedEditorHtml: vi.fn(),
}));

import { useTabManager } from "@/hooks/useTabManager";

function Harness({
  apiRef,
}: {
  apiRef: { current: ReturnType<typeof useTabManager> | null };
}) {
  const api = useTabManager();
  apiRef.current = api;
  return null;
}

describe("useTabManager activateTab / openNote", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    fileDiscard.mockReset();
    fileRead.mockReset();
    fileDiscard.mockResolvedValue(undefined);
    fileRead.mockImplementation(async (path: string) => {
      if (path === "a.md") {
        return '---\ntitle: "A"\n---\n\nbody-a';
      }
      if (path === "b.md") {
        return '---\ntitle: "B"\n---\n\nbody-b';
      }
      return '---\ntitle: "X"\n---\n\nbody';
    });
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("activateTab restores in-memory edits without re-reading disk", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await act(async () => {
      await apiRef.current!.openFile("a.md", "A");
    });
    const edited = '---\ntitle: "A"\n---\n\nedited-a';
    act(() => {
      apiRef.current!.setMarkdown(edited);
    });

    await act(async () => {
      await apiRef.current!.openFile("b.md", "B");
    });
    expect(fileRead).toHaveBeenCalledTimes(2);

    act(() => {
      apiRef.current!.activateTab("a.md");
    });

    expect(fileRead).toHaveBeenCalledTimes(2);
    expect(apiRef.current!.activePath).toBe("a.md");
    expect(apiRef.current!.markdown).toBe(edited);
  });

  it("openNote uses activateTab when the path is already open", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await act(async () => {
      await apiRef.current!.openFile("a.md", "A");
      await apiRef.current!.openFile("b.md", "B");
    });
    expect(fileRead).toHaveBeenCalledTimes(2);

    act(() => {
      apiRef.current!.openNote("a.md");
    });

    expect(fileRead).toHaveBeenCalledTimes(2);
    expect(apiRef.current!.activePath).toBe("a.md");
  });

  it("openNote reads disk when the tab is not open yet", async () => {
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    await act(async () => {
      root.render(createElement(Harness, { apiRef }));
    });

    await act(async () => {
      apiRef.current!.openNote("a.md", "A");
    });

    expect(fileRead).toHaveBeenCalledTimes(1);
    expect(apiRef.current!.activePath).toBe("a.md");
  });
});
