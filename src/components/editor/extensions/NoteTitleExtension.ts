import { Node, mergeAttributes } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

export const NOTE_TITLE_SOFT_LIMIT = 80;
export const NOTE_TITLE_HARD_LIMIT = 200;

export const noteTitlePluginKey = new PluginKey("noteTitleLimit");

function noteTitleTextLength(doc: {
  firstChild?: { type: { name: string }; textContent: string } | null;
}): number {
  const first = doc.firstChild;
  if (!first || first.type.name !== "noteTitle") return 0;
  return first.textContent.length;
}

export const NoteTitleExtension = Node.create({
  name: "noteTitle",

  content: "text*",
  marks: "",
  defining: true,
  isolating: true,
  group: "block",

  parseHTML() {
    return [{ tag: "h1.iris-doc-title" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "h1",
      mergeAttributes({ class: "iris-doc-title" }, HTMLAttributes),
      0,
    ];
  },

  addKeyboardShortcuts() {
    return {
      Enter: ({ editor }) => {
        if (!editor.isActive("noteTitle")) return false;
        const afterTitle = editor.state.doc.firstChild?.nodeSize ?? 1;
        return editor
          .chain()
          .setTextSelection(afterTitle)
          .focus(undefined, { scrollIntoView: true })
          .run();
      },
      "Mod-Enter": ({ editor }) => {
        if (!editor.isActive("noteTitle")) return false;
        editor.commands.focus("end");
        return true;
      },
      Backspace: ({ editor }) => {
        const { $from, empty } = editor.state.selection;
        if (!empty) return false;
        if ($from.parent.type.name !== "paragraph") return false;
        if ($from.parentOffset !== 0) return false;
        const index = $from.index($from.depth);
        if (index !== 1) return false;
        const nodeBefore = editor.state.doc.child(0);
        if (nodeBefore.type.name !== "noteTitle") return false;
        return true;
      },
    };
  },

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: noteTitlePluginKey,
        filterTransaction: (tr, state) => {
          if (!tr.docChanged) return true;
          const nextLen = noteTitleTextLength(tr.doc);
          if (nextLen <= NOTE_TITLE_HARD_LIMIT) return true;
          const prevLen = noteTitleTextLength(state.doc);
          return nextLen < prevLen;
        },
      }),
    ];
  },

  addNodeView() {
    return ({ node, getPos, editor }) => {
      const wrap = document.createElement("div");
      wrap.className = "iris-doc-title-wrap";

      const dom = document.createElement("h1");
      dom.className = "iris-doc-title";
      dom.setAttribute("data-placeholder", "无标题");
      if (node.textContent.length === 0) {
        dom.classList.add("is-empty");
      }

      const chip = document.createElement("span");
      chip.className = "iris-doc-title-count";
      chip.setAttribute("aria-hidden", "true");

      /* contentDOM 必须是 dom 的后代，否则输入会落到正文 */
      wrap.appendChild(dom);
      wrap.appendChild(chip);

      const updateChip = (text: string) => {
        const len = text.length;
        if (len <= NOTE_TITLE_SOFT_LIMIT) {
          chip.textContent = "";
          chip.classList.remove("is-warning");
          chip.removeAttribute("title");
          return;
        }
        chip.textContent = `${len}/${NOTE_TITLE_HARD_LIMIT}`;
        chip.classList.toggle("is-warning", len > NOTE_TITLE_HARD_LIMIT);
        chip.title =
          len > NOTE_TITLE_HARD_LIMIT
            ? "标题已达上限"
            : "标题较长可能影响 tab 显示";
      };

      updateChip(node.textContent);

      return {
        dom: wrap,
        contentDOM: dom,
        update: (updatedNode) => {
          if (updatedNode.type.name !== "noteTitle") return false;
          dom.classList.toggle(
            "is-empty",
            updatedNode.textContent.length === 0,
          );
          updateChip(updatedNode.textContent);
          return true;
        },
        ignoreMutation(mutation) {
          if (mutation.type === "selection") return true;
          const target = mutation.target;
          if (!(target instanceof globalThis.Node)) return false;
          // Only ignore the char-count chip; let ProseMirror sync title text in contentDOM.
          if (chip.contains(target)) return true;
          return false;
        },
        destroy: () => {
          void getPos;
          void editor;
        },
      };
    };
  },
});
