import { act, createElement } from "react";
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
  getMarkdown,
  onReady,
}: {
  getMarkdown?: () => string;
  onReady: (api: {
    notifyDirty: () => void;
    flushSave: () => Promise<string | null>;
  }) => void;
}) {
  const { notifyDirty, flushSave } = useEditorSave(
    "note.md",
    getMarkdown ?? (() => '---\ntitle: "x"\n---\n\nSubstantive body.'),
  );
  onReady({ notifyDirty, flushSave });
  return null;
}

describe("useEditorSave", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.useFakeTimers();
    fileWrite.mockClear();
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
    const getMarkdown = vi.fn(
      () => '---\ntitle: "x"\n---\n\nManual checkpoint body.',
    );

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
  });
});
