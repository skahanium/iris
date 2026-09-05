import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

const ZWSP = "\u200b";

function headingLooksEmpty(node: { textContent: string }): boolean {
  return node.textContent.replaceAll(ZWSP, "").trim().length === 0;
}

/**
 * Protects heading nodes from WebKit contenteditable block-wrapping.
 *
 * When a user types the first character into an empty heading, WebKit can wrap
 * the typed text in a `<p>` inside the `<h1>`. ProseMirror then normalizes this
 * into an empty heading followed by a paragraph, which makes the text look like
 * body text and removes the heading from the outline.
 *
 * This plugin watches transactions for that exact pattern: an empty heading
 * that gains a non-empty paragraph immediately after it in the same
 * transaction. When detected, it merges the paragraph text back into the
 * heading and removes the stray paragraph.
 */
export const headingDomGuardPluginKey = new PluginKey("headingDomGuard");

export const HeadingDomGuardExtension = Extension.create({
  name: "headingDomGuard",

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: headingDomGuardPluginKey,
        appendTransaction(_transactions, oldState, newState) {
          const tr = newState.tr;
          let changed = false;

          newState.doc.forEach((node, pos) => {
            if (changed) return;
            if (node.type.name !== "heading" || !headingLooksEmpty(node)) {
              return;
            }

            const paragraphStart = pos + node.nodeSize;
            const after = newState.doc.nodeAt(paragraphStart);
            if (
              !after ||
              after.type.name !== "paragraph" ||
              after.textContent.trim().length === 0
            ) {
              return;
            }

            // Only merge when the paragraph did not exist before this
            // transaction. A pre-existing empty heading + paragraph is a
            // legitimate document and must not be rewritten.
            // Use the old-state heading node size because the IME placeholder
            // can change the new-state heading size and shift positions.
            const oldHeading = oldState.doc.nodeAt(pos);
            const oldParagraphStart = oldHeading
              ? pos + oldHeading.nodeSize
              : -1;
            const oldAfter =
              oldParagraphStart >= 0
                ? oldState.doc.nodeAt(oldParagraphStart)
                : null;
            if (oldAfter && oldAfter.type.name === "paragraph") {
              return;
            }

            const text = after.textContent;
            tr.delete(paragraphStart, paragraphStart + after.nodeSize);
            // Remove the IME placeholder before inserting the real text.
            if (node.textContent.startsWith(ZWSP)) {
              tr.delete(pos + 1, pos + 2);
            }
            tr.insertText(text, pos + 1);
            changed = true;
          });

          return changed ? tr : null;
        },
      }),
    ];
  },
});
