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
  const folderPrefix = options.folderPrefix ?? "";
  const titleHint = options.titleHint;
  const extraTaken = new Set(options.extraTakenTitles ?? []);

  for (let attempt = 0; attempt < 5; attempt++) {
    const files = await fileList();
    const { title, path } = allocateNewDocumentName(
      files,
      [...extraTaken],
      folderPrefix,
      titleHint,
    );
    const content = `---\ntitle: ${quoteYamlString(title)}\n---\n\n`;
    try {
      const entry = await fileCreate(path, content);
      return { path: entry.path, title };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (
        msg.includes("already exists") ||
        msg.includes("File already exists")
      ) {
        // Name conflict (stale DB or disk leftover) — blacklist and retry
        extraTaken.add(title);
        continue;
      }
      throw e;
    }
  }
  throw new Error("无法分配不冲突的文件名，请手动清理笔记库后重试");
}
