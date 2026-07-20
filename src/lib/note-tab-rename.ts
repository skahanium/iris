import type { TabItem } from "@/components/layout/TabBar";

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
