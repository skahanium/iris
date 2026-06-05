import type { Editor } from "@tiptap/core";

/** True when `text` appears in a `listItem` nested under another `listItem`. */
export function hasNestedListItem(editor: Editor, text: string): boolean {
  let found = false;
  editor.state.doc.descendants((node, pos) => {
    if (node.type.name !== "listItem" || !node.textContent.includes(text)) {
      return;
    }
    const $pos = editor.state.doc.resolve(pos);
    for (let depth = $pos.depth; depth > 0; depth -= 1) {
      if ($pos.node(depth).type.name === "listItem") {
        found = true;
        return false;
      }
    }
  });
  return found;
}

/** True when `text` is in a top-level `listItem` (not nested under another list item). */
export function hasTopLevelListItem(editor: Editor, text: string): boolean {
  let found = false;
  editor.state.doc.descendants((node, pos) => {
    if (node.type.name !== "listItem" || !node.textContent.includes(text)) {
      return;
    }
    const $pos = editor.state.doc.resolve(pos);
    for (let depth = $pos.depth; depth > 0; depth -= 1) {
      if ($pos.node(depth).type.name === "listItem") {
        return;
      }
    }
    found = true;
    return false;
  });
  return found;
}
