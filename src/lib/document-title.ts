import { resolveNoteDisplayTitle } from "@/lib/note-display";

export { displayTitleFromMarkdown } from "@/lib/note-title";

/** Resolve `files.title` for a path; never exposes internal `untitled-*` stems. */
export async function resolveDocumentTitle(
  path: string,
  _hint?: string,
): Promise<string> {
  return resolveNoteDisplayTitle({ path });
}
