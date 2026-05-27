import { mergeAttributes, Node } from "@tiptap/core";
import type { Editor } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";

import { InlineAiNodeView } from "../InlineAiNodeView";

// ─── Types ───────────────────────────────────────────────

export type InlineAiStatus = "pending" | "streaming" | "ready" | "error";

export type InlineAiAction =
  | "continue"
  | "rewrite"
  | "expand"
  | "simplify"
  | "cite"
  | "check";

export interface InlineAiOptions {
  HTMLAttributes: Record<string, unknown>;
  onExecute?: (
    editor: Editor,
    action: InlineAiAction,
    context: string
  ) => Promise<string>;
}

// ─── Command Declarations ────────────────────────────────

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    inlineAi: {
      /**
       * Insert inline AI for selected text with specified action.
       */
      insertInlineAi: (payload: {
        action: InlineAiAction;
        context?: string;
      }) => ReturnType;

      /**
       * Update the streaming content of the active inline AI node.
       */
      updateInlineAiContent: (content: string) => ReturnType;

      /**
       * Set the status of the active inline AI node.
       */
      setInlineAiStatus: (status: InlineAiStatus) => ReturnType;

      /**
       * Accept the AI-generated content, replacing the node.
       */
      acceptInlineAi: () => ReturnType;

      /**
       * Reject the AI-generated content and remove the node.
       */
      rejectInlineAi: () => ReturnType;

      /**
       * Retry the inline AI operation.
       */
      retryInlineAi: () => ReturnType;
    };
  }
}

// ─── Helper Functions ────────────────────────────────────

export function findInlineAiNode(state: {
  doc: {
    descendants: (
      f: (
        node: {
          type: { name: string };
          nodeSize: number;
          textContent: string;
          attrs: Record<string, unknown>;
        },
        pos: number
      ) => boolean | void
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
    if (node.type.name === "inlineAi" && found === null) {
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

// ─── Action Labels ───────────────────────────────────────

export const INLINE_AI_ACTION_LABELS: Record<InlineAiAction, string> = {
  continue: "续写",
  rewrite: "改写",
  expand: "扩写",
  simplify: "简化",
  cite: "引用",
  check: "检查",
};

// ─── Extension ───────────────────────────────────────────

export const InlineAiExtension = Node.create<InlineAiOptions>({
  name: "inlineAi",
  group: "block",
  content: "inline*",
  atom: false,

  addOptions() {
    return {
      HTMLAttributes: {},
      onExecute: undefined,
    };
  },

  addAttributes() {
    return {
      status: {
        default: "pending",
        parseHTML: (element) => element.getAttribute("data-status") ?? "pending",
        renderHTML: (attributes) => ({
          "data-status": attributes.status,
        }),
      },
      action: {
        default: "continue",
        parseHTML: (element) => element.getAttribute("data-action") ?? "continue",
        renderHTML: (attributes) => ({
          "data-action": attributes.action,
        }),
      },
      context: {
        default: "",
        parseHTML: (element) => element.getAttribute("data-context") ?? "",
        renderHTML: (attributes) => ({
          "data-context": attributes.context,
        }),
      },
      originalText: {
        default: "",
        parseHTML: (element) => element.getAttribute("data-original") ?? "",
        renderHTML: (attributes) => ({
          "data-original": attributes.originalText,
        }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-type="inline-ai"]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes({ "data-type": "inline-ai" }, HTMLAttributes),
      0,
    ];
  },

  addNodeView() {
    return ReactNodeViewRenderer(InlineAiNodeView);
  },

  addCommands() {
    return {
      insertInlineAi:
        ({ action, context }) =>
        ({ commands, editor }) => {
          // Don't insert in title
          if (editor.isActive("noteTitle")) return false;

          // Get selected text as context if not provided
          const { from, to } = editor.state.selection;
          const selectedText = editor.state.doc.textBetween(from, to, " ");
          const aiContext = context ?? selectedText;

          return commands.insertContent({
            type: this.name,
            attrs: {
              status: "pending",
              action,
              context: aiContext,
              originalText: selectedText,
            },
            content: [],
          });
        },

      updateInlineAiContent:
        (content) =>
        ({ tr, state, dispatch }) => {
          const found = findInlineAiNode(state);
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

      setInlineAiStatus:
        (status) =>
        ({ tr, state, dispatch }) => {
          const found = findInlineAiNode(state);
          if (!found || !dispatch) return false;

          tr.setNodeMarkup(found.pos, undefined, {
            ...found.attrs,
            status,
          });

          dispatch(tr);
          return true;
        },

      acceptInlineAi:
        () =>
        ({ tr, state, dispatch }) => {
          const found = findInlineAiNode(state);
          if (!found || !dispatch) return false;

          const text = found.text;

          // Replace the AI node with a paragraph containing the generated text
          tr.replaceWith(
            found.pos,
            found.pos + found.nodeSize,
            state.schema.nodes.paragraph!.create(
              {},
              text ? state.schema.text(text) : undefined
            )
          );

          dispatch(tr);
          return true;
        },

      rejectInlineAi:
        () =>
        ({ tr, state, dispatch }) => {
          const found = findInlineAiNode(state);
          if (!found || !dispatch) return false;

          const originalText =
            typeof found.attrs.originalText === "string"
              ? found.attrs.originalText
              : "";

          if (!originalText) {
            // No original text, just remove the node
            tr.delete(found.pos, found.pos + found.nodeSize);
          } else {
            // Restore original text
            tr.replaceWith(
              found.pos,
              found.pos + found.nodeSize,
              state.schema.nodes.paragraph!.create(
                {},
                state.schema.text(originalText)
              )
            );
          }

          dispatch(tr);
          return true;
        },

      retryInlineAi:
        () =>
        ({ commands }) => {
          // Reset status to pending to trigger a retry
          commands.setInlineAiStatus("pending");
          return true;
        },
    };
  },

  addKeyboardShortcuts() {
    return {
      // Accept with Ctrl/Cmd + Enter
      "Mod-Enter": () => {
        if (this.editor.isActive("inlineAi")) {
          return this.editor.commands.acceptInlineAi();
        }
        return false;
      },

      // Reject with Escape
      Escape: () => {
        if (this.editor.isActive("inlineAi")) {
          return this.editor.commands.rejectInlineAi();
        }
        return false;
      },
    };
  },
});
