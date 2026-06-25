import { act } from "react";
import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useHomeWorkspaceTransitions } from "@/hooks/useHomeWorkspaceTransitions";

type OpenNoteFn = (
  path: string,
  titleHint?: string,
  options?: unknown,
) => Promise<void>;
type SetHomeActiveFn = (active: boolean) => void;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function Harness({
  activePath = null,
  apiRef,
  openTabs = [],
  openNote,
  setHomeActive,
}: {
  activePath?: string | null;
  apiRef: { current: ReturnType<typeof useHomeWorkspaceTransitions> | null };
  openTabs?: Array<{ path: string }>;
  openNote: OpenNoteFn;
  setHomeActive: SetHomeActiveFn;
}) {
  apiRef.current = useHomeWorkspaceTransitions({
    activePathRef: { current: activePath },
    activateArtifact: vi.fn(),
    activateTab: vi.fn(),
    handleNewNote: vi.fn(async () => undefined),
    openNote,
    openTabs,
    setActiveArtifactId: vi.fn(),
    setHomeActive,
  });
  return null;
}

describe("useHomeWorkspaceTransitions", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(1000);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    vi.useRealTimers();
    act(() => root.unmount());
    host.remove();
  });

  it("leaves Home immediately and keeps pending open until the editor surface settles", async () => {
    const apiRef: {
      current: ReturnType<typeof useHomeWorkspaceTransitions> | null;
    } = {
      current: null,
    };
    const stagedOpen = deferred<void>();
    const openNote = vi.fn(() => stagedOpen.promise);
    const setHomeActive = vi.fn();

    await act(async () => {
      root.render(createElement(Harness, { apiRef, openNote, setHomeActive }));
    });

    let openPromise!: Promise<void>;
    await act(async () => {
      openPromise = apiRef.current!.openNoteLeavingHome("new.md", "New");
    });

    expect(setHomeActive).toHaveBeenCalledWith(false);
    expect(apiRef.current!.pendingOpen).toMatchObject({
      kind: "note",
      path: "new.md",
      sequence: 1,
      startedAt: 1000,
      title: "New",
    });

    await act(async () => {
      stagedOpen.resolve();
      await openPromise;
    });

    expect(apiRef.current!.pendingOpen).toMatchObject({
      path: "new.md",
      sequence: 1,
      startedAt: 1000,
    });
  });

  it("directly activates an already-open note from Home without pending loading", async () => {
    const apiRef: {
      current: ReturnType<typeof useHomeWorkspaceTransitions> | null;
    } = {
      current: null,
    };
    const openNote = vi.fn(async () => undefined);
    const setHomeActive = vi.fn();

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "current.md",
          apiRef,
          openNote,
          openTabs: [{ path: "current.md" }],
          setHomeActive,
        }),
      );
    });

    await act(async () => {
      await apiRef.current!.openNoteLeavingHome("current.md", "Current");
    });

    expect(openNote).toHaveBeenCalledWith("current.md", "Current", undefined);
    expect(setHomeActive).toHaveBeenCalledWith(false);
    expect(apiRef.current!.pendingOpen).toBeNull();
  });
});
