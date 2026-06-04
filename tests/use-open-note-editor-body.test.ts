import { act } from "react";
import { createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useOpenNote } from "@/hooks/useOpenNote";

function Harness({
  activePath,
  markdown,
  editorContentTick,
  outRef,
}: {
  activePath: string | null;
  markdown: string;
  editorContentTick: number;
  outRef: {
    current: {
      editorBodyMarkdown: string;
      bodyMarkdown: string;
    } | null;
  };
}) {
  const api = useOpenNote({
    activePath,
    editorContentTick,
    activePathRef: { current: activePath },
    markdownRef: { current: markdown },
    frontmatterYamlRef: { current: null },
    editorRef: { current: null },
    updateTabTitle: vi.fn(),
    replaceOpenTabPath: vi.fn(),
  });
  outRef.current = {
    editorBodyMarkdown: api.editorBodyMarkdown,
    bodyMarkdown: api.bodyMarkdown,
  };
  return null;
}

describe("useOpenNote editorBodyMarkdown", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("derives editor body on first render when markdown is already loaded", async () => {
    const md = '---\ntitle: "Note"\n---\n\nHello body';
    const outRef: {
      current: {
        editorBodyMarkdown: string;
        bodyMarkdown: string;
      } | null;
    } = { current: null };

    await act(async () => {
      root.render(
        createElement(Harness, {
          activePath: "note.md",
          markdown: md,
          editorContentTick: 1,
          outRef,
        }),
      );
    });

    expect(outRef.current?.editorBodyMarkdown.trim()).toBe("Hello body");
    expect(outRef.current?.bodyMarkdown.trim()).toBe("Hello body");
  });
});
