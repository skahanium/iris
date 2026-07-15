import { act, createElement, useRef, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { EDITOR_SAVE_DEBOUNCE_MS, useEditorSave } from "@/hooks/useEditorSave";

const fileWrite = vi.fn().mockResolvedValue({
  id: 0,
  path: "note.md",
  title: "note",
  updated_at: "",
  word_count: 1,
});
const versionSaveManual = vi.fn();
const versionSaveIdle = vi.fn();

vi.mock("@/lib/ipc", () => ({
  fileWrite: (...args: unknown[]) => fileWrite(...args),
  versionSaveManual: (...args: unknown[]) => versionSaveManual(...args),
  versionSaveIdle: (...args: unknown[]) => versionSaveIdle(...args),
}));

function TestHarness({
  path = "note.md",
  getMarkdown,
  onSaved,
  onReady,
}: {
  path?: string;
  getMarkdown?: () => string;
  onSaved?: (markdown: string) => void;
  onReady: (api: {
    notifyDirty: () => void;
    flushSave: () => Promise<string | null>;
    cancelPendingSave: () => void;
    awaitSaveInFlight: () => Promise<void>;
    getLastSavedSnapshot: () => {
      path: string;
      markdown: string;
      savedAt: number;
      dirtyGeneration: number;
    } | null;
    flushSaveForPath: (
      targetPath: string,
      getMarkdownOverride?: () => string,
    ) => Promise<string | null>;
    recordSavedSnapshot: (targetPath: string, markdown: string) => void;
  }) => void;
}) {
  const {
    notifyDirty,
    flushSave,
    cancelPendingSave,
    awaitSaveInFlight,
    getLastSavedSnapshot,
    flushSaveForPath,
    recordSavedSnapshot,
  } = useEditorSave(
    path,
    getMarkdown ?? (() => '---\ntitle: "x"\n---\n\nSubstantive body.'),
    onSaved,
  );
  onReady({
    notifyDirty,
    flushSave,
    cancelPendingSave,
    awaitSaveInFlight,
    getLastSavedSnapshot,
    flushSaveForPath,
    recordSavedSnapshot,
  });
  return null;
}

function PathSwitchHarness({
  onReady,
}: {
  onReady: (api: {
    setPath: (path: string) => void;
    flushSaveForPath: (
      targetPath: string,
      getMarkdownOverride?: () => string,
    ) => Promise<string | null>;
  }) => void;
}) {
  const [path, setPath] = useState("a.md");
  const bodyA = '---\ntitle: "a"\n---\n\nContent for note A.';
  const bodyB = '---\ntitle: "b"\n---\n\nContent for note B.';
  const whichRef = useRef<"a" | "b">("a");
  const { flushSaveForPath } = useEditorSave(path, () =>
    whichRef.current === "a" ? bodyA : bodyB,
  );
  onReady({ setPath, flushSaveForPath });
  return null;
}

describe("useEditorSave", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    fileWrite.mockReset();
    fileWrite.mockResolvedValue({
      id: 0,
      path: "note.md",
      title: "note",
      updated_at: "",
      word_count: 1,
    });
    versionSaveManual.mockClear();
    versionSaveIdle.mockClear();
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
    vi.useRealTimers();
  });

  it("uses 1200ms debounce for layer-1 persistence", () => {
    expect(EDITOR_SAVE_DEBOUNCE_MS).toBe(1200);
  });

  it("debounced notifyDirty calls fileWrite only, not version IPC", async () => {
    let notifyDirty!: () => void;

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          onReady: (api) => {
            notifyDirty = api.notifyDirty;
          },
        }),
      );
    });

    act(() => {
      notifyDirty();
    });

    await act(async () => {
      vi.advanceTimersByTime(EDITOR_SAVE_DEBOUNCE_MS);
    });

    expect(fileWrite).toHaveBeenCalledTimes(1);
    expect(fileWrite).toHaveBeenCalledWith(
      "note.md",
      '---\ntitle: "x"\n---\n\nSubstantive body.',
    );
    expect(versionSaveManual).not.toHaveBeenCalled();
    expect(versionSaveIdle).not.toHaveBeenCalled();
  });

  it("flushSave returns the markdown it wrote", async () => {
    let flushSave!: () => Promise<string | null>;
    let getLastSavedSnapshot!: () => {
      path: string;
      markdown: string;
      savedAt: number;
      dirtyGeneration: number;
    } | null;
    const getMarkdown = vi.fn(
      () => '---\ntitle: "x"\n---\n\nManual checkpoint body.',
    );

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          getMarkdown,
          onReady: (api) => {
            flushSave = api.flushSave;
            getLastSavedSnapshot = api.getLastSavedSnapshot;
          },
        }),
      );
    });

    let saved: string | null = null;
    await act(async () => {
      saved = await flushSave();
    });

    expect(saved).toBe('---\ntitle: "x"\n---\n\nManual checkpoint body.');
    expect(getMarkdown).toHaveBeenCalledTimes(1);
    expect(fileWrite).toHaveBeenCalledWith(
      "note.md",
      '---\ntitle: "x"\n---\n\nManual checkpoint body.',
    );
    expect(getLastSavedSnapshot()).toMatchObject({
      path: "note.md",
      markdown: '---\ntitle: "x"\n---\n\nManual checkpoint body.',
      dirtyGeneration: 0,
    });
    expect(getLastSavedSnapshot()?.savedAt).toBeGreaterThan(0);
  });

  it("records the dirty generation that produced the saved snapshot", async () => {
    let notifyDirty!: () => void;
    let flushSave!: () => Promise<string | null>;
    let getLastSavedSnapshot!: () => {
      path: string;
      markdown: string;
      savedAt: number;
      dirtyGeneration: number;
    } | null;

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          onReady: (api) => {
            notifyDirty = api.notifyDirty;
            flushSave = api.flushSave;
            getLastSavedSnapshot = api.getLastSavedSnapshot;
          },
        }),
      );
    });

    act(() => {
      notifyDirty();
      notifyDirty();
    });

    await act(async () => {
      await flushSave();
    });

    expect(getLastSavedSnapshot()).toMatchObject({
      path: "note.md",
      markdown: '---\ntitle: "x"\n---\n\nSubstantive body.',
      dirtyGeneration: 2,
    });
  });

  it("skips the first flush when opened content is already recorded as saved", async () => {
    const opened = '---\ntitle: "x"\n---\n\nOpened body.';
    const getMarkdown = vi.fn(() => opened);
    const onSaved = vi.fn();
    let flushSave!: () => Promise<string | null>;
    let recordSavedSnapshot!: (targetPath: string, markdown: string) => void;

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          getMarkdown,
          onSaved,
          onReady: (api) => {
            flushSave = api.flushSave;
            recordSavedSnapshot = api.recordSavedSnapshot;
          },
        }),
      );
    });

    act(() => {
      recordSavedSnapshot("note.md", opened);
    });

    let saved: string | null = null;
    await act(async () => {
      saved = await flushSave();
    });

    expect(saved).toBe(opened);
    expect(getMarkdown).toHaveBeenCalledTimes(1);
    expect(fileWrite).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("writes after a recorded baseline when dirty content changes", async () => {
    const opened = '---\ntitle: "x"\n---\n\nOpened body.';
    const edited = '---\ntitle: "x"\n---\n\nEdited body.';
    let current = opened;
    const getMarkdown = vi.fn(() => current);
    const onSaved = vi.fn();
    let notifyDirty!: () => void;
    let flushSave!: () => Promise<string | null>;
    let recordSavedSnapshot!: (targetPath: string, markdown: string) => void;

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          getMarkdown,
          onSaved,
          onReady: (api) => {
            notifyDirty = api.notifyDirty;
            flushSave = api.flushSave;
            recordSavedSnapshot = api.recordSavedSnapshot;
          },
        }),
      );
    });

    act(() => {
      recordSavedSnapshot("note.md", opened);
      current = edited;
      notifyDirty();
    });

    let saved: string | null = null;
    await act(async () => {
      saved = await flushSave();
    });

    expect(saved).toBe(edited);
    expect(fileWrite).toHaveBeenCalledTimes(1);
    expect(fileWrite).toHaveBeenCalledWith("note.md", edited);
    expect(onSaved).toHaveBeenCalledWith(edited, true);
  });

  it("flushSaveForPath writes the leaving path while active path is already B", async () => {
    const bodyA = '---\ntitle: "a"\n---\n\nContent for note A.';
    let flushSaveForPath!: (
      targetPath: string,
      getMarkdownOverride?: () => string,
    ) => Promise<string | null>;
    let setPath!: (path: string) => void;

    await act(async () => {
      root.render(
        createElement(PathSwitchHarness, {
          onReady: (api) => {
            flushSaveForPath = api.flushSaveForPath;
            setPath = api.setPath;
          },
        }),
      );
    });

    await act(async () => {
      setPath("b.md");
    });

    let saved: string | null = null;
    await act(async () => {
      saved = await flushSaveForPath("a.md", () => bodyA);
    });

    expect(saved).toBe(bodyA);
    expect(fileWrite).toHaveBeenCalledTimes(1);
    expect(fileWrite).toHaveBeenCalledWith("a.md", bodyA);
    expect(fileWrite).not.toHaveBeenCalledWith("b.md", bodyA);
  });

  it("path change cleanup does not flush to the new path", async () => {
    const bodyA = '---\ntitle: "a"\n---\n\nPending A edits.';
    const getMarkdown = vi.fn(() => bodyA);
    let notifyDirty!: () => void;
    let setPath!: (path: string) => void;

    await act(async () => {
      root.render(
        createElement(function SwitchNotifyHarness() {
          const [path, setPathState] = useState("a.md");
          const { notifyDirty: notify } = useEditorSave(path, getMarkdown);
          setPath = setPathState;
          notifyDirty = notify;
          return null;
        }),
      );
    });

    act(() => {
      notifyDirty();
    });

    await act(async () => {
      setPath("b.md");
    });

    await act(async () => {
      vi.advanceTimersByTime(EDITOR_SAVE_DEBOUNCE_MS);
    });

    expect(fileWrite).not.toHaveBeenCalled();
  });

  it("coalesces overlapping flushSave calls into the in-flight save loop", async () => {
    const writes: Array<() => void> = [];
    fileWrite.mockImplementation(
      async () =>
        new Promise((resolve) => {
          writes.push(() =>
            resolve({
              id: 0,
              path: "note.md",
              title: "note",
              updated_at: "",
              word_count: 1,
            }),
          );
        }),
    );
    const getMarkdown = vi
      .fn()
      .mockReturnValueOnce('---\ntitle: "x"\n---\n\nFirst body.')
      .mockReturnValueOnce('---\ntitle: "x"\n---\n\nLatest body.')
      .mockReturnValue('---\ntitle: "x"\n---\n\nUnexpected third body.');
    let flushSave!: () => Promise<string | null>;

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          getMarkdown,
          onReady: (api) => {
            flushSave = api.flushSave;
          },
        }),
      );
    });

    const first = flushSave();
    const second = flushSave();
    expect(fileWrite).toHaveBeenCalledTimes(1);

    await act(async () => {
      writes.shift()?.();
      await Promise.resolve();
    });
    expect(fileWrite).toHaveBeenCalledTimes(2);

    await act(async () => {
      writes.shift()?.();
      await Promise.resolve();
    });
    if (writes.length > 0) {
      await act(async () => {
        writes.shift()?.();
        await Promise.resolve();
      });
    }
    await act(async () => {
      await first;
      await second;
    });

    expect(fileWrite).toHaveBeenCalledTimes(2);
    expect(fileWrite).toHaveBeenLastCalledWith(
      "note.md",
      '---\ntitle: "x"\n---\n\nLatest body.',
    );
  });

  it("flushSaveForPath waits for an active in-flight save before writing a leaving path", async () => {
    const writes: Array<() => void> = [];
    fileWrite.mockImplementation(
      async () =>
        new Promise((resolve) => {
          writes.push(() =>
            resolve({
              id: 0,
              path: "note.md",
              title: "note",
              updated_at: "",
              word_count: 1,
            }),
          );
        }),
    );
    let flushSave!: () => Promise<string | null>;
    let flushSaveForPath!: (
      targetPath: string,
      getMarkdownOverride?: () => string,
    ) => Promise<string | null>;

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          getMarkdown: () => '---\ntitle: "x"\n---\n\nActive body.',
          onReady: (api) => {
            flushSave = api.flushSave;
            flushSaveForPath = api.flushSaveForPath;
          },
        }),
      );
    });

    const active = flushSave();
    const leaving = flushSaveForPath(
      "leaving.md",
      () => '---\ntitle: "leaving"\n---\n\nLeaving body.',
    );
    expect(fileWrite).toHaveBeenCalledTimes(1);

    await act(async () => {
      writes.shift()?.();
      await Promise.resolve();
    });
    expect(fileWrite).toHaveBeenCalledTimes(2);
    expect(fileWrite).toHaveBeenLastCalledWith(
      "leaving.md",
      '---\ntitle: "leaving"\n---\n\nLeaving body.',
    );

    await act(async () => {
      writes.shift()?.();
      await active;
      await leaving;
    });

    expect(fileWrite).toHaveBeenCalledTimes(2);
  });

  it("cancels a debounced save before it writes a deleted path", async () => {
    let notifyDirty!: () => void;
    let cancelPendingSave!: () => void;

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          onReady: (api) => {
            notifyDirty = api.notifyDirty;
            cancelPendingSave = api.cancelPendingSave;
          },
        }),
      );
    });

    act(() => {
      notifyDirty();
      cancelPendingSave();
    });

    await act(async () => {
      vi.advanceTimersByTime(EDITOR_SAVE_DEBOUNCE_MS);
    });

    expect(fileWrite).not.toHaveBeenCalled();
  });

  it("awaits an in-flight save before path deletion proceeds", async () => {
    const writes: Array<() => void> = [];
    fileWrite.mockImplementation(
      async () =>
        new Promise((resolve) => {
          writes.push(() =>
            resolve({
              id: 0,
              path: "note.md",
              title: "note",
              updated_at: "",
              word_count: 1,
            }),
          );
        }),
    );
    let flushSave!: () => Promise<string | null>;
    let awaitSaveInFlight!: () => Promise<void>;

    await act(async () => {
      root.render(
        createElement(TestHarness, {
          onReady: (api) => {
            flushSave = api.flushSave;
            awaitSaveInFlight = api.awaitSaveInFlight;
          },
        }),
      );
    });

    const active = flushSave();
    let settled = false;
    const waiting = awaitSaveInFlight().then(() => {
      settled = true;
    });

    await act(async () => {
      await Promise.resolve();
    });
    expect(settled).toBe(false);

    await act(async () => {
      writes.shift()?.();
      await active;
      await waiting;
    });

    expect(settled).toBe(true);
  });
});
