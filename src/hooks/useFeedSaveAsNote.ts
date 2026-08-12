//! 「保存为笔记」App 层接线（阶段 5.2）。
//!
//! 经 `createFeedNote`（fileCreate 写盘回执链路）创建独立副本后打开生成
//! 的笔记并返回文档模式；失败原样抛出让文章侧显示可重试错误。Feed
//! 组件不直接 `invoke` 或 `fs.write`。

import { useCallback, useRef } from "react";

import { createFeedNote } from "@/lib/feed-note-export";

/** 笔记已落盘、但工作区打开失败。重试时只能重新打开，不能再次创建。 */
export class FeedNoteOpenError extends Error {
  readonly savedPath: string;

  constructor(savedPath: string) {
    super(`笔记已保存到 ${savedPath}，但未能打开；请重试打开。`);
    this.name = "FeedNoteOpenError";
    this.savedPath = savedPath;
  }
}

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
  const pendingOpenRef = useRef<{
    signature: string;
    path: string;
  } | null>(null);

  return useCallback(
    async (markdown: string, titleHint: string, folderPath: string) => {
      // Markdown 中的“保存”时间每次点击都会变化；签名忽略这一行但保留
      // 其余正文，既避免真实 UI 重试重复创建，也不会混淆同名的不同文章。
      const stableMarkdown = markdown.replace(/^> 保存：.*$/m, "> 保存：");
      const signature = JSON.stringify([stableMarkdown, titleHint, folderPath]);
      const pending = pendingOpenRef.current;
      const path =
        pending?.signature === signature
          ? pending.path
          : await createFeedNote(markdown, titleHint, folderPath);
      try {
        await openNote(path);
      } catch {
        pendingOpenRef.current = { signature, path };
        throw new FeedNoteOpenError(path);
      }
      pendingOpenRef.current = null;
      onOpened?.();
      return path;
    },
    [onOpened, openNote],
  );
}
