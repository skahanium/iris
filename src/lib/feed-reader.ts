//! 订阅文章只读渲染与安全边界（阶段 4）。
//!
//! 职责：Markdown 渲染配置、DOMPurify allowlist、外链拦截与远程图片占位。
//! 不包含任何业务数据读取；正文渲染路径 = proseMarked → 专用净化 →
//! 远程图片占位（默认零请求）。

import { openExternalHttpsUrl } from "@/lib/ipc";
import { proseMarked } from "@/lib/markdown-render";
import { DOMPurify } from "@/lib/sanitize-vendor";

/** 订阅正文允许的标签：禁止 style/iframe/form/video/audio/object/embed；
 * 图片允许通过净化（https 仅），但默认渲染会被占位替换。 */
const FEED_ALLOWED_TAGS = [
  "p",
  "br",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "blockquote",
  "pre",
  "ul",
  "ol",
  "li",
  "hr",
  "div",
  "table",
  "thead",
  "tbody",
  "tr",
  "th",
  "td",
  "strong",
  "em",
  "code",
  "a",
  "span",
  "img",
  "sup",
  "sub",
  "del",
  "ins",
  "mark",
];

const FEED_ALLOWED_ATTR = [
  "href",
  "src",
  "alt",
  "title",
  "class",
  "id",
  "colspan",
  "rowspan",
  "align",
  "start",
  "target",
  "rel",
  "aria-label",
];

/** 链接只允许 HTTPS；其余 scheme（http/javascript:/data:/file:）被净化。 */
const FEED_ALLOWED_URI_REGEXP = /^https:\/\//i;

const FEED_FORBID_TAGS = [
  "style",
  "script",
  "iframe",
  "form",
  "video",
  "audio",
  "object",
  "embed",
];

const FEED_FORBID_ATTR = [
  "onclick",
  "onerror",
  "onload",
  "onmouseover",
  "onfocus",
  "onblur",
  "style",
];

/** 订阅正文专用 DOMPurify allowlist。 */
export function sanitizeFeedHtml(html: string): string {
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS: FEED_ALLOWED_TAGS,
    ALLOWED_ATTR: FEED_ALLOWED_ATTR,
    ALLOWED_URI_REGEXP: FEED_ALLOWED_URI_REGEXP,
    FORBID_TAGS: FEED_FORBID_TAGS,
    FORBID_ATTR: FEED_FORBID_ATTR,
    ALLOW_DATA_ATTR: false,
    ALLOW_UNKNOWN_PROTOCOLS: false,
  });
}

/** 远程图片默认阻止：把 `http(s)://` 图片替换为占位（保留 alt 与
 * data-src 供按需加载），占位无 `src` 属性，浏览器不会发起请求。 */
export function blockRemoteImages(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const images = Array.from(doc.querySelectorAll("img"));
  for (const img of images) {
    const src = img.getAttribute("src") ?? "";
    if (!/^https?:\/\//i.test(src)) continue;
    const placeholder = doc.createElement("span");
    placeholder.className = "feed-img-placeholder";
    placeholder.setAttribute("aria-label", img.getAttribute("alt") ?? "图片");
    placeholder.setAttribute("data-src", src);
    placeholder.textContent = "图片";
    img.replaceWith(placeholder);
  }
  return doc.body.innerHTML;
}

/** 订阅正文完整渲染链路：Markdown → 净化 → 远程图片占位。 */
export function renderFeedMarkdown(markdown: string): string {
  const html = proseMarked.parse(markdown) as string;
  return blockRemoteImages(sanitizeFeedHtml(html));
}

/** 外链拦截：只允许 HTTPS 经 `openExternalHttpsUrl` 打开；
 * 非 HTTPS 链接点击被吞掉（防御，normalize 已不产出此类链接）。 */
export function handleFeedLinkClick(event: MouseEvent): void {
  const target = event.target as HTMLElement | null;
  const anchor = target?.closest?.("a[href]") as HTMLAnchorElement | null;
  if (!anchor) return;
  const href = anchor.getAttribute("href") ?? "";
  event.preventDefault();
  if (/^https:\/\//i.test(href)) {
    void openExternalHttpsUrl(href);
  }
}
