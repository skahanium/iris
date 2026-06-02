import { quoteYamlString } from "@/lib/frontmatter";
import { fileCreate, fileList } from "@/lib/ipc";
import { allocateNewDocumentName } from "@/lib/note-names";

export interface CreatedNote {
  path: string;
  title: string;
}

export interface CreateDefaultNoteOptions {
  /** Open-tab titles not yet visible in {@link fileList} (e.g. other blank tabs). */
  extraTakenTitles?: Iterable<string>;
  /** Target folder prefix, e.g. `notes/` — empty for vault root. */
  folderPrefix?: string;
  /** Optional user-provided title or filename from the file tree input. */
  titleHint?: string;
}

/** Create a note with display title in frontmatter; path aligns with title. */
export async function createDefaultNote(
  options: CreateDefaultNoteOptions = {},
): Promise<CreatedNote> {
  const files = await fileList();
  const { title, path } = allocateNewDocumentName(
    files,
    options.extraTakenTitles,
    options.folderPrefix ?? "",
    options.titleHint,
  );
  const content = `---\ntitle: ${quoteYamlString(title)}\n---\n\n`;
  const entry = await fileCreate(path, content);
  return { path: entry.path, title };
}
