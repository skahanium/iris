import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useFileConflictResolution } from "@/hooks/useFileConflictResolution";

type HookApi = ReturnType<typeof useFileConflictResolution>;

function Harness({
  flushWhenEditorReady,
  onReady,
}: {
  flushWhenEditorReady: (actionLabel: string) => Promise<unknown>;
  onReady: (api: HookApi) => void;
}) {
  const api = useFileConflictResolution({
    activePathRef: { current: "note.md" },
    applyMarkdownToEditor: vi.fn(),
    conflictState: null,
    dirtyRef: { current: true },
    flushWhenEditorReady,
    invalidatePreparedNote: vi.fn(),
    isMutationBlocked: () => false,
    markClean: vi.fn(),
    openNoteLeavingHome: vi.fn(),
    setConflictState: vi.fn(),
    syncTabMarkdownCache: vi.fn(),
  });
  onReady(api);
  return null;
}

describe("useFileConflictResolution", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("uses the loading-safe flush gate before keeping local conflict content", async () => {
    const flushWhenEditorReady = vi.fn(async () => ({
      markdown: null,
      ok: false,
    }));
    let api!: HookApi;

    await act(async () => {
      root.render(
        createElement(Harness, {
          flushWhenEditorReady,
          onReady: (next) => {
            api = next;
          },
        }),
      );
    });

    await act(async () => {
      api.handleConflictKeepLocal();
      await Promise.resolve();
    });

    expect(flushWhenEditorReady).toHaveBeenCalledWith("保留本地修改");
  });
});
