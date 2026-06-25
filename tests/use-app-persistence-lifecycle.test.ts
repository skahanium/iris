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

vi.mock("@/lib/ipc", () => ({
  appExit: vi.fn(),
  fileWrite: (...args: unknown[]) => fileWrite(...args),
  versionSaveIdle: vi.fn(),
  versionSaveManual: vi.fn(),
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
  persistBeforeLeaveRef,
}: {
  editorReady: boolean;
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

  useAppPersistenceLifecycle({
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
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("does not cache editor HTML while the active editor is not ready for persistence", async () => {
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

    expect(fileWrite).toHaveBeenCalledWith(
      "note.md",
      expect.stringContaining("Original body that must remain authoritative."),
    );
    expect(setCachedEditorHtml).not.toHaveBeenCalled();
  });
});
