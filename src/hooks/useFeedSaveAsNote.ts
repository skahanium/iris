//! 「保存为笔记」App 层接线（阶段 5.2）。
//!
//! 经 `createFeedNote`（fileCreate 写盘回执链路）创建独立副本后打开生成
//! 的笔记并返回文档模式；失败原样抛出让文章侧显示可重试错误。Feed
//! 组件不直接 `invoke` 或 `fs.write`。

import { useCallback } from "react";

import { createFeedNote } from "@/lib/feed-note-export";

/**
 * @param openNote 现有文档打开链路（tab 复用/持久化协调器 baseline）。
 * @param onOpened 打开完成后返回文档工作区的回调（可选）。
 */
export function useFeedSaveAsNote(
  openNote: (path: string) => Promise<void>,
  onOpened?: () => void,
): (
  markdown: string,
  titleHint: string,
  folderPath: string,
) => Promise<string> {
  return useCallback(
    async (markdown: string, titleHint: string, folderPath: string) => {
      const path = await createFeedNote(markdown, titleHint, folderPath);
      await openNote(path);
      onOpened?.();
      return path;
    },
    [onOpened, openNote],
  );
}
