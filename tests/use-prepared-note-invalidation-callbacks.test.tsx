import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { describe, expect, it, vi, afterEach } from "vitest";

import { usePreparedNoteInvalidationCallbacks } from "@/hooks/usePreparedNoteInvalidationCallbacks";

type CallbackApi = ReturnType<typeof usePreparedNoteInvalidationCallbacks>;

function Harness({
  activePath,
  onReady,
  callbacks,
}: {
  activePath: string | null;
  onReady: (api: CallbackApi) => void;
  callbacks: {
    handleFileDeleted: (path?: string) => void;
    handleFilePathChanged: (
      oldPath: string,
      newPath: string,
      title?: string,
    ) => void;
    invalidateDocumentRuntimeState: (path: string) => void;
    invalidatePreparedNote: (path: string) => void;
  };
}) {
  const activePathRef = { current: activePath };
  const api = usePreparedNoteInvalidationCallbacks({
    activePathRef,
    ...callbacks,
  });
  onReady(api);
  return null;
}

describe("usePreparedNoteInvalidationCallbacks", () => {
  let host: HTMLDivElement;
  let root: Root;

  afterEach(() => {
    root?.unmount();
    host?.remove();
  });

  function renderHarness(activePath: string | null) {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    const callbacks = {
      handleFileDeleted: vi.fn(),
      handleFilePathChanged: vi.fn(),
      invalidateDocumentRuntimeState: vi.fn(),
      invalidatePreparedNote: vi.fn(),
    };
    let api!: CallbackApi;

    act(() => {
      root.render(
        <Harness
          activePath={activePath}
          callbacks={callbacks}
          onReady={(next) => {
            api = next;
          }}
        />,
      );
    });

    return { api, callbacks };
  }

  it("invalidates prepared and runtime state for active, deleted, and renamed paths", () => {
    const { api, callbacks } = renderHarness("active.md");

    act(() => {
      api.invalidateActivePreparedNote();
      api.handlePreparedFileDeleted("deleted.md");
      api.handlePreparedFilePathChanged("old.md", "new.md", "New");
    });

    expect(callbacks.invalidatePreparedNote).toHaveBeenCalledWith("active.md");
    expect(callbacks.invalidateDocumentRuntimeState).toHaveBeenCalledWith(
      "active.md",
    );
    expect(callbacks.invalidatePreparedNote).toHaveBeenCalledWith("deleted.md");
    expect(callbacks.invalidateDocumentRuntimeState).toHaveBeenCalledWith(
      "deleted.md",
    );
    expect(callbacks.invalidatePreparedNote).toHaveBeenCalledWith("old.md");
    expect(callbacks.invalidatePreparedNote).toHaveBeenCalledWith("new.md");
    expect(callbacks.invalidateDocumentRuntimeState).toHaveBeenCalledWith(
      "old.md",
    );
    expect(callbacks.invalidateDocumentRuntimeState).toHaveBeenCalledWith(
      "new.md",
    );
    expect(callbacks.handleFileDeleted).toHaveBeenCalledWith("deleted.md");
    expect(callbacks.handleFilePathChanged).toHaveBeenCalledWith(
      "old.md",
      "new.md",
      "New",
    );
  });

  it("retires only the old-path cache after an application-owned rename", () => {
    const { api, callbacks } = renderHarness("new.md");

    act(() => {
      api.handleApplicationPathRenamed("old.md");
    });

    expect(callbacks.invalidatePreparedNote).toHaveBeenCalledTimes(1);
    expect(callbacks.invalidatePreparedNote).toHaveBeenCalledWith("old.md");
    expect(callbacks.invalidateDocumentRuntimeState).toHaveBeenCalledTimes(1);
    expect(callbacks.invalidateDocumentRuntimeState).toHaveBeenCalledWith(
      "old.md",
    );
    expect(callbacks.handleFilePathChanged).not.toHaveBeenCalled();
  });
});
