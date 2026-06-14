import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { readFileSync } from "node:fs";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { EditorOutline } from "@/components/editor/EditorOutline";
import { outlineFromDoc } from "@/lib/document-outline";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

let root: Root | null = null;
let host: HTMLDivElement | null = null;
let editor: Editor | null = null;

function makeEditor(
  markdownHeadings: string[],
  levels?: Array<1 | 2 | 3>,
): Editor {
  const content = markdownHeadings
    .map((text, index) => {
      const level = levels?.[index] ?? (((index % 3) + 1) as 1 | 2 | 3);
      return `<h${level}>${text}</h${level}><p>正文 ${index + 1}</p>`;
    })
    .join("");

  return new Editor({
    extensions: [StarterKit],
    content,
  });
}

function renderOutline(ed: Editor, open = true, locked = false) {
  host = document.createElement("div");
  host.style.height = "640px";
  document.body.append(host);
  root = createRoot(host);
  act(() => {
    root?.render(
      <EditorOutline
        editor={ed}
        open={open}
        locked={locked}
        onOpenChange={() => {}}
      />,
    );
  });
}

function press(key: string) {
  const rail = document.querySelector<HTMLElement>(
    '[data-testid="outline-rail"]',
  );
  if (!rail) throw new Error("missing outline rail");
  act(() => {
    rail.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
  });
}

afterEach(() => {
  if (root) {
    act(() => root?.unmount());
  }
  editor?.destroy();
  host?.remove();
  root = null;
  host = null;
  editor = null;
  vi.restoreAllMocks();
});

describe("outline ghost spine", () => {
  it("renders a transparent text index instead of minimap ticks or captions", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");
    const css = read("src/styles/globals.css");

    expect(outline).toContain("outline-ghost--active");
    expect(outline).toContain("outline-ghost-item");
    expect(outline).toContain("useVirtualizer");
    expect(outline).not.toContain("outline-luminous-tick");
    expect(outline).not.toContain("OutlineLuminousCaption");
    expect(outline).not.toContain("getTickTop");
    expect(outline).not.toContain("nearestIndexFromPointer");
    expect(css).toContain(".outline-ghost");
    expect(css).toContain(".outline-ghost-item--active");
    expect(css).not.toContain(".outline-luminous-tick");
    expect(css).not.toContain(".outline-luminous-caption");
  });

  it("uses virtualized rows for long outlines without absolute tick geometry", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");
    const css = read("src/styles/globals.css");

    expect(outline).toContain("VIRTUAL_OUTLINE_THRESHOLD");
    expect(outline).toContain("entries.length >= VIRTUAL_OUTLINE_THRESHOLD");
    expect(outline).toContain("getTotalSize()");
    expect(css).not.toContain("--outline-tick-top");
    expect(css).not.toContain("top: var(--outline-tick-top)");
  });

  it("marks the active section and jumps with keyboard navigation", () => {
    editor = makeEditor(["第一章", "第二节", "第三段"]);
    const entries = outlineFromDoc(editor.state.doc);
    editor.commands.setTextSelection(entries[0]!.pos);
    const scrollSpy = vi.fn();
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      configurable: true,
      value: scrollSpy,
    });

    renderOutline(editor);

    expect(
      document.querySelector(
        '[data-testid="outline-ghost-item"][aria-current="location"]',
      )?.textContent,
    ).toContain("第一章");

    press("ArrowDown");
    press("Enter");

    expect(editor.state.selection.head).toBe(entries[1]!.pos);
    expect(scrollSpy).toHaveBeenCalled();
  });

  it("jumps to outline entries while the editor is locked without focusing the editor", () => {
    editor = makeEditor(["第一章", "第二节", "第三段"]);
    const entries = outlineFromDoc(editor.state.doc);
    editor.commands.setTextSelection(entries[0]!.pos);
    editor.setEditable(false);
    const focusSpy = vi.spyOn(editor.view, "focus");
    vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
      callback(0);
      return 0;
    });
    const scrollSpy = vi.fn();
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      configurable: true,
      value: scrollSpy,
    });

    renderOutline(editor, true, true);

    const items = Array.from(
      document.querySelectorAll<HTMLButtonElement>(
        '[data-testid="outline-ghost-item"]',
      ),
    );

    act(() => {
      items[1]?.click();
    });

    expect(editor.state.selection.head).toBe(entries[1]!.pos);
    expect(scrollSpy).toHaveBeenCalled();
    expect(focusSpy).not.toHaveBeenCalled();
  });

  it("keeps adjacent top-level headings as full-width stacked rows", () => {
    editor = makeEditor(
      ["chidafan", "Shui大叫", "sha d j k na s j k d"],
      [1, 1, 1],
    );

    renderOutline(editor);

    const rail = document.querySelector<HTMLElement>(
      '[data-testid="outline-rail"]',
    );
    const list = document.querySelector<HTMLElement>(".outline-ghost-list");
    const items = Array.from(
      document.querySelectorAll<HTMLElement>(
        '[data-testid="outline-ghost-item"]',
      ),
    );

    expect(rail?.className).toContain("w-[var(--editor-outline-rail-width)]");
    expect(rail?.className).toContain(
      "min-w-[var(--editor-outline-rail-width)]",
    );
    expect(list?.className).toContain("flex");
    expect(list?.className).toContain("flex-col");
    expect(items).toHaveLength(3);
    expect(items.map((item) => item.textContent)).toEqual([
      "chidafan",
      "Shui大叫",
      "sha d j k na s j k d",
    ]);
    for (const item of items) {
      expect(item.className).toContain("outline-ghost-item");
      expect(item.className).toContain("w-full");
    }
  });

  it("keeps level labels in a left column and truncates long left-aligned titles", () => {
    editor = makeEditor(
      ["很长很长很长很长很长很长很长很长的一级标题", "二级标题", "三级标题"],
      [1, 2, 3],
    );

    renderOutline(editor);

    const items = Array.from(
      document.querySelectorAll<HTMLElement>(
        '[data-testid="outline-ghost-item"]',
      ),
    );
    const firstText = items[0]?.querySelector(".outline-ghost-text");

    expect(items[0]?.querySelector(".outline-ghost-marker")).toBeNull();
    expect(firstText).not.toBeNull();
    expect(items[0]?.className).toContain("flex");
    expect(items[0]?.className).not.toContain("grid-cols-[");
    expect(firstText?.className).toContain("block");
    expect(firstText?.className).toContain("flex-1");
    expect(firstText?.className).toContain("text-left");
    expect(firstText?.className).toContain("min-w-0");
    expect(firstText?.className).toContain("overflow-hidden");
    expect(firstText?.className).toContain("text-ellipsis");
    expect(firstText?.className).toContain("whitespace-nowrap");

    const css = read("src/styles/globals.css");
    expect(css).toContain("text-align: left");
    expect(css).toContain(".outline-ghost-text");
    expect(css).not.toContain(".outline-ghost-marker");
    expect(css).not.toContain("grid-template-areas");
    expect(css).not.toContain("grid-area: text");
    expect(css).toContain("text-overflow: ellipsis");
  });

  it("adds active-neighborhood classes around the selected heading", () => {
    editor = makeEditor(["一", "二", "三", "四", "五"], [1, 1, 1, 1, 1]);
    const entries = outlineFromDoc(editor.state.doc);
    editor.commands.setTextSelection(entries[2]!.pos);

    renderOutline(editor);

    const items = Array.from(
      document.querySelectorAll<HTMLElement>(
        '[data-testid="outline-ghost-item"]',
      ),
    );

    expect(items.map((item) => item.className)).toEqual([
      expect.stringContaining("outline-ghost-item--near-2"),
      expect.stringContaining("outline-ghost-item--near-1"),
      expect.stringContaining("outline-ghost-item--active"),
      expect.stringContaining("outline-ghost-item--near-1"),
      expect.stringContaining("outline-ghost-item--near-2"),
    ]);
  });

  it("does not add active-neighborhood classes outside outline bounds", () => {
    editor = makeEditor(["一", "二", "三"], [1, 1, 1]);
    const entries = outlineFromDoc(editor.state.doc);
    editor.commands.setTextSelection(entries[0]!.pos);

    renderOutline(editor);

    const items = Array.from(
      document.querySelectorAll<HTMLElement>(
        '[data-testid="outline-ghost-item"]',
      ),
    );

    expect(items[0]?.className).toContain("outline-ghost-item--active");
    expect(items[1]?.className).toContain("outline-ghost-item--near-1");
    expect(items[2]?.className).toContain("outline-ghost-item--near-2");
    expect(items[0]?.className).not.toContain("outline-ghost-item--near");
  });

  it("defines hierarchy, active emphasis, spacing, and reduced-motion styles", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");
    const css = read("src/styles/globals.css");

    expect(outline).toContain("const OUTLINE_ROW_HEIGHT = 56");
    expect(css).toContain("row-gap: 0.65rem");
    expect(css).toContain("min-height: 3rem");
    expect(css).toContain(".outline-ghost-item--level-1");
    expect(css).toContain(".outline-ghost-item--level-2");
    expect(css).toContain(".outline-ghost-item--level-3");
    expect(css).toContain("--outline-level-size: 0.95rem");
    expect(css).toContain("--outline-level-size: 0.82rem");
    expect(css).toContain("--outline-level-size: 0.72rem");
    expect(css).toContain("--outline-text-indent: 0rem");
    expect(css).toContain("--outline-text-indent: 1.35rem");
    expect(css).toContain("--outline-text-indent: 2.5rem");
    expect(outline).toContain("paddingLeft");
    expect(css).not.toContain(".outline-ghost-indent");
    expect(css).toContain("font-size: calc(var(--outline-level-size))");
    expect(css).toContain(".outline-ghost-item--near-1");
    expect(css).toContain(".outline-ghost-item--near-2");
    expect(css).toContain("opacity: 0.85");
    expect(css).toContain("opacity: 0.78");
    expect(css).toContain("font-weight: 400");
    expect(css).toContain("@media (prefers-reduced-motion: reduce)");
  });
});
