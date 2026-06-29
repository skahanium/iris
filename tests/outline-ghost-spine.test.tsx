import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { readFileSync } from "node:fs";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { EditorOutline } from "@/components/editor/EditorOutline";
import { outlineFromDoc } from "@/lib/document-outline";
import { fileLinkSummary } from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({
  fileLinkSummary: vi.fn(),
}));

const mockFileLinkSummary = vi.mocked(fileLinkSummary);

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

function renderOutlineWithLinks(ed: Editor, onOpenNote = vi.fn(), open = true) {
  host = document.createElement("div");
  host.style.height = "640px";
  document.body.append(host);
  root = createRoot(host);
  act(() => {
    root?.render(
      <EditorOutline
        editor={ed}
        open={open}
        notePath="target.md"
        onOpenNote={onOpenNote}
        onOpenChange={() => {}}
      />,
    );
  });
  return onOpenNote;
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
  mockFileLinkSummary.mockReset();
  vi.restoreAllMocks();
});

describe("outline ghost spine", () => {
  it("renders a transparent text index instead of minimap ticks or captions", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");
    const css = read("src/styles/globals.css");

    expect(outline).toContain("outline-ghost--active");
    expect(outline).toContain("outline-ghost-item");
    expect(outline).not.toContain("outline-luminous-tick");
    expect(outline).not.toContain("OutlineLuminousCaption");
    expect(outline).not.toContain("getTickTop");
    expect(outline).not.toContain("nearestIndexFromPointer");
    expect(css).toContain(".outline-ghost");
    expect(css).toContain(".outline-ghost-item--active");
    expect(css).not.toContain(".outline-luminous-tick");
    expect(css).not.toContain(".outline-luminous-caption");
  });

  it("uses the same stacked row layout for short and long outlines", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");
    const css = read("src/styles/globals.css");

    expect(outline).not.toContain("useVirtualizer");
    expect(outline).not.toContain("VIRTUAL_OUTLINE_THRESHOLD");
    expect(outline).not.toContain("getTotalSize()");
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

  it("aligns clicked headings to the top of the editor viewport", () => {
    editor = makeEditor(["Intro", "Target", "After"], [1, 1, 1]);
    const entries = outlineFromDoc(editor.state.doc);
    renderOutline(editor);

    const targetHeading = editor.view.nodeDOM(
      entries[1]!.pos - 1,
    ) as HTMLElement | null;
    const targetScrollIntoView = vi.fn();
    Object.defineProperty(targetHeading!, "scrollIntoView", {
      configurable: true,
      value: targetScrollIntoView,
    });

    const items = Array.from(
      document.querySelectorAll<HTMLButtonElement>(
        '[data-testid="outline-ghost-item"]',
      ),
    );

    act(() => {
      items[1]?.click();
    });

    expect(editor.state.selection.head).toBe(entries[1]!.pos);
    expect(targetScrollIntoView).toHaveBeenCalledWith({
      block: "start",
      inline: "nearest",
    });
  });

  it("jumps on primary pointer down without waiting for a second click", () => {
    editor = makeEditor(["Intro", "Pointer Target", "After"], [1, 1, 1]);
    const entries = outlineFromDoc(editor.state.doc);
    renderOutline(editor);

    const targetHeading = editor.view.nodeDOM(
      entries[1]!.pos - 1,
    ) as HTMLElement | null;
    const targetScrollIntoView = vi.fn();
    Object.defineProperty(targetHeading!, "scrollIntoView", {
      configurable: true,
      value: targetScrollIntoView,
    });

    const items = Array.from(
      document.querySelectorAll<HTMLButtonElement>(
        '[data-testid="outline-ghost-item"]',
      ),
    );
    const event = new Event("pointerdown", { bubbles: true, cancelable: true });
    Object.defineProperty(event, "button", { value: 0 });

    act(() => {
      items[1]?.dispatchEvent(event);
    });

    expect(event.defaultPrevented).toBe(true);
    expect(editor.state.selection.head).toBe(entries[1]!.pos);
    expect(targetScrollIntoView).toHaveBeenCalledWith({
      block: "start",
      inline: "nearest",
    });
  });

  it("aligns locked-editor jumps without focusing the editor", () => {
    editor = makeEditor(["Intro", "Locked Target", "After"], [1, 1, 1]);
    const entries = outlineFromDoc(editor.state.doc);
    editor.commands.setTextSelection(entries[0]!.pos);
    editor.setEditable(false);
    const focusSpy = vi.spyOn(editor.view, "focus");
    renderOutline(editor, true, true);

    const targetHeading = editor.view.nodeDOM(
      entries[1]!.pos - 1,
    ) as HTMLElement | null;
    const targetScrollIntoView = vi.fn();
    Object.defineProperty(targetHeading!, "scrollIntoView", {
      configurable: true,
      value: targetScrollIntoView,
    });

    const items = Array.from(
      document.querySelectorAll<HTMLButtonElement>(
        '[data-testid="outline-ghost-item"]',
      ),
    );

    act(() => {
      items[1]?.click();
    });

    expect(editor.state.selection.head).toBe(entries[1]!.pos);
    expect(targetScrollIntoView).toHaveBeenCalledWith({
      block: "start",
      inline: "nearest",
    });
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

  it("uses relative heading levels so the shallowest present level sits on the baseline", () => {
    editor = makeEditor(
      ["只有二级", "二级续写", "三级细节", "跳级细节", "二级收束"],
      [2, 2, 3, 3, 2],
    );

    renderOutline(editor);

    const items = Array.from(
      document.querySelectorAll<HTMLElement>(
        '[data-testid="outline-ghost-item"]',
      ),
    );

    expect(items).toHaveLength(5);
    expect(items[0]?.style.paddingLeft).toBe(
      "calc(0rem + var(--editor-outline-text-offset))",
    );
    expect(items[1]?.style.paddingLeft).toBe(
      "calc(0rem + var(--editor-outline-text-offset))",
    );
    expect(items[2]?.style.paddingLeft).toBe(
      "calc(1.45rem + var(--editor-outline-text-offset))",
    );
    expect(items[3]?.style.paddingLeft).toBe(
      "calc(1.45rem + var(--editor-outline-text-offset))",
    );
    expect(items[4]?.style.paddingLeft).toBe(
      "calc(0rem + var(--editor-outline-text-offset))",
    );
  });

  it("compresses skipped heading levels when calculating outline indentation", () => {
    editor = makeEditor(["一级", "三级", "三级续写"], [1, 3, 3]);

    renderOutline(editor);

    const items = Array.from(
      document.querySelectorAll<HTMLElement>(
        '[data-testid="outline-ghost-item"]',
      ),
    );

    expect(items[0]?.style.paddingLeft).toBe(
      "calc(0rem + var(--editor-outline-text-offset))",
    );
    expect(items[1]?.style.paddingLeft).toBe(
      "calc(1.45rem + var(--editor-outline-text-offset))",
    );
    expect(items[2]?.style.paddingLeft).toBe(
      "calc(1.45rem + var(--editor-outline-text-offset))",
    );
  });

  it("keeps h3-only outlines on the top-level baseline", () => {
    editor = makeEditor(["三级章", "三级续章"], [3, 3]);

    renderOutline(editor);

    const items = Array.from(
      document.querySelectorAll<HTMLElement>(
        '[data-testid="outline-ghost-item"]',
      ),
    );

    expect(items[0]?.style.paddingLeft).toBe(
      "calc(0rem + var(--editor-outline-text-offset))",
    );
    expect(items[1]?.style.paddingLeft).toBe(
      "calc(0rem + var(--editor-outline-text-offset))",
    );
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
    expect(firstText?.className).not.toContain("whitespace-nowrap");
    expect(firstText?.hasAttribute("title")).toBe(false);

    const outline = read("src/components/editor/EditorOutline.tsx");
    expect(outline).not.toContain("title={entry.text}");
    expect(outline).not.toContain("whitespace-nowrap");

    const css = read("src/styles/globals.css");
    expect(css).toContain("text-align: left");
    expect(css).toContain(".outline-ghost-text");
    expect(css).not.toContain(".outline-ghost-marker");
    expect(css).not.toContain("grid-template-areas");
    expect(css).not.toContain("grid-area: text");
    expect(css).toContain("text-overflow: ellipsis");
    expect(css).toMatch(/\.outline-ghost-text \{[\s\S]*white-space: pre;/);
    expect(css).not.toMatch(
      /\.outline-ghost-text \{[\s\S]*white-space: nowrap;/,
    );
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

  it("keeps outline typography visually aligned with the title-bar tab text", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");
    const titleBar = read("src/components/layout/DesktopTitleBar.tsx");
    const css = read("src/styles/globals.css");

    expect(titleBar).toContain("text-xs");
    expect(titleBar).toContain("text-muted-foreground");
    expect(titleBar).toContain("text-[hsl(var(--outline-rail-active))]");
    expect(outline).toContain("text-[0.9375rem]");
    expect(outline).toContain("leading-[1.45rem]");
    expect(outline).toContain("font-normal");
    expect(outline).toContain("text-muted-foreground");
    expect(outline).toContain("text-[hsl(var(--outline-rail-active))]");
    expect(outline).not.toContain(
      "outline-ghost-item flex w-full items-center text-left text-xs",
    );
    expect(outline).not.toContain(
      "outline-ghost-item flex w-full items-center text-left text-sm",
    );

    expect(css).toContain("--outline-item-height: 2.25rem");
    expect(css).toContain("--outline-row-gap: 0.2rem");
    expect(css).toContain("--outline-font-family: inherit");
    expect(css).toContain("font-family: var(--outline-font-family)");
    expect(css).toContain("font-size: 0.9375rem");
    expect(css).toContain("font-weight: 400");
    expect(css).toContain("line-height: 1.45rem");
    expect(css).toContain("--outline-level-tone: hsl(var(--muted-foreground))");
    expect(css).toContain("color: var(--outline-level-tone)");
    expect(css).toContain("color: hsl(var(--outline-rail-active));");
    expect(css).not.toContain("Segoe UI Variable Text");
    expect(css).not.toContain("Microsoft YaHei UI Light");
    expect(css).not.toContain("--outline-text-level-1");
    expect(css).not.toContain("--outline-text-level-2");
    expect(css).not.toContain("--outline-text-level-3");
    expect(outline).not.toContain('fontSize: "0.8125rem"');
    expect(outline).not.toContain('fontSize: "0.75rem"');
    expect(outline).toContain('indent: "0rem"');
    expect(outline).toContain('indent: "1.45rem"');
    expect(outline).toContain('indent: "2.55rem"');
    expect(outline).toContain("paddingLeft");
    expect(css).not.toContain(".outline-ghost-indent");
    expect(css).toContain(
      "background: hsl(var(--outline-rail-active) / 0.075)",
    );
    expect(css).toContain("box-shadow: none");
    expect(css).not.toContain(
      "text-shadow: 0 1px 2px hsl(var(--background) / 0.95)",
    );
    expect(css).toContain(".outline-ghost-item--near-1");
    expect(css).toContain(".outline-ghost-item--near-2");
    expect(css).toMatch(/\.outline-ghost-item--near-1 \{[\s\S]*opacity: 0\.9/);
    expect(css).toMatch(/\.outline-ghost-item--near-2 \{[\s\S]*opacity: 0\.82/);
    expect(css).toContain("font-synthesis: none");
    expect(css).toContain("@media (prefers-reduced-motion: reduce)");
  });

  it("separates rail placement from a shared readable text offset", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");
    const css = read("src/styles/globals.css");

    expect(css).toContain("--editor-outline-inset: 0.75rem");
    expect(css).toContain("--editor-outline-text-offset: 2rem");
    expect(css).toMatch(
      /\.outline-ghost-list \{[\s\S]*padding: 0\.25rem 0\.25rem 0\.45rem 0;/,
    );
    expect(css).toMatch(
      /\.outline-ghost-item \{[\s\S]*padding-left: var\(--editor-outline-text-offset\)/,
    );
    expect(css).toMatch(
      /\.outline-link-summary \{[\s\S]*margin-left: var\(--editor-outline-text-offset\)/,
    );
    expect(outline).toContain("+ var(--editor-outline-text-offset)");
    expect(outline).not.toContain("+ var(--editor-outline-inset)");
    expect(outline).not.toContain("+ 0.5rem");
  });

  it("shows backlink summary below the open outline and opens linked notes", async () => {
    mockFileLinkSummary.mockResolvedValue({
      inboundCount: 2,
      outboundCount: 1,
      inbound: [
        {
          path: "source.md",
          title: "Source",
          context: "Source links [[Target]]",
        },
      ],
      outbound: [
        {
          path: "out.md",
          title: "Outbound",
          context: "Target links [[Outbound]]",
        },
      ],
    });
    editor = makeEditor(["Overview"]);
    const onOpenNote = renderOutlineWithLinks(editor);

    await act(async () => {
      await Promise.resolve();
    });

    expect(mockFileLinkSummary).toHaveBeenCalledWith("target.md");
    expect(
      document.querySelector('[data-testid="outline-link-summary"]')
        ?.textContent,
    ).toContain("2 入链");
    expect(
      document.querySelector('[data-testid="outline-link-summary"]')
        ?.textContent,
    ).toContain("1 出链");

    act(() => {
      document
        .querySelector<HTMLButtonElement>(
          '[data-testid="outline-link-summary-item"]',
        )
        ?.click();
    });

    expect(onOpenNote).toHaveBeenCalledWith("source.md");
  });

  it("does not render backlink summary while the outline is collapsed", () => {
    mockFileLinkSummary.mockResolvedValue({
      inboundCount: 0,
      outboundCount: 0,
      inbound: [],
      outbound: [],
    });
    editor = makeEditor(["Overview"]);

    renderOutlineWithLinks(editor, vi.fn(), false);

    expect(
      document.querySelector('[data-testid="outline-rail-handle"]'),
    ).not.toBeNull();
    expect(
      document.querySelector('[data-testid="outline-link-summary"]'),
    ).toBeNull();
    expect(mockFileLinkSummary).not.toHaveBeenCalled();
  });
});
