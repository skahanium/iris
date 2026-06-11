import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

/**
 * Prevents ProseMirror's DOMObserver from reading intermediate IME composition
 * mutations (raw pinyin) and committing them to the document.
 *
 * Workaround for upstream prosemirror-view#187 / tiptap#7271:
 * During IME composition the browser mutates the DOM with intermediate
 * (pre-confirmation) text. ProseMirror's MutationObserver processes these
 * mutations synchronously, which can corrupt the composition — leaving raw
 * pinyin in the heading while the confirmed Chinese characters are displaced
 * to a new paragraph.
 *
 * Strategy: override `DOMObserver.flush()` to a no-op while composing, so
 * mutations queue up but are never read. On `compositionend` we restore the
 * original `flush` and call `forceFlush()` to process the final DOM state.
 *
 * Upstream `prosemirror-view` was archived 2026-04 without a merge fix.
 */
const RESTORE_DELAY_MS = 50;

export const ImeCompositionGuardExtension = Extension.create({
  name: "imeCompositionGuard",

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey("imeCompositionGuard"),
        view: (editorView) => {
          const dom = editorView.dom;
          // ProseMirror's DOMObserver — not part of the public API.
          const domObserver = (editorView as unknown as Record<string, unknown>)
            .domObserver as
            | {
                flush: () => void;
                forceFlush: () => void;
              }
            | undefined;

          if (!domObserver) return {};

          let originalFlush: (() => void) | null = null;
          let restoreTimer: ReturnType<typeof setTimeout> | null = null;

          const noop = () => {
            /* swallow — mutations stay queued inside the observer */
          };

          const onStart = () => {
            if (originalFlush) return;
            clearTimeout(restoreTimer!);
            restoreTimer = null;
            originalFlush = domObserver.flush.bind(domObserver);
            domObserver.flush = noop;
          };

          const onEnd = () => {
            if (!originalFlush) return;
            const saved = originalFlush;
            originalFlush = null;
            restoreTimer = setTimeout(() => {
              restoreTimer = null;
              domObserver.flush = saved;
              domObserver.forceFlush();
            }, RESTORE_DELAY_MS);
          };

          dom.addEventListener("compositionstart", onStart, true);
          dom.addEventListener("compositionend", onEnd, true);

          return {
            destroy() {
              dom.removeEventListener("compositionstart", onStart, true);
              dom.removeEventListener("compositionend", onEnd, true);
              clearTimeout(restoreTimer!);
              if (originalFlush) {
                domObserver.flush = originalFlush;
              }
            },
          };
        },
      }),
    ];
  },
});
