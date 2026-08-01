const STORAGE_KEY = "iris.workspaceNavigator.preferences";

export type NavigatorSortDirection = "asc" | "desc";

export interface WorkspaceNavigatorPreferences {
  dividerPercent: number;
  folderSort: {
    key: "name" | "count";
    direction: NavigatorSortDirection;
  };
  fileSort: {
    key: "name" | "updatedAt";
    direction: NavigatorSortDirection;
  };
  showMedia: boolean;
}

export const DEFAULT_WORKSPACE_NAVIGATOR_PREFERENCES: WorkspaceNavigatorPreferences =
  {
    dividerPercent: 45,
    folderSort: { key: "name", direction: "asc" },
    fileSort: { key: "name", direction: "asc" },
    showMedia: false,
  };

function isDirection(value: unknown): value is NavigatorSortDirection {
  return value === "asc" || value === "desc";
}

function isPreferences(value: unknown): value is WorkspaceNavigatorPreferences {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Record<string, unknown>;
  const folderSort = candidate.folderSort;
  const fileSort = candidate.fileSort;
  if (
    !folderSort ||
    typeof folderSort !== "object" ||
    !fileSort ||
    typeof fileSort !== "object"
  ) {
    return false;
  }
  const folder = folderSort as Record<string, unknown>;
  const file = fileSort as Record<string, unknown>;
  return (
    typeof candidate.dividerPercent === "number" &&
    candidate.dividerPercent >= 25 &&
    candidate.dividerPercent <= 70 &&
    (folder.key === "name" || folder.key === "count") &&
    isDirection(folder.direction) &&
    (file.key === "name" || file.key === "updatedAt") &&
    isDirection(file.direction) &&
    typeof candidate.showMedia === "boolean"
  );
}

/** Read navigator presentation preferences only; folder selection and searches stay ephemeral. */
export function loadWorkspaceNavigatorPreferences(): WorkspaceNavigatorPreferences {
  if (typeof localStorage === "undefined") {
    return DEFAULT_WORKSPACE_NAVIGATOR_PREFERENCES;
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_WORKSPACE_NAVIGATOR_PREFERENCES;
    const parsed: unknown = JSON.parse(raw);
    return isPreferences(parsed)
      ? parsed
      : DEFAULT_WORKSPACE_NAVIGATOR_PREFERENCES;
  } catch {
    return DEFAULT_WORKSPACE_NAVIGATOR_PREFERENCES;
  }
}

/** Persist only path-free display preferences for the workspace navigator. */
export function saveWorkspaceNavigatorPreferences(
  preferences: WorkspaceNavigatorPreferences,
): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(preferences));
  } catch {
    // Storage is an enhancement; navigating files must remain available.
  }
}
