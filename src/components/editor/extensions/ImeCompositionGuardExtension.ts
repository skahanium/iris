import { Extension } from "@tiptap/core";
import { splitListItem } from "@tiptap/pm/schema-list";
import { Plugin, PluginKey } from "@tiptap/pm/state";

/**
 * Prevents ProseMirror's DOMObserver from reading intermediate IME composition
 * mutations (raw pinyin) and committing them to the document. During the
 * post-composition Enter window, it restores and flushes the final DOM text
 * before list handling runs, so Enter never acts on a stale empty list item.
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
 *   - Track separate active-composition and post-composition grace flags.
 *   - Swallow Enter only while composition is still active.
 *   - On the first Enter after `compositionend`, restore both methods and
 *     call the real `forceFlush()` plus `flush()`; `forceFlush()` alone can
 *     be a no-op when ProseMirror has queued mutations but no flush timer.
 *   - If that Enter is inside a list item, split the list item immediately
 *     after flushing so ProseMirror cannot treat stale state as an empty item.
 */
const RESTORE_DELAY_MS = 50;
const RAF_FALLBACK_MS = RESTORE_DELAY_MS + 120;

export const ImeCompositionGuardExtension = Extension.create({
  name: "imeCompositionGuard",

  addProseMirrorPlugins() {
    let composing = false;
    let compositionActive = false;

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

          const cancelScheduledRestore = () => {
            if (restoreTimer != null) {
              clearTimeout(restoreTimer);
              restoreTimer = null;
            }
            if (rafId != null) {
              cancelAnimationFrame(rafId);
              rafId = null;
            }
          };

          const restoreNow = () => {
            cancelScheduledRestore();
            composing = false;
            if (!originalFlush) return;

            const savedFlush = originalFlush;
            const savedForceFlush = originalForceFlush!;
            domObserver.flush = savedFlush;
            domObserver.forceFlush = savedForceFlush;
            originalFlush = null;
            originalForceFlush = null;
            savedForceFlush();
            savedFlush();
          };

          const splitCurrentListItem = () => {
            const { state } = editorView;
            const { $from } = state.selection;
            for (let depth = $from.depth; depth > 0; depth--) {
              const nodeName = $from.node(depth).type.name;
              if (nodeName !== "listItem" && nodeName !== "taskItem") {
                continue;
              }

              const itemType = state.schema.nodes[nodeName];
              if (!itemType) return false;
              return splitListItem(itemType)(state, editorView.dispatch);
            }
            return false;
          };

          const onStart = () => {
            compositionActive = true;
            composing = true;
            cancelScheduledRestore();

            if (originalFlush) return;

            originalFlush = domObserver.flush.bind(domObserver);
            originalForceFlush = domObserver.forceFlush.bind(domObserver);
            domObserver.flush = noop;
            domObserver.forceFlush = noop;
          };

          const onEnd = () => {
            compositionActive = false;
            if (!originalFlush) return;

            cancelScheduledRestore();

            rafId = requestAnimationFrame(() => {
              rafId = null;
              restoreTimer = setTimeout(() => {
                restoreTimer = null;
                restoreNow();
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
              restoreNow();
            }, RAF_FALLBACK_MS);
          };

          /**
           * Capture-phase keydown interceptor.
           *
           * ProseMirror registers its keydown handler in the bubbling phase
           * (`initInput` → addEventListener without `capture`). A capture-
           * phase listener on the same element fires first, giving us the
           * chance to flush finalized IME DOM before ProseMirror's list
           * keymap sees stale state. We only stop propagation when the key is
           * still part of active composition or when we handle list splitting
           * ourselves after the flush.
           */
          const onKeyDown = (event: KeyboardEvent) => {
            if (event.key !== "Enter" && event.keyCode !== 13) {
              return;
            }

            if (compositionActive || event.isComposing) {
              event.stopPropagation();
              event.preventDefault();
              return;
            }

            if (composing) {
              restoreNow();
              if (splitCurrentListItem()) {
                event.stopPropagation();
                event.preventDefault();
              }
            }
          };

          dom.addEventListener("compositionstart", onStart, true);
          dom.addEventListener("compositionend", onEnd, true);
          dom.addEventListener("keydown", onKeyDown, true);

          return {
            destroy() {
              composing = false;
              compositionActive = false;
              dom.removeEventListener("compositionstart", onStart, true);
              dom.removeEventListener("compositionend", onEnd, true);
              dom.removeEventListener("keydown", onKeyDown, true);
              cancelScheduledRestore();
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
