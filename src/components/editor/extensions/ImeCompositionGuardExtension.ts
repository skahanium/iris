import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

/**
 * Prevents ProseMirror's DOMObserver from reading intermediate IME composition
 * mutations (raw pinyin) and committing them to the document. Also blocks
 * Enter key handling during the composition grace period so ProseMirror
 * doesn't split blocks while the IME is finalizing confirmed text.
 *
 * This addresses two upstream gaps in prosemirror-view:
 *
 * 1. `DOMObserver.forceFlush()` is NOT suppressed by the stock
 *    `inOrNearComposition` guard during the compositionend → keydown
 *    gap on Windows (Chrome/Edge/WebView2). ProseMirror calls
 *    `forceFlush()` in `editHandlers.keydown` before dispatching to
 *    plugin key handlers, so a noop'd `flush` alone is insufficient.
 *
 * 2. The `inOrNearComposition` guard only adds a post-compositionend
 *    grace period for Safari. On Windows, `compositionend` fires
 *    before `keydown(Enter)`, setting `view.composing = false` before
 *    the Enter key is evaluated — so the stock guard does not fire.
 *
 * Strategy:
 *   - Override BOTH `flush()` and `forceFlush()` to no-ops while
 *     composing so queued mutations are never read mid-composition.
 *   - Track a `composing` flag that stays true through the grace
 *     period after `compositionend`.
 *   - Add a capture-phase DOM `keydown` listener that intercepts
 *     Enter when `composing` is true, preventing ProseMirror's
 *     bubbling-phase handler from ever seeing the event.
 *   - On `compositionend`, wait for the next animation frame + a
 *     small delay, then restore both methods and call the real
 *     `forceFlush()` to sync the final DOM state.
 */
const RESTORE_DELAY_MS = 50;
const RAF_FALLBACK_MS = RESTORE_DELAY_MS + 120;

export const ImeCompositionGuardExtension = Extension.create({
  name: "imeCompositionGuard",

  addProseMirrorPlugins() {
    let composing = false;

    return [
      new Plugin({
        key: new PluginKey("imeCompositionGuard"),
        view: (editorView) => {
          const dom = editorView.dom;
          const domObserver = (editorView as unknown as Record<string, unknown>)
            .domObserver as
            | {
                flush: () => void;
                forceFlush: () => void;
              }
            | undefined;

          if (!domObserver) return {};

          let originalFlush: (() => void) | null = null;
          let originalForceFlush: (() => void) | null = null;
          let restoreTimer: ReturnType<typeof setTimeout> | null = null;
          let rafId: number | null = null;

          const noop = () => {
            /* swallow */
          };

          const onStart = () => {
            composing = true;
            clearTimeout(restoreTimer!);
            restoreTimer = null;
            if (rafId != null) {
              cancelAnimationFrame(rafId);
              rafId = null;
            }

            if (originalFlush) return;

            originalFlush = domObserver.flush.bind(domObserver);
            originalForceFlush = domObserver.forceFlush.bind(domObserver);
            domObserver.flush = noop;
            domObserver.forceFlush = noop;
          };

          const onEnd = () => {
            if (!originalFlush) return;

            const savedFlush = originalFlush!;
            const savedForceFlush = originalForceFlush!;
            originalFlush = null;
            originalForceFlush = null;

            let restored = false;

            const doRestore = () => {
              if (restored) return;
              restored = true;
              composing = false;
              domObserver.flush = savedFlush;
              domObserver.forceFlush = savedForceFlush;
              savedForceFlush();
            };

            rafId = requestAnimationFrame(() => {
              rafId = null;
              if (restored) return;
              restoreTimer = setTimeout(() => {
                restoreTimer = null;
                doRestore();
              }, RESTORE_DELAY_MS);
            });

            // Fallback: if rAF stalls (background tab), restore anyway
            restoreTimer = setTimeout(() => {
              if (rafId != null) {
                cancelAnimationFrame(rafId);
                rafId = null;
              }
              if (restoreTimer) {
                clearTimeout(restoreTimer);
                restoreTimer = null;
              }
              doRestore();
            }, RAF_FALLBACK_MS);
          };

          /**
           * Capture-phase keydown interceptor.
           *
           * ProseMirror registers its keydown handler in the bubbling phase
           * (`initInput` → addEventListener without `capture`). A capture-
           * phase listener on the same element fires first, giving us the
           * chance to swallow Enter before ProseMirror's `splitBlock` keymap
           * sees it. `stopPropagation()` prevents the event from reaching
           * the bubbling phase on this element.
           */
          const onKeyDown = (event: KeyboardEvent) => {
            if (composing && (event.key === "Enter" || event.keyCode === 13)) {
              event.stopPropagation();
              event.preventDefault();
            }
          };

          dom.addEventListener("compositionstart", onStart, true);
          dom.addEventListener("compositionend", onEnd, true);
          dom.addEventListener("keydown", onKeyDown, true);

          return {
            destroy() {
              composing = false;
              dom.removeEventListener("compositionstart", onStart, true);
              dom.removeEventListener("compositionend", onEnd, true);
              dom.removeEventListener("keydown", onKeyDown, true);
              clearTimeout(restoreTimer!);
              if (rafId != null) {
                cancelAnimationFrame(rafId);
                rafId = null;
              }
              if (originalFlush) {
                domObserver.flush = originalFlush;
                domObserver.forceFlush = originalForceFlush!;
              }
            },
          };
        },
      }),
    ];
  },
});
