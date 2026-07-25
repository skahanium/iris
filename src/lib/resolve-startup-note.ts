export interface StartupNoteCandidate {
  path: string;
  titleHint?: string;
}

/**
 * Pick the note to restore on cold start from the last workspace session snapshot
 * (`openNotes` / `activePath` only). When the user closed every tab before quit,
 * `openNotePaths` is empty and this returns null — library recents are not auto-opened.
 */
export function resolveStartupNote(input: {
  activePath: string | null;
  openNotePaths: readonly string[];
}): StartupNoteCandidate | null {
  const { activePath, openNotePaths } = input;
  if (openNotePaths.length === 0) {
    return null;
  }
  const normalizedActive =
    typeof activePath === "string" && activePath.length > 0
      ? activePath
      : null;
  if (normalizedActive && openNotePaths.includes(normalizedActive)) {
    return { path: normalizedActive };
  }
  return { path: openNotePaths[0]! };
}
