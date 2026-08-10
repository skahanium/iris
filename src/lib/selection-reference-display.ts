import type { SelectionReferenceDisplay } from "@/types/ai";

function fileNameForPath(value: string): string | undefined {
  const normalized = value.replace(/\\/g, "/").trim();
  if (!normalized) return undefined;
  return normalized.split("/").filter(Boolean).at(-1);
}

/**
 * Projects persisted explicit references into safe UI metadata without
 * carrying the selected note body into chat history.
 */
export function selectionReferenceDisplayFromExplicitReferences(
  references: readonly unknown[],
): SelectionReferenceDisplay | null {
  for (const value of references) {
    if (typeof value !== "object" || value === null) continue;
    const reference = value as { kind?: unknown; filePath?: unknown };
    if (
      reference.kind !== "selection" ||
      typeof reference.filePath !== "string"
    )
      continue;
    const fileName = fileNameForPath(reference.filePath);
    if (fileName) return { fileName };
  }
  return null;
}
