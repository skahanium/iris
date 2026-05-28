import { Extension, type Editor } from "@tiptap/core";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import { Plugin, PluginKey, TextSelection } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

interface HeadingFoldState {
  collapsed: Set<number>;
  decorations: DecorationSet;
}

export const headingFoldPluginKey = new PluginKey<HeadingFoldState>(
  "headingFold",
);

const MAX_FOLD_LEVEL = 3;

function isFoldableHeading(node: ProseMirrorNode): boolean {
  return (
    node.type.name === "heading" &&
    typeof node.attrs.level === "number" &&
    node.attrs.level <= MAX_FOLD_LEVEL
  );
}

function topLevelBlocks(
  doc: ProseMirrorNode,
): { pos: number; node: ProseMirrorNode }[] {
  const blocks: { pos: number; node: ProseMirrorNode }[] = [];
  doc.forEach((child, offset) => {
    if (child.isBlock) {
      blocks.push({ pos: offset, node: child });
    }
  });
  return blocks;
}

function buildFoldDecorations(
  doc: ProseMirrorNode,
  collapsed: Set<number>,
  onToggle: (headingPos: number) => void,
): DecorationSet {
  const decorations: Decoration[] = [];
  const blocks = topLevelBlocks(doc);

  for (let i = 0; i < blocks.length; i++) {
    const { pos, node } = blocks[i]!;
    if (!isFoldableHeading(node)) continue;

    const level = node.attrs.level as number;
    const isCollapsed = collapsed.has(pos);

    decorations.push(
      Decoration.widget(
        pos + 1,
        () => {
          const wrap = document.createElement("span");
          wrap.className = "iris-heading-fold-gutter";
          wrap.setAttribute("contenteditable", "false");

          const btn = document.createElement("button");
          btn.type = "button";
          btn.className = "iris-heading-fold-btn";
          btn.setAttribute("aria-label", isCollapsed ? "展开章节" : "折叠章节");
          btn.textContent = isCollapsed ? "▸" : "▾";
          btn.addEventListener("mousedown", (event) => {
            event.preventDefault();
            event.stopPropagation();
            onToggle(pos);
          });
          wrap.appendChild(btn);
          return wrap;
        },
        {
          side: -1,
          key: `fold-${pos}-${isCollapsed ? "c" : "e"}`,
          ignoreSelection: true,
        },
      ),
    );

    if (!isCollapsed) continue;

    for (let j = i + 1; j < blocks.length; j++) {
      const next = blocks[j]!;
      if (
        isFoldableHeading(next.node) &&
        (next.node.attrs.level as number) <= level
      ) {
        break;
      }
      decorations.push(
        Decoration.node(next.pos, next.pos + next.node.nodeSize, {
          class: "iris-fold-hidden",
        }),
      );
    }
  }

  return DecorationSet.create(doc, decorations);
}

function nextVisiblePos(
  doc: ProseMirrorNode,
  collapsed: Set<number>,
  from: number,
  dir: 1 | -1,
): number | null {
  const blocks = topLevelBlocks(doc);
  const hidden = new Set<number>();

  for (let i = 0; i < blocks.length; i++) {
    const { pos, node } = blocks[i]!;
    if (!isFoldableHeading(node) || !collapsed.has(pos)) continue;
    const level = node.attrs.level as number;
    for (let j = i + 1; j < blocks.length; j++) {
      const next = blocks[j]!;
      if (
        isFoldableHeading(next.node) &&
        (next.node.attrs.level as number) <= level
      ) {
        break;
      }
      hidden.add(next.pos);
    }
  }

  let pos = from;
  const step = dir;
  while (pos >= 0 && pos <= doc.content.size) {
    const inHidden = [...hidden].some((start) => {
      const block = blocks.find((b) => b.pos === start);
      if (!block) return false;
      const end = start + block.node.nodeSize;
      return pos > start && pos < end;
    });
    if (!inHidden && pos >= 0 && pos <= doc.content.size) {
      return pos;
    }
    pos += step;
    if (pos < 0 || pos > doc.content.size) return null;
  }
  return null;
}

function focusFirstBodyAfterHeading(editor: Editor): boolean {
  const { state } = editor;
  const { $from } = state.selection;
  if ($from.parent.type.name !== "heading") return false;

  const docDepth = 1;
  const index = $from.index(docDepth);
  let insertPos = $from.after(docDepth);

  for (let i = index + 1; i < state.doc.childCount; i++) {
    const node = state.doc.child(i);
    if (node.type.name === "paragraph" && node.content.size === 0) {
      insertPos += node.nodeSize;
      continue;
    }
    if (node.type.name === "paragraph") {
      return editor
        .chain()
        .focus()
        .setTextSelection(insertPos + 1)
        .run();
    }
    break;
  }

  return editor
    .chain()
    .insertContentAt(insertPos, { type: "paragraph" })
    .focus()
    .setTextSelection(insertPos + 1)
    .run();
}

export const HeadingFoldExtension = Extension.create({
  name: "headingFold",

  addKeyboardShortcuts() {
    return {
      Enter: ({ editor }) => {
        if (!editor.isActive("heading")) return false;
        return focusFirstBodyAfterHeading(editor);
      },
    };
  },

  addProseMirrorPlugins() {
    const editor = this.editor;

    const onToggle = (headingPos: number) => {
      const { state, view } = editor;
      view.dispatch(
        state.tr
          .setMeta(headingFoldPluginKey, { toggle: headingPos })
          .setMeta("addToHistory", false),
      );
    };

    return [
      new Plugin<HeadingFoldState>({
        key: headingFoldPluginKey,
        state: {
          init: (_, state) => {
            const collapsed = new Set<number>();
            return {
              collapsed,
              decorations: buildFoldDecorations(state.doc, collapsed, onToggle),
            };
          },
          apply(tr, value, _oldState, newState) {
            let { collapsed, decorations } = value;
            let rebuild = false;

            if (tr.docChanged) {
              const remapped = new Set<number>();
              for (const p of collapsed) {
                const mapped = tr.mapping.map(p, 1);
                if (mapped >= 0) remapped.add(mapped);
              }
              collapsed = remapped;
              rebuild = true;
            }

            const meta = tr.getMeta(headingFoldPluginKey) as
              | { toggle: number }
              | undefined;
            if (meta?.toggle != null) {
              const next = new Set(collapsed);
              if (next.has(meta.toggle)) {
                next.delete(meta.toggle);
              } else {
                next.add(meta.toggle);
              }
              collapsed = next;
              rebuild = true;
            }

            if (rebuild) {
              decorations = buildFoldDecorations(
                newState.doc,
                collapsed,
                onToggle,
              );
            } else {
              decorations = decorations.map(tr.mapping, newState.doc);
            }

            return { collapsed, decorations };
          },
        },
        props: {
          decorations(state) {
            return (
              headingFoldPluginKey.getState(state)?.decorations ??
              DecorationSet.empty
            );
          },
          handleKeyDown(view, event) {
            if (event.key !== "ArrowDown" && event.key !== "ArrowUp") {
              return false;
            }
            const foldState = headingFoldPluginKey.getState(view.state);
            if (!foldState || foldState.collapsed.size === 0) return false;

            const { selection } = view.state;
            const dir = event.key === "ArrowDown" ? 1 : -1;
            const next = nextVisiblePos(
              view.state.doc,
              foldState.collapsed,
              selection.head + dir,
              dir,
            );
            if (next == null || next === selection.head) return false;
            const tr = view.state.tr.setSelection(
              TextSelection.create(view.state.doc, next),
            );
            view.dispatch(tr);
            return true;
          },
        },
      }),
    ];
  },
});
