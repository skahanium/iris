import { createDocument, type Content } from "@tiptap/core";
import type { Editor } from "@tiptap/react";
import type { ParseOptions } from "@tiptap/pm/model";
import { EditorState, TextSelection } from "@tiptap/pm/state";

interface ResetEditorBaselineOptions {
  parseOptions?: ParseOptions;
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
    selection: TextSelection.atStart(doc),
  });

  editor.view.updateState(nextState);

  const refresh = editor.state.tr
    .setMeta("addToHistory", false)
    .setMeta("preventUpdate", true);
  editor.view.dispatch(refresh);
}
