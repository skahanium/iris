//! 订阅文章「保存为笔记」（阶段 5.2）。
//!
//! `buildFeedNoteMarkdown` 只消费安全 `FeedItemDetail`（无 raw payload），
//! 生成独立 Markdown 副本（输出契约见计划文档 Task 5.2）；`createFeedNote`
//! 经现有 `fileCreate` 写盘回执链路创建笔记，默认文件名经
//! `allocateNewDocumentName` 校验，不静默覆盖。保存后的笔记与订阅条目
//! 互不级联：后续 Feed 更新不修改 `.md`，删除笔记不影响 Feed。

import { fileCreate, fileList } from "@/lib/ipc";
import { isCreateConflict } from "@/lib/note-create";
import { allocateNewDocumentName } from "@/lib/note-names";
import type { FeedItemDetail } from "@/types/ipc";

/** 文件名非法字符（与 `sanitizeNoteFileName` 一致）。 */
const INVALID_FILE_CHARS = /[\\/:*?"<>|]/;

/** 目标目录合法段：非空、不含非法字符、不包含 `..`。 */
export function isValidFeedNoteFolder(folderPath: string): boolean {
  const trimmed = folderPath.trim();
  if (!trimmed) return true; // 空 = Vault 根
  return trimmed.split("/").every((segment) => {
    if (!segment || segment === "." || segment === "..") return false;
    return !INVALID_FILE_CHARS.test(segment);
  });
}

/** 标题行安全化：换行/回车替换为空格，防止破坏 Markdown 结构。 */
function safeTitleLine(title: string): string {
  return title.replace(/[\r\n]+/g, " ").trim();
}

/**
 * 生成保存为笔记的 Markdown 副本。缺 URL 的行省略链接（仅文本），
 * 缺日期/URL 的元数据行整体省略；正文原样拼接，不做二次 HTML 解码。
 */
export function buildFeedNoteMarkdown(
  detail: FeedItemDetail,
  savedAt: string,
): string {
  const { summary } = detail;
  const lines: string[] = [`# ${safeTitleLine(summary.title)}`, ""];

  const meta: string[] = [];
  const siteUrl = detail.siteUrl?.trim();
  if (siteUrl) {
    meta.push(`> 来源：[${safeTitleLine(summary.sourceTitle)}](${siteUrl})  `);
  } else {
    meta.push(`> 来源：${safeTitleLine(summary.sourceTitle)}  `);
  }
  if (summary.canonicalUrl) {
    meta.push(`> 原文：[打开原文](${summary.canonicalUrl})  `);
  }
  if (summary.publishedAt) {
    meta.push(`> 发布：${summary.publishedAt}  `);
  }
  meta.push(`> 保存：${savedAt}`);

  lines.push(...meta, "", detail.contentMarkdown.trimEnd(), "");
  return lines.join("\n");
}

/**
 * 经现有写盘回执链路创建笔记，返回 vault 相对路径。目标目录为 vault 内
 * 相对路径（空 = 根）；文件名冲突时自动追加「（N）」重试，不静默覆盖。
 */
export async function createFeedNote(
  markdown: string,
  titleHint: string,
  folderPath: string,
): Promise<string> {
  const folderPrefix = folderPath.trim()
    ? `${folderPath.trim().replace(/^\/+|\/+$/g, "")}/`
    : "";
  const extraTaken = new Set<string>();
  for (let attempt = 0; attempt < 5; attempt += 1) {
    const files = await fileList();
    const { title, path } = allocateNewDocumentName(
      files,
      [...extraTaken],
      folderPrefix,
      titleHint,
    );
    try {
      const receipt = await fileCreate(path, markdown);
      return receipt.entry.path;
    } catch (error) {
      if (isCreateConflict(error)) {
        extraTaken.add(title);
        continue;
      }
      throw error;
    }
  }
  throw new Error("无法分配不冲突的文件名，请手动清理笔记库后重试");
}
