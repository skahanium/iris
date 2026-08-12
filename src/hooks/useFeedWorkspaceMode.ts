//! 订阅工作区模式状态（阶段 4）。
//!
//! 持有 `documents | feeds` 模式与切换动作；进入 feeds 前退出禅模式
//! 并调用 `notify` 提示一次（非阻断）。

import { useCallback, useState } from "react";

import type { AppWorkspaceMode } from "@/lib/workspace-chrome-layout";

export interface FeedWorkspaceModeApi {
  workspaceMode: AppWorkspaceMode;
  /** 模式切换入口（标题栏 Rss 按钮）；进入 feeds 前先退出禅模式。 */
  handleWorkspaceModeChange: (mode: AppWorkspaceMode) => void;
  /** 点击文档 Tab / 返回文档时调用。 */
  returnToDocuments: () => void;
}

export function useFeedWorkspaceMode(
  zen: boolean,
  setZen: (zen: boolean) => void,
  notify: (message: string) => void,
): FeedWorkspaceModeApi {
  const [workspaceMode, setWorkspaceMode] =
    useState<AppWorkspaceMode>("documents");

  const handleWorkspaceModeChange = useCallback(
    (mode: AppWorkspaceMode) => {
      if (mode === "feeds" && zen) {
        setZen(false);
        notify("禅模式已退出：订阅工作区暂不支持禅模式");
      }
      setWorkspaceMode(mode);
    },
    [notify, setZen, zen],
  );

  const returnToDocuments = useCallback(() => {
    setWorkspaceMode("documents");
  }, []);

  return { workspaceMode, handleWorkspaceModeChange, returnToDocuments };
}
