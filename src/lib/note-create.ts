import { fileCreate } from "@/lib/ipc";

export async function createDefaultNote(): Promise<string> {
  const name = `note-${Date.now()}.md`;
  await fileCreate(name);
  return name;
}
