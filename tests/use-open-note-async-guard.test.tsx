import type { Editor } from "@tiptap/react";
import { act, createElement, type RefObject } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useOpenNote } from "@/hooks/useOpenNote";

const ingestMarkdownForEditorAsync = vi.hoisted(() => vi.fn());

vi.mock("@/lib/editor-ingest-async", () => ({
  ingestMarkdownForEditorAsync,
}));

type HookApi = ReturnType<typeof useOpenNote>;

interface MockEditor {
  commands: {
    setContent: ReturnType<typeof vi.fn>;
  };
}

function Harness({
  activePath,
  dirtyRef,
  editorRef,
  markdownRef,
  onReady,
}: {
  activePath: string | null;
  dirtyRef: RefObject<boolean>;
  editorRef: RefObject<MockEditor | null>;
  markdownRef: RefObject<string>;
  onReady: (api: HookApi) => void;
}) {
  const api = useOpenNote({
    activePath,
    editorContentTick: 1,
    activePathRef: { current: activePath },
    markdownRef,
    frontmatterYamlRef: { current: null },
    editorRef: editorRef as RefObject<Editor | null>,
    dirtyRef,
    updateTabTitle: vi.fn(),
    replaceOpenTabPath: vi.fn(),
  });
  onReady(api);
  return null;
}

describe("useOpenNote async editor load guard", () => {
  let container: HTMLDivElement;
  let root: Root;
  let api!: HookApi;
  let resolveIngest!: (value: { tipTapHtml: string }) => void;
  let editor: MockEditor;
  let dirtyRef: { current: boolean };
  let markdownRef: { current: string };

  beforeEach(async () => {
    ingestMarkdownForEditorAsync.mockReset();
    ingestMarkdownForEditorAsync.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveIngest = resolve as (value: { tipTapHtml: string }) => void;
        }),
    );
    editor = {
      commands: {
        setContent: vi.fn(),
      },
    };
    dirtyRef = { current: false };
    markdownRef = { current: "# Initial\n\nBody" };
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "note.md",
          dirtyRef,
          editorRef: { current: editor },
          markdownRef,
          onReady: (next) => {
            api = next;
          },
        }),
      );
    });
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("does not apply delayed ingest HTML after the user has edited locally", async () => {
    act(() => {
      api.loadBodyIntoEditor("# Old title:\n\nBody");
    });
    expect(ingestMarkdownForEditorAsync).toHaveBeenCalled();

    dirtyRef.current = true;

    await act(async () => {
      resolveIngest({ tipTapHtml: "<h1>Old title:</h1><p>Body</p>" });
      await Promise.resolve();
    });

    expect(editor.commands.setContent).not.toHaveBeenCalled();
  });
});
