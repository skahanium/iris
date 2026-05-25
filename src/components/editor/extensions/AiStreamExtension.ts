import { mergeAttributes, Node } from "@tiptap/core";
import type { Editor } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";

import { AiNodeView } from "../AiNodeView";

export type AiStreamStatus = "streaming" | "ready" | "error";

export interface AiStreamOptions {
  HTMLAttributes: Record<string, unknown>;
  onRetry?: (editor: Editor) => void;
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    aiStream: {
      insertAiStreamForSelection: (payload: {
        originalText: string;
        action: string;
      }) => ReturnType;
      insertAiStreamAtCursor: (payload: {
        originalText: string;
        action: string;
      }) => ReturnType;
      updateAiStream: (content: string) => ReturnType;
      clearAiStreamContent: () => ReturnType;
      setAiStreamStatus: (status: AiStreamStatus) => ReturnType;
      acceptAiStream: () => ReturnType;
      rollbackAiStream: () => ReturnType;
      removeAiStream: () => ReturnType;
    };
  }
}

export function findAiStreamNode(state: {
  doc: {
    descendants: (
      f: (
        node: {
          type: { name: string };
          nodeSize: number;
          textContent: string;
          attrs: Record<string, unknown>;
        },
        pos: number,
      ) => boolean | void,
    ) => void;
  };
}): {
  pos: number;
  nodeSize: number;
  text: string;
  attrs: Record<string, unknown>;
} | null {
  let found: {
    pos: number;
    nodeSize: number;
    text: string;
    attrs: Record<string, unknown>;
  } | null = null;
  state.doc.descendants((node, pos) => {
    if (node.type.name === "aiStream" && found === null) {
      found = {
        pos,
        nodeSize: node.nodeSize,
        text: node.textContent,
        attrs: node.attrs as Record<string, unknown>,
      };
    }
  });
  return found;
}

export const AiStreamExtension = Node.create<AiStreamOptions>({
  name: "aiStream",
  group: "block",
  content: "inline*",
  atom: false,

  addOptions() {
    return {
      HTMLAttributes: {},
      onRetry: undefined,
    };
  },

  addAttributes() {
    return {
      status: { default: "streaming" },
      originalText: { default: "" },
      action: { default: "" },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-type="ai-stream"]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes({ "data-type": "ai-stream" }, HTMLAttributes),
      0,
    ];
  },

  addNodeView() {
    return ReactNodeViewRenderer(AiNodeView);
  },

  addCommands() {
    return {
      insertAiStreamForSelection:
        ({ originalText, action }) =>
        ({ commands }) =>
          commands.insertContent({
            type: this.name,
            attrs: {
              status: "streaming",
              originalText,
              action,
            },
            content: [],
          }),

      insertAiStreamAtCursor:
        ({ originalText, action }) =>
        ({ chain, state }) => {
          const { from } = state.selection;
          return chain()
            .insertContentAt(from, {
              type: this.name,
              attrs: {
                status: "streaming",
                originalText,
                action,
              },
              content: [],
            })
            .run();
        },

      updateAiStream:
        (content) =>
        ({ tr, state, dispatch }) => {
          const found = findAiStreamNode(state);
          if (!found || !dispatch) return false;
          const node = state.doc.nodeAt(found.pos);
          if (!node) return false;
          const from = found.pos + 1;
          const to = found.pos + node.nodeSize - 1;
          if (!content) {
            if (from < to) tr.delete(from, to);
          } else if (from >= to) {
            tr.insert(from, state.schema.text(content));
          } else {
            tr.replaceWith(from, to, state.schema.text(content));
          }
          dispatch(tr);
          return true;
        },

      clearAiStreamContent:
        () =>
        ({ commands }) =>
          commands.updateAiStream(""),

      setAiStreamStatus:
        (status) =>
        ({ tr, state, dispatch }) => {
          const found = findAiStreamNode(state);
          if (!found || !dispatch) return false;
          tr.setNodeMarkup(found.pos, undefined, {
            ...found.attrs,
            status,
          });
          dispatch(tr);
          return true;
        },

      acceptAiStream:
        () =>
        ({ tr, state, dispatch }) => {
          const found = findAiStreamNode(state);
          if (!found || !dispatch) return false;
          const text = found.text;
          tr.replaceWith(
            found.pos,
            found.pos + found.nodeSize,
            state.schema.nodes.paragraph!.create(
              {},
              text ? state.schema.text(text) : undefined,
            ),
          );
          dispatch(tr);
          return true;
        },

      rollbackAiStream:
        () =>
        ({ tr, state, dispatch }) => {
          const found = findAiStreamNode(state);
          if (!found || !dispatch) return false;
          const originalText =
            typeof found.attrs.originalText === "string"
              ? found.attrs.originalText
              : "";
          if (!originalText) {
            tr.delete(found.pos, found.pos + found.nodeSize);
          } else {
            tr.replaceWith(
              found.pos,
              found.pos + found.nodeSize,
              state.schema.nodes.paragraph!.create(
                {},
                state.schema.text(originalText),
              ),
            );
          }
          dispatch(tr);
          return true;
        },

      removeAiStream:
        () =>
        ({ commands }) =>
          commands.rollbackAiStream(),
    };
  },
});
