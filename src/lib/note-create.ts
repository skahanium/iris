import { fileCreate } from "@/lib/ipc";

export interface CreatedNote {
  path: string;
  title: string;
}

const NEW_NOTE_TEMPLATE = "---\ntitle: \"\"\n---\n\n";

/** Create a note with a stable machine path; display title lives in frontmatter. */
export async function createDefaultNote(): Promise<CreatedNote> {
  const path = `untitled-${Date.now()}.md`;
  const entry = await fileCreate(path, NEW_NOTE_TEMPLATE);
  return { path: entry.path, title: "无标题" };
}
