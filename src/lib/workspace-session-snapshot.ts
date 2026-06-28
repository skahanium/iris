export interface WorkspaceSessionNoteSnapshot {
  path: string;
  title: string;
  isLocked: boolean;
  lastActiveAt: number;
}

export interface WorkspaceSessionSnapshotV1 {
  version: 1;
  savedAt: number;
  activePath: string | null;
  openNotes: WorkspaceSessionNoteSnapshot[];
}

interface WorkspaceSessionSnapshotInput {
  activePath: string | null;
  openNotes: WorkspaceSessionNoteSnapshot[];
}

const SNAPSHOT_PREFIX = "iris.workspace-session.v1:";
const MAX_SNAPSHOT_NOTES = 16;

function storageKey(vaultId: string): string {
  return `${SNAPSHOT_PREFIX}${vaultId}`;
}

function isNoteSnapshot(value: unknown): value is WorkspaceSessionNoteSnapshot {
  if (!value || typeof value !== "object") {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.path === "string" &&
    typeof candidate.title === "string" &&
    typeof candidate.isLocked === "boolean" &&
    typeof candidate.lastActiveAt === "number"
  );
}

function sanitizeSnapshot(
  input: WorkspaceSessionSnapshotInput,
): WorkspaceSessionSnapshotV1 {
  return {
    version: 1,
    savedAt: Date.now(),
    activePath: input.activePath,
    openNotes: input.openNotes
      .filter(isNoteSnapshot)
      .slice(0, MAX_SNAPSHOT_NOTES)
      .map((note) => ({
        path: note.path,
        title: note.title,
        isLocked: note.isLocked,
        lastActiveAt: note.lastActiveAt,
      })),
  };
}

export function saveWorkspaceSessionSnapshot(
  vaultId: string,
  input: WorkspaceSessionSnapshotInput,
): void {
  try {
    localStorage.setItem(
      storageKey(vaultId),
      JSON.stringify(sanitizeSnapshot(input)),
    );
  } catch {
    return;
  }
}

export function loadWorkspaceSessionSnapshot(
  vaultId: string,
): WorkspaceSessionSnapshotV1 | null {
  let raw: string | null = null;
  try {
    raw = localStorage.getItem(storageKey(vaultId));
  } catch {
    return null;
  }
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== "object") return null;
    const candidate = parsed as Record<string, unknown>;
    if (candidate.version !== 1 || !Array.isArray(candidate.openNotes)) {
      return null;
    }
    return {
      version: 1,
      savedAt: typeof candidate.savedAt === "number" ? candidate.savedAt : 0,
      activePath:
        typeof candidate.activePath === "string" ? candidate.activePath : null,
      openNotes: candidate.openNotes
        .filter(isNoteSnapshot)
        .slice(0, MAX_SNAPSHOT_NOTES),
    };
  } catch {
    return null;
  }
}
