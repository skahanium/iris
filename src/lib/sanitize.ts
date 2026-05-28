import DOMPurify from "dompurify";

/** 白名单标签：Markdown 渲染后需要的元素 */
const ALLOWED_TAGS = [
  // 块级元素
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
  // 表格
  "table",
  "thead",
  "tbody",
  "tr",
  "th",
  "td",
  // 内联元素
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
  // 任务列表
  "input",
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
  "type",
  "checked",
  "disabled",
  "start",
  "value",
  "target",
  "rel",
  "data-cite-ref",
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
    // 禁止 data: URI（图片除外，由 ALLOWED_URI_REGEXP 控制）
    ALLOW_DATA_ATTR: false,
    // 保留 HTML 实体
    ALLOW_UNKNOWN_PROTOCOLS: false,
  });
}
