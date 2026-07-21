import { createDocument, type Content } from "@tiptap/core";
import type { Editor } from "@tiptap/react";
import type { Node as ProseMirrorNode, ParseOptions } from "@tiptap/pm/model";
import { EditorState, Selection, TextSelection } from "@tiptap/pm/state";

type ResetEditorBaselineSelection =
  | "start"
  | "preserve"
  | { from: number; to?: number };

interface ResetEditorBaselineOptions {
  parseOptions?: ParseOptions;
  selection?: ResetEditorBaselineSelection;
}

function clampTextSelectionPosition(doc: ProseMirrorNode, position: number) {
  const max = Math.max(1, doc.content.size - 1);
  const clamped = Math.min(Math.max(1, position), max);
  try {
    const $pos = doc.resolve(clamped);
    if ($pos.parent.inlineContent) {
      return clamped;
    }
    return TextSelection.near($pos, -1).from;
  } catch {
    return TextSelection.atStart(doc).from;
  }
}

function resolveBaselineSelection(
  editor: Editor,
  doc: ProseMirrorNode,
  selection: ResetEditorBaselineSelection = "start",
): Selection {
  if (selection === "start") {
    return TextSelection.atStart(doc);
  }

  const source =
    selection === "preserve"
      ? {
          from: editor.state.selection.anchor,
          to: editor.state.selection.head,
        }
      : selection;
  const from = clampTextSelectionPosition(doc, source.from);
  const to = clampTextSelectionPosition(doc, source.to ?? source.from);

  try {
    return TextSelection.create(doc, from, to);
  } catch {
    try {
      return TextSelection.near(doc.resolve(to), -1);
    } catch {
      return TextSelection.atStart(doc);
    }
  }
}

/**
 * Replace the editor document as a new content baseline.
 *
 * This is intentionally different from `editor.commands.setContent`: Tiptap's
 * command suppresses updates but still mutates through a normal transaction,
 * so ProseMirror history can treat a document load as undoable user input.
 */
export function resetEditorContentBaseline(
  editor: Editor,
  content: Content,
  options: ResetEditorBaselineOptions = {},
): void {
  if (editor.isDestroyed) return;

  const doc = createDocument(content, editor.schema, options.parseOptions);
  const nextState = EditorState.create({
    doc,
    plugins: editor.state.plugins,
    schema: editor.schema,
    selection: resolveBaselineSelection(editor, doc, options.selection),
  });

  editor.view.updateState(nextState);

  const refresh = editor.state.tr
    .setMeta("addToHistory", false)
    .setMeta("preventUpdate", true);
  editor.view.dispatch(refresh);
}
