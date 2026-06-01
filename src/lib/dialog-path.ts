/** Normalize `plugin-dialog` `open()` result to a single filesystem path. */
export function normalizeOpenDialogPath(
  selected: string | string[] | null | undefined,
): string | null {
  if (selected == null) return null;
  if (Array.isArray(selected)) {
    const first = selected[0];
    return typeof first === "string" && first.length > 0 ? first : null;
  }
  return selected.length > 0 ? selected : null;
}
