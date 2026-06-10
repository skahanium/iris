import { Extension, type RawCommands } from "@tiptap/core";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import {
  Plugin,
  PluginKey,
  TextSelection,
  type EditorState,
} from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

export interface HeadingFoldState {
  collapsed: Set<number>;
  decorations: DecorationSet;
}

export interface FoldableHeadingBlock {
  pos: number;
  level: 1 | 2 | 3;
  text: string;
  collapsed: boolean;
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    headingFold: {
      toggleHeadingFold: (headingPos: number) => ReturnType;
    };
  }
}

export const headingFoldPluginKey = new PluginKey<HeadingFoldState>(
  "headingFold",
);

const MAX_FOLD_LEVEL = 3;

/** Beyond this size, skip fold gutter widgets unless a section is collapsed. */
const LARGE_DOC_FOLD_WIDGET_THRESHOLD = 12_000;

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

export function collectFoldableHeadingBlocks(
  doc: ProseMirrorNode,
  collapsed: Set<number> = new Set(),
): FoldableHeadingBlock[] {
  return topLevelBlocks(doc)
    .filter(({ node }) => isFoldableHeading(node))
    .map(({ pos, node }) => ({
      pos,
      level: node.attrs.level as 1 | 2 | 3,
      text: node.textContent.trim(),
      collapsed: collapsed.has(pos),
    }));
}

export function getHeadingFoldState(
  state: EditorState,
): HeadingFoldState | null {
  return headingFoldPluginKey.getState(state) ?? null;
}

/**
 * O(n) 单次遍历构建折叠装饰。
 *
 * 维护一个折叠区间状态：遇到折叠标题时标记区间开始，
 * 遇到同级或更高级标题时结束区间。区间内的节点添加隐藏装饰。
 */
function buildFoldDecorations(
  doc: ProseMirrorNode,
  collapsed: Set<number>,
): DecorationSet {
  if (
    collapsed.size === 0 &&
    doc.textContent.length > LARGE_DOC_FOLD_WIDGET_THRESHOLD
  ) {
    return DecorationSet.empty;
  }
  const decorations: Decoration[] = [];
  const blocks = topLevelBlocks(doc);

  let foldStartIdx = -1;
  let foldLevel = 0;

  for (let i = 0; i < blocks.length; i++) {
    const { pos, node } = blocks[i]!;

    if (isFoldableHeading(node)) {
      const level = node.attrs.level as number;

      // 遇到同级或更高级标题，结束当前折叠区间
      if (foldStartIdx >= 0 && level <= foldLevel) {
        foldStartIdx = -1;
      }

      if (collapsed.has(pos)) {
        foldStartIdx = i;
        foldLevel = level;
      }
    } else if (foldStartIdx >= 0) {
      // 折叠区间内的非标题节点，添加隐藏装饰
      decorations.push(
        Decoration.node(pos, pos + node.nodeSize, {
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

export const HeadingFoldExtension = Extension.create({
  name: "headingFold",

  addCommands(): Partial<RawCommands> {
    return {
      toggleHeadingFold:
        (headingPos: number) =>
        ({ state, dispatch }) => {
          if (dispatch) {
            dispatch(
              state.tr
                .setMeta(headingFoldPluginKey, { toggle: headingPos })
                .setMeta("addToHistory", false),
            );
          }
          return true;
        },
    } as Partial<RawCommands>;
  },

  addProseMirrorPlugins() {
    return [
      new Plugin<HeadingFoldState>({
        key: headingFoldPluginKey,
        state: {
          init: (_, state) => {
            const collapsed = new Set<number>();
            return {
              collapsed,
              decorations: buildFoldDecorations(state.doc, collapsed),
            };
          },
          apply(tr, value, _oldState, newState) {
            let { collapsed, decorations } = value;
            let rebuild = false;

            const meta = tr.getMeta(headingFoldPluginKey) as
              | { toggle: number }
              | undefined;

            if (tr.docChanged) {
              // 重新映射折叠位置
              const remapped = new Set<number>();
              for (const p of collapsed) {
                const mapped = tr.mapping.map(p, 1);
                if (mapped >= 0) remapped.add(mapped);
              }
              collapsed = remapped;

              if (collapsed.size > 0) {
                // 有折叠状态时需要全量重建（折叠区域内的编辑可能改变隐藏范围）
                rebuild = true;
              } else {
                // 无折叠状态，直接映射装饰位置（高效增量更新）
                decorations = decorations.map(tr.mapping, newState.doc);
              }
            }

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
              decorations = buildFoldDecorations(newState.doc, collapsed);
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
