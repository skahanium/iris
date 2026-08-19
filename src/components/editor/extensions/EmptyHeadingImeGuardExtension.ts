import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

/**
 * Keeps empty headings as IME editing hosts.
 *
 * WebKit can lose an empty `<h1>/<h2>/<h3>` during Chinese/Japanese IME
 * composition: the composed text ends up in a paragraph and the heading node
 * disappears. A common workaround is to place an invisible zero-width space
 * (U+200B) inside the heading before composition starts, then remove it after
 * composition ends.
 *
 * This extension only touches headings during composition, so normal empty
 * heading semantics and the `HeadingDomGuardExtension` merge path are not
 * affected.
 */
export const emptyHeadingImeGuardPluginKey = new PluginKey(
  "emptyHeadingImeGuard",
);

const ZWSP = "\u200b";

export const EmptyHeadingImeGuardExtension = Extension.create({
  name: "emptyHeadingImeGuard",

  addProseMirrorPlugins() {
    let compositionHeadingPos: number | null = null;

    return [
      new Plugin({
        key: emptyHeadingImeGuardPluginKey,
        props: {
          handleDOMEvents: {
            compositionstart(view) {
              const { $from } = view.state.selection;
              const parent = $from.parent;
              if (parent.type.name !== "heading" || parent.content.size !== 0) {
                return false;
              }
              const insertPos = $from.pos;
              view.dispatch(view.state.tr.insertText(ZWSP, insertPos));
              compositionHeadingPos = insertPos;
              return false;
            },
            compositionend(view) {
              if (compositionHeadingPos == null) return false;
              const pos = compositionHeadingPos;
              compositionHeadingPos = null;
              const $pos = view.state.doc.resolve(pos);
              const parent = $pos.parent;
              if (
                parent.type.name !== "heading" ||
                !parent.textContent.startsWith(ZWSP)
              ) {
                return false;
              }
              view.dispatch(
                view.state.tr.delete($pos.start(), $pos.start() + 1),
              );
              return false;
            },
          },
        },
      }),
    ];
  },
});
