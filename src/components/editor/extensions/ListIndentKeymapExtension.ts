import { Extension } from "@tiptap/core";
import { Fragment, type Node as ProseMirrorNode } from "@tiptap/pm/model";
import { Selection, TextSelection } from "@tiptap/pm/state";

type ListItemTypeName = "listItem" | "taskItem";

const MAX_IRIS_INDENT = 6;
const STOP_NODE_WALK = false;
const INDENTABLE_TEXT_BLOCKS = new Set(["paragraph", "heading"]);
const LIST_BLOCKS = new Set(["bulletList", "orderedList", "taskList"]);
const NUMBERED_PARAGRAPH_RE = /^\s*\d+(?:[.)]|\u3001)\s*(\S[\s\S]*)$/u;

interface TextBlockAtPos {
  node: ProseMirrorNode;
  pos: number;
  parent: ProseMirrorNode;
}

interface NumberedParagraphBlock {
  node: ProseMirrorNode;
  pos: number;
  match: NumberedParagraphMatch | null;
}

interface NumberedParagraphMatch {
  body: string;
  contentStartOffset: number;
}

function normalizeIrisIndent(value: unknown): number {
  const raw =
    typeof value === "number"
      ? value
      : typeof value === "string"
        ? Number(value)
        : 0;
  if (!Number.isFinite(raw)) return 0;
  return Math.min(MAX_IRIS_INDENT, Math.max(0, Math.trunc(raw)));
}

function parseNumberedParagraph(text: string): NumberedParagraphMatch | null {
  const match = NUMBERED_PARAGRAPH_RE.exec(text);
  if (!match) return null;

  const body = match[1]?.trimEnd();
  if (!body) return null;
  return {
    body,
    contentStartOffset: text.length - match[1]!.length,
  };
}

function contentAfterTextOffset(
  node: ProseMirrorNode,
  textOffset: number,
): Fragment {
  const children: ProseMirrorNode[] = [];
  let remaining = textOffset;

  node.forEach((child) => {
    if (remaining <= 0) {
      children.push(child);
      return;
    }

    const textLength = child.textContent.length;
    if (textLength <= remaining) {
      remaining -= textLength;
      return;
    }

    if (child.isText) {
      children.push(child.cut(remaining));
    } else {
      children.push(child);
    }
    remaining = 0;
  });

  return Fragment.fromArray(children);
}

export const ListIndentKeymapExtension = Extension.create({
  name: "listIndentKeymap",
  priority: 1000,

  addGlobalAttributes() {
    return [
      {
        types: ["paragraph", "heading"],
        attributes: {
          irisIndent: {
            default: 0,
            parseHTML: (element: HTMLElement) =>
              normalizeIrisIndent(element.getAttribute("data-iris-indent")),
            renderHTML: (attributes: Record<string, unknown>) => {
              const indent = normalizeIrisIndent(attributes.irisIndent);
              return indent > 0 ? { "data-iris-indent": String(indent) } : {};
            },
          },
        },
      },
    ];
  },

  addKeyboardShortcuts() {
    const currentListItemType = (): ListItemTypeName | null => {
      const { $from } = this.editor.state.selection;
      for (let depth = $from.depth; depth > 0; depth--) {
        const name = $from.node(depth).type.name;
        if (name === "taskItem" || name === "listItem") {
          return name;
        }
      }
      return null;
    };

    const currentEmptyListItemType = (): ListItemTypeName | null => {
      const { selection } = this.editor.state;
      if (!selection.empty) return null;

      const { $from } = selection;
      if (
        $from.parent.type.name !== "paragraph" ||
        $from.parentOffset !== 0 ||
        $from.parent.content.size !== 0
      ) {
        return null;
      }

      for (let depth = $from.depth - 1; depth > 0; depth--) {
        const node = $from.node(depth);
        const name = node.type.name;
        if (name !== "taskItem" && name !== "listItem") {
          continue;
        }

        if ($from.index(depth) === 0 && node.childCount === 1) {
          return name;
        }
        return null;
      }

      return null;
    };

    const currentTextBlock = (): TextBlockAtPos | null => {
      const { selection } = this.editor.state;
      const { $from } = selection;

      for (let depth = $from.depth; depth > 0; depth--) {
        const node = $from.node(depth);
        if (INDENTABLE_TEXT_BLOCKS.has(node.type.name)) {
          return {
            node,
            pos: $from.before(depth),
            parent: $from.node(depth - 1),
          };
        }
      }

      return null;
    };

    const deleteEmptyTopLevelParagraphAfterList = (): boolean => {
      const { state, view } = this.editor;
      const { selection } = state;
      if (!selection.empty || selection.$from.parentOffset !== 0) {
        return false;
      }

      const current = currentTextBlock();
      if (
        !current ||
        current.parent.type.name !== "doc" ||
        current.node.type.name !== "paragraph" ||
        current.node.content.size !== 0
      ) {
        return false;
      }

      const previous = state.doc.childBefore(current.pos);
      if (!previous.node || !LIST_BLOCKS.has(previous.node.type.name)) {
        return false;
      }

      const tr = state.tr.delete(
        current.pos,
        current.pos + current.node.nodeSize,
      );
      const selectionPos = tr.mapping.map(current.pos, -1);
      const nextSelection = Selection.findFrom(
        tr.doc.resolve(selectionPos),
        -1,
        true,
      );
      if (nextSelection) {
        tr.setSelection(nextSelection);
      }
      view.dispatch(tr.scrollIntoView());
      return true;
    };

    const adjustSelectedTextBlockIndent = (direction: 1 | -1) => {
      const { state, view } = this.editor;
      const { selection } = state;
      const blocks = new Map<number, ProseMirrorNode>();

      if (selection.empty) {
        const current = currentTextBlock();
        if (current?.parent.type.name === "doc") {
          blocks.set(current.pos, current.node);
        }
      } else {
        state.doc.nodesBetween(
          selection.from,
          selection.to,
          (node, pos, parent) => {
            if (
              parent?.type.name === "doc" &&
              INDENTABLE_TEXT_BLOCKS.has(node.type.name)
            ) {
              blocks.set(pos, node);
              return STOP_NODE_WALK;
            }
            return true;
          },
        );
      }

      if (blocks.size === 0) {
        const current = currentTextBlock();
        if (current?.parent.type.name === "doc") {
          blocks.set(current.pos, current.node);
        }
      }

      const tr = state.tr;
      [...blocks.entries()]
        .sort(([left], [right]) => left - right)
        .forEach(([pos, node]) => {
          const current = normalizeIrisIndent(node.attrs.irisIndent);
          const next = normalizeIrisIndent(current + direction);
          if (next !== current) {
            tr.setNodeMarkup(
              pos,
              undefined,
              { ...node.attrs, irisIndent: next },
              node.marks,
            );
          }
        });

      if (tr.steps.length > 0) {
        view.dispatch(tr.scrollIntoView());
      }
      return true;
    };

    const convertNumberedParagraphsToOrderedList = ():
      | { converted: false }
      | { converted: true; shouldSink: boolean } => {
      const { state, view } = this.editor;
      const current = currentTextBlock();
      if (
        !current ||
        current.parent.type.name !== "doc" ||
        current.node.type.name !== "paragraph"
      ) {
        return { converted: false };
      }

      const docBlocks: NumberedParagraphBlock[] = [];
      state.doc.forEach((node, offset) => {
        docBlocks.push({
          node,
          pos: offset,
          match:
            node.type.name === "paragraph"
              ? parseNumberedParagraph(node.textContent)
              : null,
        });
      });

      const currentIndex = docBlocks.findIndex(
        (block) => block.pos === current.pos,
      );
      if (currentIndex < 0 || docBlocks[currentIndex]?.match === null) {
        return { converted: false };
      }

      let startIndex = currentIndex;
      while (startIndex > 0 && docBlocks[startIndex - 1]?.match !== null) {
        startIndex--;
      }

      let endIndex = currentIndex;
      while (
        endIndex + 1 < docBlocks.length &&
        docBlocks[endIndex + 1]?.match !== null
      ) {
        endIndex++;
      }

      const orderedListType = state.schema.nodes.orderedList;
      const listItemType = state.schema.nodes.listItem;
      const paragraphType = state.schema.nodes.paragraph;
      if (!orderedListType || !listItemType || !paragraphType) {
        return { converted: false };
      }

      const numberedBlocks = docBlocks.slice(startIndex, endIndex + 1);
      const sourceListItems = numberedBlocks.map((block) => {
        const content = block.match
          ? contentAfterTextOffset(block.node, block.match.contentStartOffset)
          : Fragment.empty;
        const paragraph = paragraphType.create(
          null,
          content.size > 0 ? content : undefined,
        );
        return listItemType.create(null, paragraph);
      });
      const targetIndex = currentIndex - startIndex;
      const listItems =
        targetIndex > 0
          ? sourceListItems.reduce<ProseMirrorNode[]>((items, item, index) => {
              if (index === targetIndex) {
                const previous = items[items.length - 1];
                if (!previous) {
                  items.push(item);
                  return items;
                }
                const previousChildren: ProseMirrorNode[] = [];
                previous.forEach((child) => previousChildren.push(child));
                previousChildren.push(
                  orderedListType.create({ start: 1 }, [item]),
                );
                items[items.length - 1] = listItemType.create(
                  previous.attrs,
                  previousChildren,
                  previous.marks,
                );
                return items;
              }
              items.push(item);
              return items;
            }, [])
          : sourceListItems;
      const orderedList = orderedListType.create({ start: 1 }, listItems);

      const from = numberedBlocks[0]!.pos;
      const to =
        numberedBlocks[numberedBlocks.length - 1]!.pos +
        numberedBlocks[numberedBlocks.length - 1]!.node.nodeSize;
      const tr = state.tr.replaceWith(from, to, orderedList);

      let textIndex = -1;
      let selectionPos = from + orderedList.nodeSize - 1;
      tr.doc.nodesBetween(from, from + orderedList.nodeSize, (node, pos) => {
        if (node.isText) {
          textIndex++;
          if (textIndex === targetIndex) {
            selectionPos = pos + (node.text?.length ?? 0);
            return STOP_NODE_WALK;
          }
        }
        return true;
      });
      tr.setSelection(TextSelection.create(tr.doc, selectionPos));
      view.dispatch(tr.scrollIntoView());
      return { converted: true, shouldSink: false };
    };

    return {
      Backspace: () => {
        const listItemType = currentEmptyListItemType();
        if (listItemType) {
          return this.editor.commands.liftListItem(listItemType);
        }
        return deleteEmptyTopLevelParagraphAfterList();
      },
      Tab: () => {
        const listItemType = currentListItemType();
        if (listItemType) {
          this.editor.commands.sinkListItem(listItemType);
          return true;
        }
        const converted = convertNumberedParagraphsToOrderedList();
        if (converted.converted) {
          if (converted.shouldSink) {
            this.editor.commands.sinkListItem("listItem");
          }
          return true;
        }
        adjustSelectedTextBlockIndent(1);
        return true;
      },
      "Shift-Tab": () => {
        const listItemType = currentListItemType();
        if (listItemType) {
          this.editor.commands.liftListItem(listItemType);
          return true;
        }
        adjustSelectedTextBlockIndent(-1);
        return true;
      },
    };
  },
});
