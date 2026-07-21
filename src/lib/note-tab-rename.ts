import type { TabItem } from "@/components/layout/TabBar";

export interface SelectMarkdownCacheAfterPathRenameOptions {
  destinationCached: string | undefined;
  destinationDirty: boolean;
  sourceCached: string | undefined;
  sourceDirty: boolean;
  sourceOverride: string | undefined;
}

/**
 * Chooses which in-memory Markdown snapshot survives a path merge.
 *
 * Rename-source content wins when the source itself is dirty (or an explicit
 * override is provided). A dirty destination is preserved when the source has
 * nothing recoverable, so an open dirty tab at the destination is not dropped.
 */
export function selectMarkdownCacheAfterPathRename(
  options: SelectMarkdownCacheAfterPathRenameOptions,
): string | undefined {
  const {
    destinationCached,
    destinationDirty,
    sourceCached,
    sourceDirty,
    sourceOverride,
  } = options;
  if (sourceOverride !== undefined) {
    return sourceOverride;
  }
  if (sourceDirty && sourceCached !== undefined) {
    return sourceCached;
  }
  if (destinationDirty && destinationCached !== undefined) {
    return destinationCached;
  }
  return sourceCached ?? destinationCached;
}

/**
 * 文件重命名后合并标签栏：将 `oldPath` 标签改为 `newPath`，并移除重复项。
 */
export function mergeTabsAfterPathRename(
  tabs: TabItem[],
  oldPath: string,
  newPath: string,
  title?: string,
): TabItem[] {
  if (oldPath === newPath) return tabs;

  const oldTab = tabs.find((t) => t.path === oldPath);
  const newTab = tabs.find((t) => t.path === newPath);
  const rest = tabs.filter((t) => t.path !== oldPath && t.path !== newPath);

  if (!oldTab) {
    if (!newTab || !title) return tabs;
    return tabs.map((t) => (t.path === newPath ? { ...t, title } : t));
  }

  const merged: TabItem = {
    ...(oldTab ?? newTab),
    path: newPath,
    title: title ?? newTab?.title ?? oldTab.title,
    dirty: Boolean(oldTab.dirty || newTab?.dirty),
  };

  return [...rest, merged];
}
