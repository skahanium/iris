import type { Editor } from "@tiptap/react";

export function editorHasActiveAiStream(editor: Editor): boolean {
  let streaming = false;
  editor.state.doc.descendants((node) => {
    if (streaming) return false;
    if (node.type.name !== "aiStream") return;
    const status = (node.attrs as { status?: string }).status;
    if (status === "streaming") streaming = true;
  });
  return streaming;
}
