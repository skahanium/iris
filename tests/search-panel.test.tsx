import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { SearchPanel } from "@/components/file/SearchPanel";

const searchKeyword = vi.fn();
const searchSemantic = vi.fn();

vi.mock("@/lib/ipc", () => ({
  searchKeyword: (...args: unknown[]) => searchKeyword(...args),
  searchSemantic: (...args: unknown[]) => searchSemantic(...args),
}));

describe("SearchPanel", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    searchKeyword.mockReset();
    searchSemantic.mockReset();
    searchKeyword.mockResolvedValue([
      {
        path: "notes/a.md",
        title: "Note A",
        snippet: "hello <em>world</em>",
      },
    ]);
    searchSemantic.mockResolvedValue([]);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  function renderPanel(
    props: Partial<{
      onOpen: (path: string) => void;
      onClose: () => void;
    }> = {},
  ) {
    const onOpen = props.onOpen ?? vi.fn();
    const onClose = props.onClose ?? vi.fn();
    act(() => {
      root.render(<SearchPanel open onClose={onClose} onOpen={onOpen} />);
    });
    return { onOpen, onClose };
  }

  function setQuery(value: string) {
    const input = document.querySelector<HTMLInputElement>(
      'input[placeholder="输入关键词或自然语言…"]',
    );
    if (!input) throw new Error("search input missing");
    act(() => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        "value",
      )?.set;
      setter?.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  }

  it("runs keyword vault search and opens a hit", async () => {
    const { onOpen, onClose } = renderPanel();

    expect(document.querySelector('[aria-label="全库搜索"]')).not.toBeNull();

    setQuery("hello");
    const searchBtn = Array.from(document.querySelectorAll("button")).find(
      (b) =>
        b.textContent?.includes("搜索") || b.textContent?.includes("鎼滅储"),
    );
    expect(searchBtn).toBeTruthy();
    await act(async () => {
      searchBtn?.click();
    });

    await vi.waitFor(() => {
      expect(searchKeyword).toHaveBeenCalledWith("hello", 20);
    });

    const hit = Array.from(document.querySelectorAll("button")).find((b) =>
      b.textContent?.includes("Note A"),
    );
    expect(hit).toBeTruthy();
    await act(async () => {
      hit?.click();
    });

    expect(onOpen).toHaveBeenCalledWith("notes/a.md");
    expect(onClose).toHaveBeenCalled();
    expect(searchSemantic).not.toHaveBeenCalled();
  });

  it("keeps the overlay open until async note opening resolves", async () => {
    let resolveOpen!: () => void;
    const onOpen = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveOpen = resolve;
        }),
    );
    const onClose = vi.fn();
    renderPanel({ onOpen, onClose });

    setQuery("hello");
    const searchBtn = Array.from(document.querySelectorAll("button")).find(
      (b) =>
        b.textContent?.includes("搜索") || b.textContent?.includes("鎼滅储"),
    );
    expect(searchBtn).toBeTruthy();
    await act(async () => {
      searchBtn?.click();
    });
    let hit: HTMLButtonElement | undefined;
    await vi.waitFor(() => {
      hit = Array.from(document.querySelectorAll("button")).find((b) =>
        b.textContent?.includes("Note A"),
      );
      expect(hit).toBeTruthy();
    });
    await act(async () => {
      hit?.click();
      await Promise.resolve();
    });
    expect(onOpen).toHaveBeenCalledWith("notes/a.md");
    expect(onClose).not.toHaveBeenCalled();

    await act(async () => {
      resolveOpen();
      await Promise.resolve();
    });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("runs semantic search when semantic mode is selected", async () => {
    renderPanel();
    setQuery("ideas");

    const semanticBtn = Array.from(document.querySelectorAll("button")).find(
      (b) => b.textContent === "语义",
    );
    await act(async () => {
      semanticBtn?.click();
    });

    const searchBtn = Array.from(document.querySelectorAll("button")).find(
      (b) => b.textContent === "搜索",
    );
    await act(async () => {
      searchBtn?.click();
    });

    await vi.waitFor(() => {
      expect(searchSemantic).toHaveBeenCalledWith("ideas", 5);
    });
    expect(searchKeyword).not.toHaveBeenCalled();
  });
});
