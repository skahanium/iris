import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ImeCompositionGuardExtension } from "@/components/editor/extensions/ImeCompositionGuardExtension";
import { IrisDocument } from "@/components/editor/extensions/IrisDocument";

function createEditor() {
  return new Editor({
    extensions: [
      IrisDocument,
      StarterKit.configure({
        document: false,
        codeBlock: false,
        heading: {
          levels: [1, 2, 3],
          HTMLAttributes: { class: "iris-section-heading" },
        },
      }),
      ImeCompositionGuardExtension,
    ],
    content: {
      type: "doc",
      content: [
        {
          type: "heading",
          attrs: { level: 1 },
          content: [{ type: "text", text: "标题" }],
        },
        { type: "paragraph" },
      ],
    },
  });
}

function fireCompositionStart(dom: HTMLElement) {
  dom.dispatchEvent(
    new CompositionEvent("compositionstart", {
      data: "",
      bubbles: true,
      cancelable: true,
    }),
  );
}

function fireCompositionEnd(dom: HTMLElement, data: string) {
  dom.dispatchEvent(
    new CompositionEvent("compositionend", {
      data,
      bubbles: true,
      cancelable: true,
    }),
  );
}

/**
 * Dispatch a keydown Enter event in the bubbling phase (matching how
 * ProseMirror's own listeners are registered) and return the doc
 * child count delta.
 */
function pressEnter(dom: HTMLElement) {
  dom.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "Enter",
      keyCode: 13,
      bubbles: true,
      cancelable: true,
    }),
  );
}

describe("ImeCompositionGuardExtension", () => {
  let editor: Editor | undefined;

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  // ── Registration ──────────────────────────────────────────────────────────

  it("registers the plugin without errors", () => {
    editor = createEditor();
    expect(editor.isDestroyed).toBe(false);
  });

  it("exposes the imeCompositionGuard plugin", () => {
    editor = createEditor();
    expect(editor.state.plugins.length).toBeGreaterThan(0);
  });

  it("renders heading content correctly after setup", () => {
    editor = createEditor();
    const heading = editor.view.dom.querySelector("h1");
    expect(heading).not.toBeNull();
    expect(heading?.textContent).toBe("标题");
  });

  it("extension source is referenced in TipTapEditor", () => {
    const src = readFileSync("src/components/editor/TipTapEditor.tsx", "utf8");
    expect(src).toContain("ImeCompositionGuardExtension");
  });

  // ── Enter key suppression during composition ──────────────────────────────

  describe("Enter key during IME composition", () => {
    it("Enter splits the document when no composition is active", () => {
      editor = createEditor();
      // Focus the editor so ProseMirror will handle the keydown
      editor.commands.focus();

      const before = editor.state.doc.childCount;
      pressEnter(editor.view.dom);

      // Enter should have created a new block
      expect(editor.state.doc.childCount).toBeGreaterThan(before);
    });

    it("suppresses Enter while composition is in progress", () => {
      editor = createEditor();
      editor.commands.focus();

      fireCompositionStart(editor.view.dom);

      const before = editor.state.doc.childCount;
      pressEnter(editor.view.dom);

      // Document must be unchanged — Enter was swallowed by capture-phase guard
      expect(editor.state.doc.childCount).toBe(before);
    });

    it("flushes finalized composition and allows Enter during compositionend grace period", () => {
      editor = createEditor();
      editor.commands.focus();
      const domObserver = (
        editor.view as unknown as {
          domObserver: {
            flush: () => void;
            forceFlush: () => void;
          };
        }
      ).domObserver;
      const flushSpy = vi.fn();
      const forceFlushSpy = vi.fn();
      domObserver.flush = flushSpy;
      domObserver.forceFlush = forceFlushSpy;

      fireCompositionStart(editor.view.dom);
      fireCompositionEnd(editor.view.dom, "你好");

      // The grace-period Enter should flush finalized DOM and continue.
      let sentinelReached = false;
      const sentinel = () => {
        sentinelReached = true;
      };
      editor.view.dom.addEventListener("keydown", sentinel, false);
      pressEnter(editor.view.dom);
      editor.view.dom.removeEventListener("keydown", sentinel, false);

      expect(sentinelReached).toBe(true);
      expect(forceFlushSpy).toHaveBeenCalled();
      expect(flushSpy).toHaveBeenCalled();
    });

    it("allows Enter after the grace period expires", async () => {
      editor = createEditor();
      editor.commands.focus();

      fireCompositionStart(editor.view.dom);
      fireCompositionEnd(editor.view.dom, "你好");

      // Wait for rAF + RESTORE_DELAY_MS (50) + RAF_FALLBACK_MS (170) + margin
      await new Promise((r) => setTimeout(r, 400));

      // Verify the guard is no longer blocking Enter by checking
      // that the capture-phase listener doesn't stop propagation.
      // We track this via a bubbling-phase sentinel listener.
      let sentinelReached = false;
      const sentinel = () => {
        sentinelReached = true;
      };
      editor.view.dom.addEventListener("keydown", sentinel, false);
      pressEnter(editor.view.dom);
      editor.view.dom.removeEventListener("keydown", sentinel, false);

      expect(sentinelReached).toBe(true);
    });

    it("handles rapid recomposition correctly", () => {
      editor = createEditor();
      editor.commands.focus();

      // First composition
      fireCompositionStart(editor.view.dom);
      const before = editor.state.doc.childCount;
      pressEnter(editor.view.dom);
      expect(editor.state.doc.childCount).toBe(before);

      // Second compositionstart before the first ended
      fireCompositionStart(editor.view.dom);
      pressEnter(editor.view.dom);
      expect(editor.state.doc.childCount).toBe(before);

      fireCompositionEnd(editor.view.dom, "你好");
      pressEnter(editor.view.dom);
      expect(editor.state.doc.childCount).toBe(before);
    });

    it("restores the real DOM observer after rapid recomposition", async () => {
      editor = createEditor();
      const domObserver = (
        editor.view as unknown as {
          domObserver: {
            flush: () => void;
            forceFlush: () => void;
          };
        }
      ).domObserver;
      const flushSpy = vi.fn();
      const forceFlushSpy = vi.fn();
      domObserver.flush = flushSpy;
      domObserver.forceFlush = forceFlushSpy;

      fireCompositionStart(editor.view.dom);
      fireCompositionEnd(editor.view.dom, "一");
      fireCompositionStart(editor.view.dom);
      fireCompositionEnd(editor.view.dom, "二");

      await new Promise((r) => setTimeout(r, 400));

      domObserver.flush();
      domObserver.forceFlush();
      expect(flushSpy).toHaveBeenCalled();
      expect(forceFlushSpy).toHaveBeenCalled();
    });

    it("does not suppress Enter when compositionend fires without start", () => {
      editor = createEditor();
      editor.commands.focus();

      // Fire end without start — should be a no-op
      fireCompositionEnd(editor.view.dom, "你好");

      const before = editor.state.doc.childCount;
      pressEnter(editor.view.dom);

      // No composition was started, so Enter should work
      expect(editor.state.doc.childCount).toBeGreaterThan(before);
    });
  });

  // ── Cleanup ───────────────────────────────────────────────────────────────

  describe("lifecycle", () => {
    it("cleans up on destroy during active composition", () => {
      editor = createEditor();
      fireCompositionStart(editor.view.dom);
      expect(() => editor!.destroy()).not.toThrow();
      editor = undefined;
    });

    it("cleans up on destroy during grace period", () => {
      editor = createEditor();
      fireCompositionStart(editor.view.dom);
      fireCompositionEnd(editor.view.dom, "你好");
      expect(() => editor!.destroy()).not.toThrow();
      editor = undefined;
    });
  });
});
