import DOMPurify from "dompurify";

/** 白名单标签：Markdown 渲染后需要的元素 */
const ALLOWED_TAGS = [
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

/** 白名单属性 */
const ALLOWED_ATTR = [
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
  "data-cite-ref",
];

/** 明确禁止的标签（即使出现在白名单中也被移除） */
const FORBID_TAGS = ["style", "script", "iframe", "object", "embed", "form"];

/** 明确禁止的属性 */
const FORBID_ATTR = [
  "onclick",
  "onerror",
  "onload",
  "onmouseover",
  "onfocus",
  "onblur",
];

/** 允许的 URI 协议 */
const ALLOWED_URI_REGEXP =
  /^(?:(?:https?|mailto|ftp|tel):|[^a-z]|[a-z+.]+(?:[^a-z+.:]|$))/i;

/**
 * 使用 DOMPurify 白名单策略清洗 HTML。
 * 用于 AI 消息渲染：Markdown → HTML → sanitize → dangerouslySetInnerHTML
 */
export function sanitizeHtml(html: string): string {
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    ALLOWED_URI_REGEXP,
    FORBID_TAGS,
    FORBID_ATTR,
    ALLOW_DATA_ATTR: false,
    ALLOW_UNKNOWN_PROTOCOLS: false,
  });
}

/**
 * 注册全局 DOMPurify hook：对带有 target 属性的 <a> 标签自动追加 rel="noopener noreferrer"，
 * 防止 tab-nabbing 攻击。
 */
DOMPurify.addHook("uponSanitizeElement", (node, data) => {
  if (data.tagName === "a" && node instanceof HTMLAnchorElement) {
    const target = node.getAttribute("target");
    if (target) {
      const existingRel = node.getAttribute("rel") ?? "";
      const rels = existingRel
        .split(/\s+/)
        .map((r) => r.trim())
        .filter(Boolean);
      if (!rels.includes("noopener")) rels.push("noopener");
      if (!rels.includes("noreferrer")) rels.push("noreferrer");
      node.setAttribute("rel", rels.join(" "));
    }
  }
});
