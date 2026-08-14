//! 保存为笔记契约测试（阶段 5.2）。
//!
//! `buildFeedNoteMarkdown` 只消费安全 DTO：缺作者/日期/URL、危险标题字符、
//! UTF-8、正文不被二次 HTML 解码；`createFeedNote` 复用 fileCreate 回执
//! 链路、默认文件名经现有路径校验、重名不静默覆盖。

import { beforeEach, describe, expect, it, vi } from "vitest";

const { fileCreate, fileList } = vi.hoisted(() => ({
  fileCreate: vi.fn(),
  fileList: vi.fn(),
}));

vi.mock("@/lib/ipc", () => ({
  fileCreate,
  fileList,
}));

import {
  buildFeedNoteMarkdown,
  createFeedNote,
  isValidFeedNoteFolder,
} from "@/lib/feed-note-export";
import type { FeedItemDetail, FeedItemSummary } from "@/types/ipc";

function summary(overrides: Partial<FeedItemSummary> = {}): FeedItemSummary {
  return {
    rowId: 1,
    id: "item-1",
    sourceId: "src-1",
    sourceTitle: "Example Feed",
    title: "文章标题",
    authorName: "作者",
    canonicalUrl: "https://example.com/article",
    publishedAt: "2026-08-11T08:00:00Z",
    receivedAt: "2026-08-11T08:00:00Z",
    sortAt: "2026-08-11T08:00:00Z",
    excerpt: "…",
    isRead: false,
    isStarred: false,
    isArchived: false,
    conversionStatus: "ok",
    ...overrides,
  };
}

function detail(overrides: Partial<FeedItemDetail> = {}): FeedItemDetail {
  return {
    summary: summary(),
    contentMarkdown: "# 小节\n\n正文 **加粗** 与 `代码`。\n\n- 列表项",
    summaryMarkdown: "",
    siteUrl: "https://example.com/site",
    contentOrigin: "feed",
    fulltextStatus: "not_requested",
    primaryDocument: null,
    fulltextNeedsRefresh: false,
    imagesAuthorized: false,
    ...overrides,
  };
}

const SAVED_AT = "2026-08-11T09:00:00Z";

describe("buildFeedNoteMarkdown 输出契约", () => {
  it("按模板生成完整元数据块", () => {
    const md = buildFeedNoteMarkdown(detail(), SAVED_AT);
    expect(md).toContain("# 文章标题");
    expect(md).toContain("> 来源：[Example Feed](https://example.com/site)  ");
    expect(md).toContain("> 原文：[打开原文](https://example.com/article)  ");
    expect(md).toContain("> 发布：2026-08-11T08:00:00Z  ");
    expect(md).toContain("> 保存：2026-08-11T09:00:00Z");
    // 正文原样保留：不做二次 HTML 解码/转义。
    expect(md).toContain("正文 **加粗** 与 `代码`。");
    expect(md).toContain("# 小节");
  });

  it("缺站点 URL 时来源行退化为纯文本", () => {
    const md = buildFeedNoteMarkdown(detail({ siteUrl: null }), SAVED_AT);
    expect(md).toContain("> 来源：Example Feed  ");
    expect(md).not.toContain("](https://example.com/site)");
  });

  it("缺原文 URL 与发布时间时省略对应行", () => {
    const md = buildFeedNoteMarkdown(
      detail({
        summary: summary({ canonicalUrl: null, publishedAt: null }),
      }),
      SAVED_AT,
    );
    expect(md).not.toContain("原文：");
    expect(md).not.toContain("发布：");
    expect(md).toContain("> 保存：2026-08-11T09:00:00Z");
  });

  it("危险标题字符不破坏 Markdown 结构", () => {
    const md = buildFeedNoteMarkdown(
      detail({ summary: summary({ title: "标题\n第二行\r第三行" }) }),
      SAVED_AT,
    );
    const firstLine = md.split("\n")[0];
    expect(firstLine).toBe("# 标题 第二行 第三行");
    expect(md.split("\n").length).toBeGreaterThan(3);
  });

  it("标题含 # 与引用符时保持字面", () => {
    const md = buildFeedNoteMarkdown(
      detail({ summary: summary({ title: "Rust #1 与 > 引用" }) }),
      SAVED_AT,
    );
    expect(md).toContain("# Rust #1 与 > 引用");
  });

  it("正文为 HTML 实体文本时不二次解码", () => {
    const md = buildFeedNoteMarkdown(
      detail({ contentMarkdown: "&lt;script&gt; 保持字面" }),
      SAVED_AT,
    );
    expect(md).toContain("&lt;script&gt; 保持字面");
    expect(md).not.toContain("<script>");
  });

  it("UTF-8 中文与 emoji 原样保留", () => {
    const md = buildFeedNoteMarkdown(
      detail({
        summary: summary({ title: "中文订阅 🚀 标题" }),
        contentMarkdown: "正文：日本語・한국어・中文",
      }),
      SAVED_AT,
    );
    expect(md).toContain("# 中文订阅 🚀 标题");
    expect(md).toContain("正文：日本語・한국어・中文");
  });

  it("正文为空时仍输出元数据块", () => {
    const md = buildFeedNoteMarkdown(
      detail({ contentMarkdown: "   " }),
      SAVED_AT,
    );
    expect(md).toContain("> 保存：2026-08-11T09:00:00Z");
  });
});

describe("isValidFeedNoteFolder", () => {
  it("接受空串与合法相对目录", () => {
    expect(isValidFeedNoteFolder("")).toBe(true);
    expect(isValidFeedNoteFolder("技术/Rust")).toBe(true);
  });
  it("拒绝非法字符、空段与 ..", () => {
    expect(isValidFeedNoteFolder("技:术")).toBe(false);
    expect(isValidFeedNoteFolder("技术//Rust")).toBe(false);
    expect(isValidFeedNoteFolder("../技术")).toBe(false);
    expect(isValidFeedNoteFolder("技术/../Rust")).toBe(false);
    expect(isValidFeedNoteFolder(".")).toBe(false);
  });

  it("重复校验同一非法目录时结果保持稳定", () => {
    expect(isValidFeedNoteFolder("技:术")).toBe(false);
    expect(isValidFeedNoteFolder("技:术")).toBe(false);
  });
});

describe("createFeedNote 写盘链路", () => {
  beforeEach(() => {
    fileList.mockReset();
    fileCreate.mockReset();
    fileList.mockResolvedValue([
      {
        path: "已存在.md",
        title: "已存在",
        updatedAt: "2026-08-01T00:00:00Z",
        isLocked: false,
      },
    ]);
    fileCreate.mockImplementation(async (path: string) => ({
      entry: {
        id: 1,
        path,
        title: path,
        updated_at: "2026-08-01T00:00:00Z",
        word_count: 1,
      },
      contentHash: "hash",
      indexStatus: "synced",
    }));
  });

  it("按标题分配文件名并调用 fileCreate 回执链路", async () => {
    const path = await createFeedNote("# 正文", "我的文章", "技术");
    expect(path).toBe("技术/我的文章.md");
    expect(fileCreate).toHaveBeenCalledWith("技术/我的文章.md", "# 正文");
  });

  it("重名时追加（N）不静默覆盖", async () => {
    fileList.mockResolvedValue([
      {
        path: "技术/我的文章.md",
        title: "我的文章",
        updatedAt: "2026-08-01T00:00:00Z",
        isLocked: false,
      },
    ]);
    const path = await createFeedNote("body", "我的文章", "技术");
    expect(path).toBe("技术/我的文章（1）.md");
  });

  it("fileCreate 重名冲突时黑名单重试", async () => {
    fileCreate
      .mockRejectedValueOnce({ code: "file_already_exists" })
      .mockResolvedValueOnce({
        entry: {
          id: 2,
          path: "文章（1）.md",
          title: "文章（1）",
          updated_at: "x",
          word_count: 1,
        },
        contentHash: "h",
        indexStatus: "synced",
      });
    const path = await createFeedNote("body", "文章", "");
    expect(path).toBe("文章（1）.md");
  });

  it("危险标题字符被现有路径校验清理", async () => {
    const path = await createFeedNote("body", "危险/名:称?", "");
    expect(path).not.toContain("/");
    expect(path).toMatch(/\.md$/);
  });
});
