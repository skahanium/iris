import type { Editor } from "@tiptap/react";
import { act, createElement, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  useAppPersistenceLifecycle,
  type PersistBeforeLeave,
} from "@/hooks/useAppPersistenceLifecycle";
import type { TabItem } from "@/components/layout/TabBar";

const fileWrite = vi.fn();
const setCachedEditorHtml = vi.fn();
const versionSaveManual = vi.fn();

vi.mock("@/lib/ipc", () => ({
  appExit: vi.fn(),
  fileWrite: (...args: unknown[]) => fileWrite(...args),
  versionSaveIdle: vi.fn(),
  versionSaveManual: (...args: unknown[]) => versionSaveManual(...args),
}));

vi.mock("@/lib/editor-html-cache", () => ({
  editorHtmlDigest: vi.fn(() => "body-digest"),
  setCachedEditorHtml: (...args: unknown[]) => setCachedEditorHtml(...args),
}));

vi.mock("@/lib/tauri-runtime", () => ({
  isTauriRuntime: () => false,
}));

function Harness({
  editorReady,
  onReady,
  persistBeforeLeaveRef,
}: {
  editorReady: boolean;
  onReady?: (api: ReturnType<typeof useAppPersistenceLifecycle>) => void;
  persistBeforeLeaveRef: React.MutableRefObject<PersistBeforeLeave>;
}) {
  const path = "note.md";
  const markdown =
    '---\ntitle: "Note"\n---\n\nOriginal body that must remain authoritative.';
  const activePathRef = useRef<string | null>(path);
  const dirtyRef = useRef(true);
  const autoSnapshotGenerationRef = useRef(0);
  const editorReadyRef = useRef(editorReady);
  editorReadyRef.current = editorReady;
  const editorRef = useRef({
    getHTML: () => "<p></p>",
    isDestroyed: false,
  } as Editor);
  const getLiveMarkdownRef = useRef(() => markdown);
  const tabsRef = useRef<TabItem[]>([
    {
      dirty: true,
      locked: false,
      path,
      title: "Note",
    },
  ]);

  const api = useAppPersistenceLifecycle({
    activeFileLocked: false,
    activePath: path,
    activePathRef,
    applySavedMarkdown: vi.fn(),
    autoSnapshotGenerationRef,
    autoVersionEnabled: false,
    autoVersionIdleMinutes: 5,
    dirtyRef,
    editorReadyRef,
    editorRef,
    getLiveMarkdownRef,
    getTabMarkdownCached: vi.fn(),
    markClean: vi.fn(),
    noteTitle: "Note",
    persistBeforeLeaveRef,
    schedulePathSync: vi.fn(),
    setAiStatus: vi.fn(),
    setMarkdown: vi.fn(),
    syncTabMarkdownCache: vi.fn(),
    tabsRef,
  });
  onReady?.(api);
  return null;
}

describe("useAppPersistenceLifecycle", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    fileWrite.mockReset();
    fileWrite.mockResolvedValue({
      id: 1,
      path: "note.md",
      title: "Note",
      updated_at: "",
      word_count: 6,
    });
    setCachedEditorHtml.mockReset();
    versionSaveManual.mockReset();
    versionSaveManual.mockResolvedValue(undefined);
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("does not write or cache the active note while the editor is not ready for persistence", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorReady: false,
          persistBeforeLeaveRef,
        }),
      );
    });

    await act(async () => {
      await persistBeforeLeaveRef.current("note.md");
    });

    expect(fileWrite).not.toHaveBeenCalled();
    expect(setCachedEditorHtml).not.toHaveBeenCalled();
  });

  it("does not create a manual version snapshot while the editor is not ready", async () => {
    const persistBeforeLeaveRef = {
      current: async () => null,
    } as React.MutableRefObject<PersistBeforeLeave>;
    let api!: ReturnType<typeof useAppPersistenceLifecycle>;

    await act(async () => {
      root.render(
        createElement(Harness, {
          editorReady: false,
          onReady: (next) => {
            api = next;
          },
          persistBeforeLeaveRef,
        }),
      );
    });

    await act(async () => {
      await api.handleSaveVersion();
    });

    expect(fileWrite).not.toHaveBeenCalled();
    expect(versionSaveManual).not.toHaveBeenCalled();
  });
});
