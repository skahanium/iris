export interface StartupNoteCandidate {
  path: string;
  titleHint?: string;
}

/** Prefer snapshot.activePath if in openNotePaths or recentPaths; else first recentPaths entry. */
export function resolveStartupNote(input: {
  activePath: string | null;
  openNotePaths: readonly string[];
  recentPaths: readonly string[];
}): StartupNoteCandidate | null {
  const { activePath, openNotePaths, recentPaths } = input;
  if (
    activePath &&
    (openNotePaths.includes(activePath) || recentPaths.includes(activePath))
  ) {
    return { path: activePath };
  }
  const firstRecent = recentPaths[0];
  if (firstRecent) {
    return { path: firstRecent };
  }
  return null;
}
