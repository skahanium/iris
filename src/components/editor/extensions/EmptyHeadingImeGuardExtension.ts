import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

/**
 * Keeps empty headings as IME editing hosts.
 *
 * WebKit can lose an empty `<h1>/<h2>/<h3>` during Chinese/Japanese IME
 * composition: the composed text ends up in a paragraph and the heading node
 * disappears. A common workaround is to keep an invisible zero-width space
 * (U+200B) inside empty headings so the browser always has a text node to
 * attach composition to.
 *
 * This extension maintains that placeholder **outside** of composition
 * transactions: it never dispatches while IME is active, so it cannot disrupt
 * the composition session. Once real text is present, the placeholder is
 * removed.
 */
export const emptyHeadingImeGuardPluginKey = new PluginKey(
  "emptyHeadingImeGuard",
);

const ZWSP = "\u200b";

function headingHasRealContent(node: { textContent: string }): boolean {
  return node.textContent.replaceAll(ZWSP, "").trim().length > 0;
}

export const EmptyHeadingImeGuardExtension = Extension.create({
  name: "emptyHeadingImeGuard",

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: emptyHeadingImeGuardPluginKey,
        appendTransaction(_transactions, _oldState, newState) {
          const tr = newState.tr;
          let changed = false;

          newState.doc.descendants((node, pos) => {
            if (changed) return;
            if (node.type.name !== "heading") return;

            const text = node.textContent;
            if (!headingHasRealContent(node)) {
              // Keep the placeholder in empty/whitespace-only headings.
              if (text !== ZWSP) {
                // Remove any existing content first, then insert a single ZWSP.
                if (node.content.size > 0) {
                  tr.delete(pos + 1, pos + 1 + node.content.size);
                }
                tr.insertText(ZWSP, pos + 1);
                changed = true;
              }
              return;
            }

            // Real text exists: drop a leading placeholder if one remains.
            if (text.startsWith(ZWSP)) {
              tr.delete(pos + 1, pos + 2);
              changed = true;
            }
          });

          return changed ? tr : null;
        },
      }),
    ];
  },
});
