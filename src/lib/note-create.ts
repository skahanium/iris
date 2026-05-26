import { fileCreate, fileList } from "@/lib/ipc";
import { allocateNewDocumentName } from "@/lib/note-names";

export interface CreatedNote {
  path: string;
  title: string;
}

export async function createDefaultNote(): Promise<CreatedNote> {
  const files = await fileList();
  const { title, path } = allocateNewDocumentName(files);
  const entry = await fileCreate(path, `# ${title}\n\n`);
  return { path: entry.path, title: entry.title };
}
