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
  "loading",
  "referrerpolicy",
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
  const sanitized = DOMPurify.sanitize(html, {
    ALLOWED_TAGS: FEED_ALLOWED_TAGS,
    ALLOWED_ATTR: FEED_ALLOWED_ATTR,
    ALLOWED_URI_REGEXP: FEED_ALLOWED_URI_REGEXP,
    FORBID_TAGS: FEED_FORBID_TAGS,
    FORBID_ATTR: FEED_FORBID_ATTR,
    ALLOW_DATA_ATTR: false,
    ALLOW_UNKNOWN_PROTOCOLS: false,
  });
  const doc = new DOMParser().parseFromString(sanitized, "text/html");
  for (const image of Array.from(doc.querySelectorAll("img"))) {
    image.setAttribute("loading", "lazy");
    image.setAttribute("referrerpolicy", "no-referrer");
    image.setAttribute("data-feed-image", "remote");
  }
  return doc.body.innerHTML;
}

/** 本地受控图片读取失败时改成中性文本，不保留破图框。 */
export function handleFeedImageError(event: Event): void {
  const image = event.target;
  if (!(image instanceof HTMLImageElement)) return;
  const imageKind = image.getAttribute("data-feed-image");
  if (imageKind !== "cached" && imageKind !== "remote") return;
  const fallback = document.createElement("span");
  fallback.className = "feed-img-unavailable";
  fallback.textContent = image.alt?.trim()
    ? `图片无法加载：${image.alt.trim()}`
    : "图片无法加载";
  image.replaceWith(fallback);
}

/** 为只有一张正文图片的段落标记块级布局，避免图片或占位混入行文。 */
function markStandaloneImageBlocks(doc: Document): void {
  for (const paragraph of Array.from(doc.querySelectorAll("p"))) {
    const children = Array.from(paragraph.children);
    if (
      children.length === 1 &&
      (children[0]?.tagName === "IMG" ||
        children[0]?.classList.contains("feed-img-placeholder"))
    ) {
      paragraph.classList.add("feed-image-block");
    }
  }
}

function createImagePlaceholder(
  doc: Document,
  image: HTMLImageElement,
  failed: boolean,
): HTMLElement {
  const placeholder = doc.createElement(failed ? "button" : "span");
  placeholder.className = failed
    ? "feed-img-placeholder feed-img-placeholder--failed"
    : "feed-img-placeholder";
  const alt = image.getAttribute("alt")?.trim() || "图片";
  placeholder.setAttribute("aria-label", alt);
  placeholder.setAttribute("data-src", image.getAttribute("src") ?? "");
  if (failed) {
    placeholder.setAttribute("type", "button");
    placeholder.setAttribute("data-feed-image-retry", "");
    placeholder.textContent = "图片加载失败，点击重试";
    placeholder.setAttribute("aria-label", "图片加载失败，点击重试");
  } else {
    placeholder.textContent = "图片";
  }
  return placeholder;
}

/**
 * 把已获本篇授权的图片替换为后端签发的本地 lease。
 * 即使调用方请求加载，未知图片仍保持占位，绝不把远程 `src` 交给 WebView。
 */
function replaceAuthorizedImages(
  html: string,
  imageLeases: ReadonlyMap<string, string>,
  failedImages: ReadonlySet<string>,
): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  for (const img of Array.from(doc.querySelectorAll("img"))) {
    const sourceUrl = img.getAttribute("src") ?? "";
    const leaseUrl = imageLeases.get(sourceUrl);
    if (leaseUrl?.startsWith("iris-feed-image://localhost/")) {
      img.setAttribute("src", leaseUrl);
      img.setAttribute("data-feed-image", "cached");
      continue;
    }
    img.replaceWith(
      createImagePlaceholder(doc, img, failedImages.has(sourceUrl)),
    );
  }
  markStandaloneImageBlocks(doc);
  return doc.body.innerHTML;
}

/** 远程图片默认阻止：把 `http(s)://` 图片替换为占位（保留 alt 与
 * data-src 供按需加载），占位无 `src` 属性，浏览器不会发起请求。 */
export function blockRemoteImages(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const images = Array.from(doc.querySelectorAll("img"));
  for (const img of images) {
    const src = img.getAttribute("src") ?? "";
    if (!/^https?:\/\//i.test(src)) continue;
    img.replaceWith(createImagePlaceholder(doc, img, false));
  }
  markStandaloneImageBlocks(doc);
  return doc.body.innerHTML;
}

/** 订阅正文完整渲染链路：Markdown → 净化 → 远程图片占位。
 * `allowRemoteImages` 为 true 时只允许后端签发的本地图片 lease，绝不热链。 */
export function renderFeedMarkdown(
  markdown: string,
  allowRemoteImages = false,
  imageLeases: ReadonlyMap<string, string> = new Map(),
  failedImages: ReadonlySet<string> = new Set(),
): string {
  const html = proseMarked.parse(markdown) as string;
  const sanitized = sanitizeFeedHtml(html);
  return allowRemoteImages
    ? replaceAuthorizedImages(sanitized, imageLeases, failedImages)
    : blockRemoteImages(sanitized);
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
