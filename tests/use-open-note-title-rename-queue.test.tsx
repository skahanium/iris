import { act, createElement, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useOpenNote } from "@/hooks/useOpenNote";
import type { DocumentPersistenceMoveResult } from "@/lib/document-persistence-coordinator";

const documentRenameByTitle = vi.fn();

vi.mock("@/lib/ipc", () => ({
  documentRenameByTitle: (...args: unknown[]) => documentRenameByTitle(...args),
}));

interface HookApi {
  noteTitle: string;
  onTitleBlur: (title?: string) => void;
  setTitleFocused: (focused: boolean) => void;
}

function Harness({
  activePath,
  onReady,
  renamePersistedPath,
}: {
  activePath: { current: string | null };
  onReady: (api: HookApi) => void;
  renamePersistedPath: (
    oldPath: string,
    migrationPath: string,
    snapshot: string,
    move: () => Promise<DocumentPersistenceMoveResult>,
  ) => Promise<string>;
}) {
  const activePathRef = useRef(activePath.current);
  activePathRef.current = activePath.current;
  const api = useOpenNote({
    activePath: activePath.current,
    editorContentTick: 1,
    activePathRef,
    markdownRef: { current: "# body\n" },
    frontmatterYamlRef: { current: null },
    editorRef: { current: null },
    renamePersistedPath,
    updateTabTitle: vi.fn(),
    replaceOpenTabPath: (oldPath, newPath) => {
      if (activePath.current === oldPath) {
        activePath.current = newPath;
      }
    },
  });
  onReady({
    noteTitle: api.noteTitle,
    onTitleBlur: api.onTitleBlur,
    setTitleFocused: api.setTitleFocused,
  });
  return null;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

describe("useOpenNote title rename queue reconcile", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    documentRenameByTitle.mockReset();
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("reconciles title to the path stem when an older failed rename is superseded", async () => {
    const activePath = { current: "原标题.md" as string | null };
    let api!: HookApi;
    const firstBarrier = deferred<void>();
    let renameCalls = 0;
    const renamePersistedPath = vi.fn(
      async (
        _oldPath: string,
        _migrationPath: string,
        _snapshot: string,
        move: () => Promise<DocumentPersistenceMoveResult>,
      ) => {
        renameCalls += 1;
        if (renameCalls === 1) {
          await firstBarrier.promise;
          throw new Error("stale rename failed");
        }
        await move();
        throw new Error("latest rename failed");
      },
    );

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath,
          onReady: (next) => {
            api = next;
          },
          renamePersistedPath,
        }),
      );
    });

    await act(async () => {
      api.setTitleFocused(true);
      api.onTitleBlur("第一次");
    });
    // Let the first rename pass its generation check and park on the barrier.
    await act(async () => {
      await Promise.resolve();
    });
    expect(renameCalls).toBe(1);

    await act(async () => {
      api.onTitleBlur("第二次");
      api.setTitleFocused(false);
    });

    await act(async () => {
      firstBarrier.resolve();
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    await vi.waitFor(() => {
      expect(renameCalls).toBe(2);
      expect(api.noteTitle).toBe("原标题");
    });
    expect(activePath.current).toBe("原标题.md");
  });
});
