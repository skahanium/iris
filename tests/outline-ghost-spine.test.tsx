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

function renderOutline(
  ed: Editor,
  open = true,
  locked = false,
  onOpenChange = vi.fn(),
) {
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
        onOpenChange={onOpenChange}
      />,
    );
  });
  return onOpenChange;
}

function renderOutlineWithPath(ed: Editor, open = true) {
  host = document.createElement("div");
  host.style.height = "640px";
  document.body.append(host);
  root = createRoot(host);
  act(() => {
    root?.render(
      <EditorOutline editor={ed} open={open} onOpenChange={() => {}} />,
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
  mockFileLinkSummary.mockReset();
  vi.restoreAllMocks();
});

describe("outline ghost spine", () => {
  it("renders a floating bar rail instead of minimap ticks or always-visible text rows", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");
    const css = read("src/styles/globals.css");

    expect(outline).toContain("outline-ghost--active");
    expect(outline).toContain("outline-ghost-item");
    expect(outline).toContain("outline-ghost-items");
    expect(outline).toContain("outline-ghost-bar-track");
    expect(outline).toContain("outline-ghost-item-line");
    expect(outline).toContain("outline-ghost-popover");
    expect(outline).not.toContain("ListTree");
    expect(outline).not.toContain('data-testid="outline-rail-handle"');
    expect(outline).not.toContain("outline-ghost-handle");
    expect(outline).not.toContain("显示目录");
    expect(outline).not.toContain("隐藏目录");
    expect(outline).not.toContain("outline-luminous-tick");
    expect(outline).not.toContain("OutlineLuminousCaption");
    expect(outline).not.toContain("getTickTop");
    expect(outline).not.toContain("nearestIndexFromPointer");
    expect(css).toContain(".outline-ghost");
    expect(css).toMatch(/\.outline-ghost \{[\s\S]*top: 50%/);
    expect(css).toMatch(
      /\.outline-ghost \{[\s\S]*transform: translateY\(-50%\)/,
    );
    expect(css).toContain(".outline-ghost-item--active");
    expect(css).toContain(".outline-ghost-item-line");
    expect(css).toContain(".outline-ghost-popover");
    expect(css).not.toContain(".outline-ghost-handle");
    expect(css).not.toContain(".outline-ghost-popover-list");
    expect(css).not.toContain(".outline-ghost-popover-item");
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
      document
        .querySelector(
          '[data-testid="outline-ghost-item"][aria-current="location"]',
        )
        ?.getAttribute("aria-label"),
    ).toBe("第一章");

    press("ArrowDown");
    press("Enter");

    expect(editor.state.selection.head).toBe(entries[1]!.pos);
    expect(scrollSpy).toHaveBeenCalled();
  });

  it("keeps the outline rail resident and ignores close shortcuts", () => {
    editor = makeEditor(["常驻一", "常驻二"], [1, 1]);
    const onOpenChange = renderOutline(editor, false, false);

    expect(
      document.querySelector('[data-testid="outline-rail"]'),
    ).not.toBeNull();
    expect(
      document.querySelector('[data-testid="outline-rail-handle"]'),
    ).toBeNull();
    expect(
      document.querySelectorAll('[data-testid="outline-ghost-item"]'),
    ).toHaveLength(2);

    press("Escape");

    expect(onOpenChange).not.toHaveBeenCalled();
    expect(
      document.querySelector('[data-testid="outline-rail"]'),
    ).not.toBeNull();
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

  it("keeps adjacent top-level headings as compact animated rail bars", () => {
    editor = makeEditor(
      ["chidafan", "Shui大叫", "sha d j k na s j k d"],
      [1, 1, 1],
    );

    renderOutline(editor);

    const rail = document.querySelector<HTMLElement>(
      '[data-testid="outline-rail"]',
    );
    const list = document.querySelector<HTMLElement>(".outline-ghost-list");
    const itemGroup = document.querySelector<HTMLElement>(
      ".outline-ghost-items",
    );
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
    expect(itemGroup).not.toBeNull();
    expect(itemGroup?.className).toContain("outline-ghost-items");
    expect(itemGroup?.parentElement).toBe(list);
    expect(items).toHaveLength(3);
    for (const item of items) {
      expect(item.parentElement).toBe(itemGroup);
    }
    expect(items.map((item) => item.getAttribute("aria-label"))).toEqual([
      "chidafan",
      "Shui大叫",
      "sha d j k na s j k d",
    ]);
    for (const item of items) {
      expect(item.className).toContain("outline-ghost-item");
      expect(item.className).toContain("w-full");
      expect(item.querySelector(".outline-ghost-item-line")).not.toBeNull();
      expect(item.querySelector(".outline-ghost-text")).toBeNull();
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
      "calc(0rem + var(--editor-outline-bar-offset))",
    );
    expect(items[1]?.style.paddingLeft).toBe(
      "calc(0rem + var(--editor-outline-bar-offset))",
    );
    expect(items[2]?.style.paddingLeft).toBe(
      "calc(0.55rem + var(--editor-outline-bar-offset))",
    );
    expect(items[3]?.style.paddingLeft).toBe(
      "calc(0.55rem + var(--editor-outline-bar-offset))",
    );
    expect(items[4]?.style.paddingLeft).toBe(
      "calc(0rem + var(--editor-outline-bar-offset))",
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
      "calc(0rem + var(--editor-outline-bar-offset))",
    );
    expect(items[1]?.style.paddingLeft).toBe(
      "calc(0.55rem + var(--editor-outline-bar-offset))",
    );
    expect(items[2]?.style.paddingLeft).toBe(
      "calc(0.55rem + var(--editor-outline-bar-offset))",
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
      "calc(0rem + var(--editor-outline-bar-offset))",
    );
    expect(items[1]?.style.paddingLeft).toBe(
      "calc(0rem + var(--editor-outline-bar-offset))",
    );
  });

  it("anchors the hover title beside the hovered bar", () => {
    editor = makeEditor(["Intro", "Hovered Target", "After"], [1, 1, 1]);
    renderOutline(editor);

    const rail = document.querySelector<HTMLElement>(
      '[data-testid="outline-rail"]',
    );
    const items = Array.from(
      document.querySelectorAll<HTMLElement>(
        '[data-testid="outline-ghost-item"]',
      ),
    );
    expect(rail).not.toBeNull();
    expect(items).toHaveLength(3);

    const originalRailRect = rail!.getBoundingClientRect.bind(rail);
    const originalTargetRect = items[1]!.getBoundingClientRect.bind(items[1]);
    Object.defineProperty(rail!, "getBoundingClientRect", {
      configurable: true,
      value: () => ({
        ...originalRailRect(),
        top: 40,
        bottom: 400,
        height: 360,
      }),
    });
    Object.defineProperty(items[1]!, "getBoundingClientRect", {
      configurable: true,
      value: () => ({
        ...originalTargetRect(),
        top: 158,
        bottom: 178,
        height: 20,
      }),
    });

    act(() => {
      items[1]?.dispatchEvent(new Event("pointerover", { bubbles: true }));
    });

    const popover = document.querySelector<HTMLElement>(
      '[data-testid="outline-ghost-popover"]',
    );
    expect(popover?.textContent).toContain("Hovered Target");
    expect(popover?.style.getPropertyValue("--outline-popover-top")).toBe(
      "128px",
    );
  });
  it("reveals only the hovered or focused title inside the popover", () => {
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
    const firstLine = items[0]?.querySelector(".outline-ghost-item-line");

    expect(items[0]?.querySelector(".outline-ghost-marker")).toBeNull();
    expect(firstLine).not.toBeNull();
    expect(items[0]?.className).toContain("flex");
    expect(items[0]?.className).not.toContain("grid-cols-[");
    expect(items[0]?.querySelector(".outline-ghost-text")).toBeNull();

    act(() => {
      items[0]?.focus();
    });

    const popover = document.querySelector<HTMLElement>(
      '[data-testid="outline-ghost-popover"]',
    );
    expect(popover?.textContent).toContain(
      "很长很长很长很长很长很长很长很长的一级标题",
    );
    expect(popover?.textContent).toContain("H1");
    expect(popover?.textContent).not.toContain("二级标题");
    expect(popover?.textContent).not.toContain("三级标题");
    expect(popover?.querySelector(".outline-ghost-popover-list")).toBeNull();
    expect(popover?.querySelector(".outline-ghost-popover-item")).toBeNull();

    const outline = read("src/components/editor/EditorOutline.tsx");
    expect(outline).not.toContain("title={entry.text}");
    expect(outline).toContain('data-testid="outline-ghost-popover"');
    expect(outline).not.toContain("outline-ghost-popover-list");
    expect(outline).not.toContain("outline-ghost-popover-item");

    const css = read("src/styles/globals.css");
    expect(css).toContain(".outline-ghost-popover");
    expect(css).not.toContain(".outline-ghost-popover-list");
    expect(css).not.toContain(".outline-ghost-popover-item");
    expect(css).not.toContain(".outline-ghost-marker");
    expect(css).not.toContain("grid-template-areas");
    expect(css).not.toContain("grid-area: text");
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
    expect(outline).toContain("outline-ghost-item-line");
    expect(outline).toContain("text-[hsl(var(--outline-rail-active))]");
    expect(outline).not.toContain(
      "outline-ghost-item flex w-full items-center text-left text-xs",
    );
    expect(outline).not.toContain(
      "outline-ghost-item flex w-full items-center text-left text-sm",
    );

    expect(css).toContain("--outline-item-height: 1.05rem");
    expect(css).toContain("--outline-row-gap: 0.84rem");
    expect(css).toContain("--outline-bar-width: 0.95rem");
    expect(css).toContain("--outline-bar-active-width: 3rem");
    expect(css).toContain("--outline-bar-candidate-width: 3.5rem");
    expect(css).toContain("height: 4px");
    expect(css).toContain("height: min(74.4dvh, 33.6rem)");
    expect(css).toMatch(/\.outline-ghost-list \{[\s\S]*overflow-y: auto;/);
    expect(css).toMatch(/\.outline-ghost-list::before \{[\s\S]*bottom: 0;/);
    expect(css).toMatch(/\.outline-ghost-items \{[\s\S]*margin-block: auto;/);
    expect(css).toMatch(
      /\.outline-ghost-items \{[\s\S]*row-gap: var\(--outline-row-gap\);/,
    );
    expect(css).toMatch(/\.outline-ghost-items \{[\s\S]*flex: 0 0 auto;/);
    expect(css).toContain("transition: width 180ms var(--motion-ease)");
    expect(css).not.toContain("Segoe UI Variable Text");
    expect(css).not.toContain("Microsoft YaHei UI Light");
    expect(css).not.toContain("--outline-text-level-1");
    expect(css).not.toContain("--outline-text-level-2");
    expect(css).not.toContain("--outline-text-level-3");
    expect(outline).not.toContain('fontSize: "0.8125rem"');
    expect(outline).not.toContain('fontSize: "0.75rem"');
    expect(outline).toContain('indent: "0rem"');
    expect(outline).toContain('indent: "0.55rem"');
    expect(outline).toContain('indent: "1.1rem"');
    expect(outline).toContain("paddingLeft");
    expect(css).not.toContain(".outline-ghost-indent");
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
    expect(css).toContain("--editor-outline-bar-offset: 0.45rem");
    expect(css).toMatch(
      /\.outline-ghost-list \{[\s\S]*padding: 0\.25rem 0\.35rem 0\.45rem 0;/,
    );
    expect(css).toMatch(/\.outline-ghost-list \{[\s\S]*min-height: 100%;/);
    expect(css).not.toContain("min-height: calc(min(74.4dvh, 33.6rem) - 3rem)");
    expect(css).toMatch(
      /\.outline-ghost-item \{[\s\S]*padding-left: var\(--editor-outline-bar-offset\)/,
    );
    expect(css).not.toContain(".outline-link-summary");
    expect(outline).toContain("+ var(--editor-outline-bar-offset)");
    expect(outline).not.toContain("+ var(--editor-outline-inset)");
    expect(outline).not.toContain("+ 0.5rem");
  });

  it("keeps backlink summary out of the floating outline", async () => {
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
    renderOutlineWithPath(editor);

    await act(async () => {
      await Promise.resolve();
    });

    expect(
      document.querySelector('[data-testid="outline-link-summary"]'),
    ).toBeNull();
    expect(mockFileLinkSummary).not.toHaveBeenCalled();
  });

  it("does not load backlink summary while the resident outline receives a stale closed prop", () => {
    mockFileLinkSummary.mockResolvedValue({
      inboundCount: 0,
      outboundCount: 0,
      inbound: [],
      outbound: [],
    });
    editor = makeEditor(["Overview"]);

    renderOutlineWithPath(editor, false);

    expect(
      document.querySelector('[data-testid="outline-rail-handle"]'),
    ).toBeNull();
    expect(
      document.querySelector('[data-testid="outline-rail"]'),
    ).not.toBeNull();
    expect(
      document.querySelector('[data-testid="outline-link-summary"]'),
    ).toBeNull();
    expect(mockFileLinkSummary).not.toHaveBeenCalled();
  });
});
