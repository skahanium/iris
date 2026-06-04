import type { ContextPacket } from "@/types/ai";

/** Markdown 内联引用 → 可安全渲染的 hash 链接（DOMPurify 白名单友好） */
const IRIS_CITE_PREFIX = "#iris-cite-";

const BAD_CITATION_LINK =
  /\[(citation:\d+)\]\((?:citation:\d+|iris-cite:[^)]*)\)/gi;

/** 尚未成链接的方括号引用（含 citation:N、[C1]、纯数字等） */
const BARE_CITATION = /(?<!\\)\[(citation:\d+|[CTFAWL]\d+|\d+)\](?!\()/gi;

/** 模型复读的过度转义引用链接，如 `[\\[citation:4\\]](#iris-cite-...)` */
const OVER_ESCAPED_CITE_LINK =
  /\[(?:\\+)+\[(citation:\d+)\](?:\\+)+\]\((#iris-cite-[^)]+)\)/gi;

export function encodeCitationRef(label: string): string {
  return encodeURIComponent(label);
}

export function decodeCitationHref(href: string): string | null {
  if (!href.startsWith(IRIS_CITE_PREFIX)) return null;
  try {
    return decodeURIComponent(href.slice(IRIS_CITE_PREFIX.length));
  } catch {
    return null;
  }
}

export function citationHrefForLabel(label: string): string {
  return `${IRIS_CITE_PREFIX}${encodeCitationRef(label)}`;
}

/** 链接文案保留方括号，渲染为可点击的 `[citation:3]` */
export function citationMarkdownLink(label: string): string {
  return `[\\[${label}\\]](${citationHrefForLabel(label)})`;
}

/** 将过度转义的引用链接恢复为 marked 可解析的标准形式 */
export function repairOverEscapedCitationLinks(markdown: string): string {
  return markdown.replace(OVER_ESCAPED_CITE_LINK, (_, label: string) => {
    return citationMarkdownLink(label);
  });
}

/**
 * 将 `[citation:3]`、`[3]`、`[C1]` 等转为 Markdown 链接，避免 `citation:` 协议被清洗。
 */
export function linkifyAiCitations(markdown: string): string {
  let text = repairOverEscapedCitationLinks(markdown);
  text = text.replace(BAD_CITATION_LINK, (_, label: string) => {
    return citationMarkdownLink(label);
  });
  text = text.replace(BARE_CITATION, (_full, label: string) => {
    return citationMarkdownLink(label);
  });
  return text;
}

/** 为 marked 输出的引用链接补上 class，便于样式与点击识别 */
export function tagCitationLinksInHtml(html: string): string {
  return html.replace(
    /href="(#iris-cite-[^"]+)"/g,
    (_, href: string) =>
      `href="${href}" class="ai-citation" data-cite-ref="${href.slice(IRIS_CITE_PREFIX.length)}"`,
  );
}

const BAD_CITATION_LINK_HTML =
  /\[((?:citation:\d+|[CTFAWL]\d+|\d+))\]\((?:citation:\d+|iris-cite:[^)]*)\)/gi;

const BARE_CITATION_IN_TEXT =
  /(?<!\\)\[(citation:\d+|[CTFAWL]\d+|\d+)\](?!\()/gi;

function citationHtmlAnchor(label: string): string {
  const href = citationHrefForLabel(label);
  const display = label.startsWith("citation:") ? `[${label}]` : `[${label}]`;
  return `<a href="${href}" class="ai-citation" data-cite-ref="${encodeCitationRef(label)}">${display}</a>`;
}

function linkifyCitationsInTextSegment(text: string): string {
  let out = text.replace(BAD_CITATION_LINK_HTML, (_, label: string) =>
    citationHtmlAnchor(label),
  );
  out = out.replace(BARE_CITATION_IN_TEXT, (_full, label: string) =>
    citationHtmlAnchor(label),
  );
  return out;
}

/**
 * Post-markdown citation linkification (avoids breaking `**bold**` and other MD syntax).
 */
export function postProcessCitations(html: string): string {
  const parts = html.split(/(<[^>]+>)/g);
  const linked = parts.map((part, index) => {
    if (index % 2 === 1) return part;
    return linkifyCitationsInTextSegment(part);
  });
  return tagCitationLinksInHtml(linked.join(""));
}

export function findPacketByCitationRef(
  ref: string,
  packets: ContextPacket[],
): ContextPacket | undefined {
  if (packets.length === 0) return undefined;

  const trimmed = ref.trim();
  const bracketed = trimmed.startsWith("[") ? trimmed : `[${trimmed}]`;
  const inner = bracketed.replace(/^\[|\]$/g, "");

  // 1) Direct label match
  const byLabel = packets.find((p) => {
    const label = p.citation_label;
    const labelInner = label.replace(/^\[|\]$/g, "");
    return (
      label === bracketed ||
      label === trimmed ||
      labelInner === inner ||
      labelInner === trimmed ||
      `citation:${labelInner}` === trimmed ||
      `citation:${labelInner}` === inner
    );
  });
  if (byLabel) return byLabel;

  // 2) citation:N position-based (1-indexed → 0-indexed)
  const citeIndex = /^citation:(\d+)$/i.exec(trimmed);
  if (citeIndex) {
    const n = Number(citeIndex[1]);
    // Try numeric match against packet label digits first
    const numericMatch = packets.find((p) => {
      const digits = p.citation_label.replace(/\D/g, "");
      return digits === String(n);
    });
    if (numericMatch) return numericMatch;
    // Fall back to position
    const idx = n - 1;
    if (idx >= 0 && idx < packets.length) return packets[idx];
  }

  // 3) Pure digit ref (e.g. "3" or "[3]")
  if (/^\d+$/.test(inner)) {
    const n = Number(inner);
    const numericMatch = packets.find((p) => {
      const digits = p.citation_label.replace(/\D/g, "");
      return digits === String(n);
    });
    if (numericMatch) return numericMatch;
    const idx = n - 1;
    if (idx >= 0 && idx < packets.length) return packets[idx];
  }

  // 4) Letter+digit ref (e.g. "W0", "[W1]", "citation:W2")
  const alphaNum = /^citation:([A-Za-z]\d+)$/i.exec(trimmed);
  const bareAlphaNum = alphaNum
    ? alphaNum[1]
    : /^([A-Za-z]\d+)$/.exec(inner)?.[1];
  if (bareAlphaNum) {
    const byAlpha = packets.find((p) => {
      const labelInner = p.citation_label.replace(/^\[|\]$/g, "");
      return labelInner.toUpperCase() === bareAlphaNum.toUpperCase();
    });
    if (byAlpha) return byAlpha;
  }

  return undefined;
}
