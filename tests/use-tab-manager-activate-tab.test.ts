import { act } from "react";
import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const fileDiscard = vi.fn();
const fileRead = vi.fn();
const resolveDocumentTitle = vi.fn();

vi.mock("@/lib/ipc", () => ({
  fileDiscard: (...args: unknown[]) => fileDiscard(...args),
  fileRead: (...args: unknown[]) => fileRead(...args),
}));

vi.mock("@/lib/note-create", () => ({
  createDefaultNote: vi.fn(),
}));

vi.mock("@/lib/document-title", () => ({
  displayTitleFromMarkdown: (_md: string, fallback: string) => fallback,
  resolveDocumentTitle: (...args: unknown[]) => resolveDocumentTitle(...args),
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
    resolveDocumentTitle.mockReset();
    resolveDocumentTitle.mockImplementation(
      async (_path: string, hint?: string) => hint?.trim() || "Title",
    );
    fileDiscard.mockResolvedValue(undefined);
    fileRead.mockImplementation(async (path: string) => {
      if (path === "a.md") {
        return { content: '---\ntitle: "A"\n---\n\nbody-a', isLocked: false };
      }
      if (path === "b.md") {
        return { content: '---\ntitle: "B"\n---\n\nbody-b', isLocked: false };
      }
      return { content: '---\ntitle: "X"\n---\n\nbody', isLocked: false };
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

    await act(async () => {
      await apiRef.current!.activateTab("a.md");
    });

    expect(fileRead).toHaveBeenCalledTimes(2);
    expect(apiRef.current!.activePath).toBe("a.md");
    expect(apiRef.current!.markdown).toBe(edited);
  });

  it("starts reading the target note without waiting for old-note persistence", async () => {
    let resolvePersist: ((value: string | null) => void) | null = null;
    const persistBeforeLeave = vi.fn(
      () =>
        new Promise<string | null>((resolve) => {
          resolvePersist = resolve;
        }),
    );
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    function PersistHarness() {
      const api = useTabManager({ persistBeforeLeave });
      apiRef.current = api;
      return null;
    }

    await act(async () => {
      root.render(createElement(PersistHarness));
    });

    await act(async () => {
      await apiRef.current!.openFile("a.md", "A");
    });
    fileRead.mockClear();

    let openPromise: Promise<void>;
    await act(async () => {
      openPromise = apiRef.current!.openFile("b.md", "B");
      await Promise.resolve();
    });

    expect(persistBeforeLeave).toHaveBeenCalledWith("a.md");
    expect(fileRead).toHaveBeenCalledWith("b.md", {
      allowClassified: false,
    });

    (resolvePersist as unknown as (value: string | null) => void)("saved-a");
    await act(async () => {
      await openPromise!;
    });

    expect(apiRef.current!.activePath).toBe("b.md");
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

    await act(async () => {
      await apiRef.current!.openNote("a.md");
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
    expect(resolveDocumentTitle).not.toHaveBeenCalled();
  });

  it("discards an externally deleted open tab without persisting the deleted path", async () => {
    const persistBeforeLeave = vi.fn().mockResolvedValue("saved");
    const apiRef: { current: ReturnType<typeof useTabManager> | null } = {
      current: null,
    };

    function PersistHarness() {
      const api = useTabManager({ persistBeforeLeave });
      apiRef.current = api;
      return null;
    }

    await act(async () => {
      root.render(createElement(PersistHarness));
    });

    await act(async () => {
      await apiRef.current!.openFile("a.md", "A");
      await apiRef.current!.openFile("b.md", "B");
      await apiRef.current!.activateTab("a.md");
    });
    persistBeforeLeave.mockClear();

    await act(async () => {
      await apiRef.current!.discardOpenTab("a.md");
    });

    expect(persistBeforeLeave).not.toHaveBeenCalledWith("a.md");
    expect(apiRef.current!.tabs.map((tab) => tab.path)).toEqual(["b.md"]);
    expect(apiRef.current!.activePath).toBe("b.md");
  });
});
