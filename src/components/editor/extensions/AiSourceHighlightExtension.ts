import { Mark, mergeAttributes, type RawCommands } from "@tiptap/core";

/** 内联 AI 进行中时高亮原文选区 */
export const AiSourceHighlightExtension = Mark.create({
  name: "aiSourceHighlight",

  inclusive: false,

  parseHTML() {
    return [{ tag: "span[data-ai-source-highlight]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(
        { "data-ai-source-highlight": "", class: "iris-ai-source-highlight" },
        HTMLAttributes,
      ),
      0,
    ];
  },

  addCommands() {
    return {
      setAiSourceHighlight:
        (from: number, to: number) =>
        ({ tr, state, dispatch }) => {
          if (from >= to || !dispatch) return false;
          const mark = state.schema.marks.aiSourceHighlight;
          if (!mark) return false;
          tr.addMark(from, to, mark.create());
          dispatch(tr);
          return true;
        },
      clearAiSourceHighlight:
        (from?: number, to?: number) =>
        ({ tr, state, dispatch }) => {
          const mark = state.schema.marks.aiSourceHighlight;
          if (!mark || !dispatch) return false;
          if (from !== undefined && to !== undefined && from < to) {
            tr.removeMark(from, to, mark);
          } else {
            const { doc } = state;
            doc.descendants((node, pos) => {
              if (!node.isText) return;
              const has = node.marks.some((m) => m.type === mark);
              if (has) {
                tr.removeMark(pos, pos + node.nodeSize, mark);
              }
            });
          }
          dispatch(tr);
          return true;
        },
    } as Partial<RawCommands>;
  },
});

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    aiSourceHighlight: {
      setAiSourceHighlight: (from: number, to: number) => ReturnType;
      clearAiSourceHighlight: (from?: number, to?: number) => ReturnType;
    };
  }
}
