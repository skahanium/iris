import { useEffect } from "react";

import type { TabItem } from "@/components/layout/TabBar";
import { saveWorkspaceSessionSnapshot } from "@/lib/workspace-session-snapshot";

interface UseWorkspaceSessionSnapshotOptions {
  activePath: string | null;
  tabs: readonly TabItem[];
  vaultPath: string | null;
}

export function useWorkspaceSessionSnapshot({
  activePath,
  tabs,
  vaultPath,
}: UseWorkspaceSessionSnapshotOptions): void {
  useEffect(() => {
    if (!vaultPath) return;
    const now = Date.now();
    saveWorkspaceSessionSnapshot(vaultPath, {
      activePath,
      openNotes: tabs.map((tab, index) => ({
        path: tab.path,
        title: tab.title,
        isLocked: tab.locked === true,
        lastActiveAt: tab.path === activePath ? now : now - index - 1,
      })),
    });
  }, [activePath, tabs, vaultPath]);
}
