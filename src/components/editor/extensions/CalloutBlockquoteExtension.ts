import Blockquote from "@tiptap/extension-blockquote";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import { Plugin } from "@tiptap/pm/state";

/**
 * Blockquote with optional Obsidian callout metadata (`data-callout-type`).
 * `calloutOriginalRaw` keeps the source markdown until the user edits the callout.
 */
export const CalloutBlockquoteExtension = Blockquote.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      calloutType: {
        default: null as string | null,
        parseHTML: (element) => element.getAttribute("data-callout-type"),
        renderHTML: (attributes) => {
          const type = attributes.calloutType as string | null;
          if (!type?.trim()) {
            return {};
          }
          return { "data-callout-type": type.trim() };
        },
      },
      calloutOriginalRaw: {
        default: null as string | null,
        parseHTML: (element) =>
          element.getAttribute("data-callout-original-raw"),
        renderHTML: (attributes) => {
          const raw = attributes.calloutOriginalRaw as string | null;
          if (!raw) {
            return {};
          }
          return { "data-callout-original-raw": raw };
        },
      },
    };
  },

  addProseMirrorPlugins() {
    const nodeName = this.name;
    const parentPlugins = this.parent?.() ?? [];
    return [
      ...parentPlugins,
      new Plugin({
        appendTransaction(transactions, oldState, newState) {
          if (!transactions.some((t) => t.docChanged)) {
            return null;
          }

          const oldCallouts: ProseMirrorNode[] = [];
          oldState.doc.descendants((node) => {
            if (
              node.type.name === nodeName &&
              node.attrs.calloutType &&
              node.attrs.calloutOriginalRaw
            ) {
              oldCallouts.push(node);
            }
          });

          let calloutIndex = 0;
          let tr = null;

          newState.doc.descendants((node, pos) => {
            if (
              node.type.name !== nodeName ||
              !node.attrs.calloutType ||
              !node.attrs.calloutOriginalRaw
            ) {
              return;
            }
            const oldNode = oldCallouts[calloutIndex];
            calloutIndex += 1;
            if (oldNode && !oldNode.eq(node)) {
              tr ??= newState.tr;
              tr.setNodeAttribute(pos, "calloutOriginalRaw", null);
            }
          });

          return tr;
        },
      }),
    ];
  },
});
