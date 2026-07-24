import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useWorkspaceTabRouting } from "@/hooks/useWorkspaceTabRouting";
import type { CloseTabResult } from "@/hooks/useTabManager";

type HookApi = ReturnType<typeof useWorkspaceTabRouting<unknown>>;

function Harness({
  activePath,
  apiRef,
  closeTab,
  setWorkspaceEmpty,
  enterWorkspaceEmpty,
  tabs,
}: {
  activePath: string | null;
  apiRef: { current: HookApi | null };
  closeTab: (path: string) => Promise<CloseTabResult> | CloseTabResult;
  setWorkspaceEmpty: (active: boolean) => void;
  enterWorkspaceEmpty: () => void;
  tabs: Array<{ path: string; title: string }>;
}) {
  apiRef.current = useWorkspaceTabRouting({
    activePath,
    closeTab,
    currentNoteIsClassified: false,
    handleActivateNoteTab: vi.fn(),
    handleNewNoteLeavingHome: vi.fn(),
    openNoteLeavingHome: vi.fn(),
    setWorkspaceEmpty,
    enterWorkspaceEmpty,
    tabs,
  });
  return null;
}

describe("useWorkspaceTabRouting", () => {
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

  it("enters workspace empty after closing the last active note tab", async () => {
    const apiRef: { current: HookApi | null } = { current: null };
    const closeTab = vi.fn(async () => ({
      closed: true,
      discardedPristine: false,
      nextActivePath: null,
      remainingNoteCount: 0,
    }));
    const setWorkspaceEmpty = vi.fn();
    const enterWorkspaceEmpty = vi.fn();

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "only.md",
          apiRef,
          closeTab,
          setWorkspaceEmpty,
          enterWorkspaceEmpty,
          tabs: [{ path: "only.md", title: "Only" }],
        }),
      );
    });

    await act(async () => {
      apiRef.current!.handleCloseWorkspaceTab("only.md");
    });

    expect(closeTab).toHaveBeenCalledWith("only.md");
    expect(enterWorkspaceEmpty).toHaveBeenCalledTimes(1);
    expect(setWorkspaceEmpty).not.toHaveBeenCalled();
  });

  it("does not enter workspace empty when closing the last tab is blocked", async () => {
    const apiRef: { current: HookApi | null } = { current: null };
    const closeTab = vi.fn(async () => ({
      closed: false,
      discardedPristine: false,
      nextActivePath: "only.md",
      remainingNoteCount: 1,
    }));
    const enterWorkspaceEmpty = vi.fn();

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "only.md",
          apiRef,
          closeTab,
          setWorkspaceEmpty: vi.fn(),
          enterWorkspaceEmpty,
          tabs: [{ path: "only.md", title: "Only" }],
        }),
      );
    });

    await act(async () => {
      apiRef.current!.handleCloseWorkspaceTab("only.md");
      await Promise.resolve();
    });

    expect(closeTab).toHaveBeenCalledWith("only.md");
    expect(enterWorkspaceEmpty).not.toHaveBeenCalled();
  });
});
