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
}

/** Create a note with display title in frontmatter; path aligns with title. */
export async function createDefaultNote(
  options: CreateDefaultNoteOptions = {},
): Promise<CreatedNote> {
  const files = await fileList();
  const { title, path } = allocateNewDocumentName(files, options.extraTakenTitles);
  const content = `---\ntitle: ${quoteYamlString(title)}\n---\n\n`;
  const entry = await fileCreate(path, content);
  return { path: entry.path, title };
}
