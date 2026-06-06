import { fileDiscard, fileRead } from "@/lib/ipc";
import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";

/**
 * Whether a note at `path` has no user-authored content.
 * Uses in-memory markdown when `path` is the active editor note.
 */
export async function isPathSubstantivelyEmpty(
  path: string,
  activePath: string | null,
  activeMarkdown: string,
): Promise<boolean> {
  if (path === activePath) {
    return isNoteSubstantivelyEmpty(activeMarkdown);
  }
  const { content } = await fileRead(path);
  return isNoteSubstantivelyEmpty(content);
}

/** Permanently remove a blank note; returns true if discarded. */
export async function discardEmptyNoteIfNeeded(
  path: string,
  activePath: string | null,
  activeMarkdown: string,
): Promise<boolean> {
  if (!(await isPathSubstantivelyEmpty(path, activePath, activeMarkdown))) {
    return false;
  }
  await fileDiscard(path);
  return true;
}
