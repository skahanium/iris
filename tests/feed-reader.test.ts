import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  blockRemoteImages,
  handleFeedLinkClick,
  renderFeedMarkdown,
  sanitizeFeedHtml,
} from "@/lib/feed-reader";

const openExternalHttpsUrl = vi.fn();
vi.mock("@/lib/ipc", () => ({
  openExternalHttpsUrl: (...args: unknown[]) => openExternalHttpsUrl(...args),
}));

beforeEach(() => {
  openExternalHttpsUrl.mockClear();
});

describe("feed-reader 安全渲染", () => {
  it("renders markdown to sanitized html", () => {
    const html = renderFeedMarkdown(
      "# 标题\n\n正文 with **bold** and [link](https://example.com/a).",
    );
    expect(html).toContain("<h1>标题</h1>");
    expect(html).toContain("<strong>bold</strong>");
    expect(html).toContain('href="https://example.com/a"');
  });

  it("forbids style/iframe/form/video/audio/object/embed", () => {
    const html = sanitizeFeedHtml(
      [
        "<p>ok</p>",
        "<style>.x{}</style>",
        '<iframe src="https://example.com/f"></iframe>',
        '<form action="https://example.com/s"><input name="q"></form>',
        '<video src="https://example.com/v.mp4"></video>',
        '<audio src="https://example.com/a.mp3"></audio>',
        '<object data="https://example.com/o"></object>',
        '<embed src="https://example.com/e">',
      ].join(""),
    );
    expect(html).toContain("<p>ok</p>");
    expect(html).not.toContain("<style");
    expect(html).not.toContain("iframe");
    expect(html).not.toContain("<form");
    expect(html).not.toContain("video");
    expect(html).not.toContain("audio");
    expect(html).not.toContain("object");
    expect(html).not.toContain("embed");
  });

  it("allows only https links and neutralizes others", () => {
    const html = sanitizeFeedHtml(
      [
        '<a href="https://example.com/safe">safe</a>',
        '<a href="http://example.com/plain">plain</a>',
        '<a href="javascript:alert(1)">js</a>',
        '<a href="file:///etc/passwd">file</a>',
      ].join(""),
    );
    expect(html).toContain('href="https://example.com/safe"');
    expect(html).not.toContain('http://example.com/plain"');
    expect(html).not.toContain("javascript:");
    expect(html).not.toContain("file:///");
  });

  it("blocks remote images by default without issuing requests", () => {
    const html = blockRemoteImages(
      '<p><img src="https://cdn.example.com/a.png" alt="a"><img src="https://cdn.example.com/b.png" alt="b"></p>',
    );
    expect(html).not.toContain("<img");
    // 占位无 `src` 属性（data-src 只作按需加载的数据源）。
    expect(html).not.toMatch(/\ssrc="/);
    // 占位保留 alt 文本与数据源（供按需加载），但无 src 就不会发请求。
    expect(html).toContain("feed-img-placeholder");
    expect(html).toContain('data-src="https://cdn.example.com/a.png"');
  });

  it("keeps data-src out of the DOM src attribute (no passive requests)", () => {
    const html = blockRemoteImages(
      '<img src="https://cdn.example.com/c.png" alt="c">',
    );
    const doc = new DOMParser().parseFromString(html, "text/html");
    const images = doc.querySelectorAll("img");
    expect(images.length).toBe(0);
    const placeholders = doc.querySelectorAll(".feed-img-placeholder");
    expect(placeholders.length).toBe(1);
    expect(placeholders[0]?.getAttribute("src")).toBeNull();
  });

  it("intercepts external link clicks through openExternalHttpsUrl", () => {
    const anchor = document.createElement("a");
    anchor.href = "https://example.com/article";
    anchor.addEventListener("click", handleFeedLinkClick);
    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    anchor.dispatchEvent(event);
    expect(openExternalHttpsUrl).toHaveBeenCalledWith(
      "https://example.com/article",
    );
  });

  it("does not open non-https links", () => {
    const anchor = document.createElement("a");
    anchor.href = "javascript:alert(1)";
    anchor.addEventListener("click", handleFeedLinkClick);
    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    anchor.dispatchEvent(event);
    expect(openExternalHttpsUrl).not.toHaveBeenCalled();
  });

  it("leaves already-rendered inline images untouched when loading is allowed", () => {
    // 用户显式加载后：允许 https 图片以 no-referrer 呈现。
    const html = sanitizeFeedHtml(
      '<img src="https://cdn.example.com/d.png" alt="d">',
    );
    expect(html).toContain('src="https://cdn.example.com/d.png"');
    expect(html).toContain('loading="lazy"');
    expect(html).toContain('referrerpolicy="no-referrer"');
  });
});
