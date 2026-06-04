import type { Editor } from "@tiptap/react";
import type { RefObject } from "react";

/** Wait until the TipTap editor is mounted (tab switch / close race). */
export async function waitForEditorRef(
  editorRef: RefObject<Editor | null>,
  maxMs = 1500,
): Promise<Editor | null> {
  const deadline = Date.now() + maxMs;
  while (Date.now() < deadline) {
    const ed = editorRef.current;
    if (ed && !ed.isDestroyed) {
      return ed;
    }
    await new Promise<void>((resolve) => {
      requestAnimationFrame(() => resolve());
    });
  }
  const ed = editorRef.current;
  return ed && !ed.isDestroyed ? ed : null;
}
