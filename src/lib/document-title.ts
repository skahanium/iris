import { fileList } from "@/lib/ipc";

function pathStem(path: string): string {
  return path.replace(/\.md$/i, "").split("/").pop() ?? path;
}

/** Resolve `files.title` for a path; falls back to filename stem. */
export async function resolveDocumentTitle(
  path: string,
  hint?: string,
): Promise<string> {
  if (hint?.trim()) {
    return hint.trim();
  }
  const list = await fileList();
  const hit = list.find((f) => f.path === path);
  if (hit?.title.trim()) {
    return hit.title.trim();
  }
  return pathStem(path);
}
