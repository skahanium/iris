import { mergeAttributes, Node, type RawCommands } from "@tiptap/core";
import type { Editor } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";

import { AiNodeView } from "../AiNodeView";

export type AiStreamStatus = "streaming" | "ready" | "error";

export interface AiStreamOptions {
  HTMLAttributes: Record<string, unknown>;
  canMutate?: () => boolean;
  onRetry?: (editor: Editor) => void;
  onDismiss?: (editor: Editor) => void;
  onAccept?: (editor: Editor) => void;
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    aiStream: {
      insertAiStreamBelowSelection: (payload: {
        originalText: string;
        action: string;
        sourceFrom: number;
        sourceTo: number;
      }) => ReturnType;
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
      dismissAiStream: () => ReturnType;
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

function readSourceRange(attrs: Record<string, unknown>): {
  sourceFrom: number;
  sourceTo: number;
} {
  const sourceFrom =
    typeof attrs.sourceFrom === "number" ? attrs.sourceFrom : 0;
  const sourceTo = typeof attrs.sourceTo === "number" ? attrs.sourceTo : 0;
  return { sourceFrom, sourceTo };
}

function clearHighlightForSource(
  tr: import("@tiptap/pm/state").Transaction,
  state: import("@tiptap/pm/state").EditorState,
  sourceFrom: number,
  sourceTo: number,
) {
  const mark = state.schema.marks.aiSourceHighlight;
  if (mark && sourceFrom < sourceTo) {
    tr.removeMark(sourceFrom, sourceTo, mark);
  }
}

/** AI 候选 UI 变更不进 undo 栈，避免 Cmd+Z 恢复候选框或重新进入流式态 */
function withoutHistory(tr: import("@tiptap/pm/state").Transaction) {
  return tr.setMeta("addToHistory", false);
}

export const AiStreamExtension = Node.create<AiStreamOptions>({
  name: "aiStream",
  group: "block",
  content: "inline*",
  atom: false,

  addOptions() {
    return {
      HTMLAttributes: {},
      canMutate: () => true,
      onRetry: undefined,
      onDismiss: undefined,
      onAccept: undefined,
    };
  },

  addAttributes() {
    return {
      status: { default: "streaming" },
      originalText: { default: "" },
      action: { default: "" },
      sourceFrom: { default: 0 },
      sourceTo: { default: 0 },
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

  addKeyboardShortcuts() {
    return {
      Escape: () => {
        if (!this.options.canMutate?.()) return false;
        if (!findAiStreamNode(this.editor.state)) return false;
        return this.editor.commands.dismissAiStream();
      },
      "Mod-Enter": () => {
        if (!this.options.canMutate?.()) return false;
        const found = findAiStreamNode(this.editor.state);
        if (!found) return false;
        if (found.attrs.status !== "ready") return false;
        return this.editor.commands.acceptAiStream();
      },
    };
  },

  addCommands(): Partial<RawCommands> {
    return {
      insertAiStreamBelowSelection:
        ({ originalText, action, sourceFrom, sourceTo }) =>
        ({ tr, state, dispatch }) => {
          if (!this.options.canMutate?.()) return false;
          if (!dispatch) return false;
          const $from = state.doc.resolve(sourceFrom);
          const $to = state.doc.resolve(sourceTo);
          const range = $from.blockRange($to);
          if (!range) return false;

          const insertPos = range.end;
          const node = state.schema.nodes.aiStream!.create(
            {
              status: "streaming",
              originalText,
              action,
              sourceFrom,
              sourceTo,
            },
            undefined,
          );
          tr.insert(insertPos, node);

          const mark = state.schema.marks.aiSourceHighlight;
          if (mark && sourceFrom < sourceTo) {
            tr.addMark(sourceFrom, sourceTo, mark.create());
          }

          dispatch(withoutHistory(tr));
          return true;
        },

      insertAiStreamForSelection:
        ({ originalText, action }) =>
        ({ state, commands }) => {
          const { from, to } = state.selection;
          return commands.insertAiStreamBelowSelection({
            originalText,
            action,
            sourceFrom: from,
            sourceTo: to,
          });
        },

      insertAiStreamAtCursor:
        ({ originalText, action }) =>
        ({ chain, state }) => {
          if (!this.options.canMutate?.()) return false;
          const { from } = state.selection;
          return chain()
            .insertContentAt(from, {
              type: this.name,
              attrs: {
                status: "streaming",
                originalText,
                action,
                sourceFrom: 0,
                sourceTo: 0,
              },
              content: [],
            })
            .command(({ tr }) => {
              withoutHistory(tr);
              return true;
            })
            .run();
        },

      updateAiStream:
        (content) =>
        ({ tr, state, dispatch }) => {
          if (!this.options.canMutate?.()) return false;
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
          dispatch(withoutHistory(tr));
          return true;
        },

      clearAiStreamContent:
        () =>
        ({ commands }) =>
          commands.updateAiStream(""),

      setAiStreamStatus:
        (status) =>
        ({ tr, state, dispatch }) => {
          if (!this.options.canMutate?.()) return false;
          const found = findAiStreamNode(state);
          if (!found || !dispatch) return false;
          tr.setNodeMarkup(found.pos, undefined, {
            ...found.attrs,
            status,
          });
          dispatch(withoutHistory(tr));
          return true;
        },

      acceptAiStream:
        () =>
        ({ state, dispatch, editor }) => {
          if (!this.options.canMutate?.()) return false;
          const found = findAiStreamNode(state);
          if (!found || !dispatch) return false;

          const text = found.text;
          const { sourceFrom, sourceTo } = readSourceRange(found.attrs);
          const streamPos = found.pos;
          const streamEnd = streamPos + found.nodeSize;

          // 1) 移除候选 UI（不进 undo 栈）
          const cleanupTr = state.tr;
          clearHighlightForSource(cleanupTr, state, sourceFrom, sourceTo);
          cleanupTr.delete(streamPos, streamEnd);
          dispatch(withoutHistory(cleanupTr));

          // 2) 下一帧应用可撤销的文本替换，避免在 command 内重入 dispatch
          queueMicrotask(() => {
            if (!this.options.canMutate?.()) return;
            if (sourceFrom > 0 && sourceTo > sourceFrom) {
              if (text) {
                editor.commands.insertContentAt(
                  { from: sourceFrom, to: sourceTo },
                  text,
                );
              } else {
                editor.commands.deleteRange({
                  from: sourceFrom,
                  to: sourceTo,
                });
              }
            } else if (text) {
              editor.commands.insertContentAt(streamPos, {
                type: "paragraph",
                content: [{ type: "text", text }],
              });
            }
            this.options.onAccept?.(editor);
          });

          return true;
        },

      rollbackAiStream:
        () =>
        ({ tr, state, dispatch }) => {
          if (!this.options.canMutate?.()) return false;
          const found = findAiStreamNode(state);
          if (!found || !dispatch) return false;
          const { sourceFrom, sourceTo } = readSourceRange(found.attrs);

          clearHighlightForSource(tr, state, sourceFrom, sourceTo);
          tr.delete(found.pos, found.pos + found.nodeSize);

          dispatch(withoutHistory(tr));
          return true;
        },

      dismissAiStream:
        () =>
        ({ editor, commands }) => {
          if (!this.options.canMutate?.()) return false;
          this.options.onDismiss?.(editor);
          return commands.rollbackAiStream();
        },

      removeAiStream:
        () =>
        ({ commands }) =>
          commands.rollbackAiStream(),
    } as Partial<RawCommands>;
  },
});
