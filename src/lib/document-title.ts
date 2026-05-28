import { resolveNoteDisplayTitle } from "@/lib/note-display";
import { fileList } from "@/lib/ipc";

export { displayTitleFromMarkdown } from "@/lib/note-title";

/** Resolve `files.title` for a path; never exposes internal `untitled-*` stems. */
export async function resolveDocumentTitle(
  path: string,
  hint?: string,
): Promise<string> {
  if (hint?.trim()) {
    return resolveNoteDisplayTitle({ path, title: hint });
  }
  const list = await fileList();
  const hit = list.find((f) => f.path === path);
  return resolveNoteDisplayTitle({
    path,
    title: hit?.title,
  });
}
