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
type ActivateTabFn = (path: string, options?: unknown) => Promise<void> | void;
type HandleNewNoteFn = (options?: unknown) => Promise<void>;

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
  activateTab = vi.fn(),
  apiRef,
  handleNewNote = vi.fn(async () => undefined),
  openTabs = [],
  openNote,
  setHomeActive,
}: {
  activePath?: string | null;
  activateTab?: ActivateTabFn;
  apiRef: { current: ReturnType<typeof useHomeWorkspaceTransitions> | null };
  handleNewNote?: HandleNewNoteFn;
  openTabs?: Array<{ path: string }>;
  openNote: OpenNoteFn;
  setHomeActive: SetHomeActiveFn;
}) {
  apiRef.current = useHomeWorkspaceTransitions({
    activePathRef: { current: activePath },
    activateTab,
    handleNewNote,
    openNote,
    openTabs,
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
      startedAt: expect.any(Number),
      title: "New",
    });

    await act(async () => {
      stagedOpen.resolve();
      await openPromise;
    });

    expect(apiRef.current!.pendingOpen).toMatchObject({
      path: "new.md",
      sequence: 1,
      startedAt: expect.any(Number),
    });
  });

  it("recovers to Home with an error when a note open never reaches staging", async () => {
    const apiRef: {
      current: ReturnType<typeof useHomeWorkspaceTransitions> | null;
    } = {
      current: null,
    };
    const stalledOpen = deferred<void>();
    const openNote = vi.fn(() => stalledOpen.promise);
    const setHomeActive = vi.fn();

    await act(async () => {
      root.render(createElement(Harness, { apiRef, openNote, setHomeActive }));
    });

    await act(async () => {
      void apiRef.current!.openNoteLeavingHome("stalled.md", "Stalled");
    });

    await act(async () => {
      vi.advanceTimersByTime(15_000);
    });

    expect(setHomeActive).toHaveBeenCalledWith(true);
    expect(apiRef.current!.pendingOpen).toMatchObject({
      error: expect.stringContaining("文档打开超时"),
      path: "stalled.md",
    });

    await act(async () => {
      stalledOpen.resolve();
      await stalledOpen.promise;
    });

    expect(setHomeActive).not.toHaveBeenLastCalledWith(false);
  });

  it("starts welcome new-note opens with disabled loading and passes the home sequence forward", async () => {
    const apiRef: {
      current: ReturnType<typeof useHomeWorkspaceTransitions> | null;
    } = {
      current: null,
    };
    const handleNewNote = vi.fn(async () => undefined);
    const openNote = vi.fn(async () => undefined);
    const setHomeActive = vi.fn();

    await act(async () => {
      root.render(
        createElement(Harness, {
          apiRef,
          handleNewNote,
          openNote,
          setHomeActive,
        }),
      );
    });

    await act(async () => {
      await apiRef.current!.handleNewNoteLeavingHome();
    });

    expect(apiRef.current!.pendingOpen).toMatchObject({
      kind: "new-note",
      loadingPolicy: "disabled",
      path: null,
      sequence: 1,
    });
    expect(handleNewNote).toHaveBeenCalledWith({ homeOpenSequence: 1 });
  });

  it("directly activates an already-open note from Home via activateTab without an openNote detour", async () => {
    const apiRef: {
      current: ReturnType<typeof useHomeWorkspaceTransitions> | null;
    } = {
      current: null,
    };
    const activateTab = vi.fn(async () => undefined);
    const openNote = vi.fn(async () => undefined);
    const setHomeActive = vi.fn();

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "current.md",
          activateTab,
          apiRef,
          openNote,
          openTabs: [{ path: "current.md" }],
          setHomeActive,
        }),
      );
    });

    await act(async () => {
      await apiRef.current!.openNoteLeavingHome("current.md", "Current", {
        source: "welcome",
      });
    });

    expect(activateTab).toHaveBeenCalledWith(
      "current.md",
      expect.objectContaining({ openBudgetKind: "hot", source: "welcome" }),
    );
    expect(openNote).not.toHaveBeenCalled();
    expect(setHomeActive).toHaveBeenCalledWith(false);
    expect(apiRef.current!.pendingOpen).toBeNull();
  });

  it("does not flip homeActive until the target tab commits, avoiding a flash of the previous document", async () => {
    const apiRef: {
      current: ReturnType<typeof useHomeWorkspaceTransitions> | null;
    } = {
      current: null,
    };
    const stagedActivate = deferred<void>();
    const activateTab = vi.fn(() => stagedActivate.promise);
    const openNote = vi.fn(async () => undefined);
    const setHomeActive = vi.fn();

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "current.md",
          activateTab,
          apiRef,
          openNote,
          openTabs: [{ path: "current.md" }],
          setHomeActive,
        }),
      );
    });

    let openPromise!: Promise<void>;
    await act(async () => {
      openPromise = apiRef.current!.openNoteLeavingHome(
        "current.md",
        "Current",
      );
    });

    // Before the target tab commits: must NOT route through openNote (its async
    // IPC gap is what reveals the still-active previous document), and must NOT
    // flip homeActive yet (that would surface the previous document's retained
    // editor surface at full opacity).
    expect(activateTab).toHaveBeenCalledWith(
      "current.md",
      expect.objectContaining({ openBudgetKind: "hot" }),
    );
    expect(openNote).not.toHaveBeenCalled();
    expect(setHomeActive).not.toHaveBeenCalledWith(false);

    await act(async () => {
      stagedActivate.resolve();
      await openPromise;
    });

    // Once activateTab has committed the target tab, homeActive flips so the
    // target document is shown directly — with no intermediate render of the
    // previous document.
    expect(setHomeActive).toHaveBeenCalledWith(false);
    expect(apiRef.current!.pendingOpen).toBeNull();
  });

  it("leaves Home active when showHome interrupts an in-flight already-open tab activation", async () => {
    const apiRef: {
      current: ReturnType<typeof useHomeWorkspaceTransitions> | null;
    } = {
      current: null,
    };
    const stagedActivate = deferred<void>();
    const activateTab = vi.fn(() => stagedActivate.promise);
    const openNote = vi.fn(async () => undefined);
    const setHomeActive = vi.fn();

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "current.md",
          activateTab,
          apiRef,
          openNote,
          openTabs: [{ path: "current.md" }],
          setHomeActive,
        }),
      );
    });

    let openPromise!: Promise<void>;
    await act(async () => {
      openPromise = apiRef.current!.openNoteLeavingHome(
        "current.md",
        "Current",
      );
    });

    // User clicks the logo to stay on Home while activation is in flight.
    await act(async () => {
      apiRef.current!.showHome();
    });

    await act(async () => {
      stagedActivate.resolve();
      await openPromise;
    });

    // showHome won: the late activateTab resolution must NOT override it by
    // flipping homeActive off, or the user would be dragged out of Home.
    expect(setHomeActive).toHaveBeenCalledWith(true);
    expect(setHomeActive).not.toHaveBeenCalledWith(false);
  });
});
