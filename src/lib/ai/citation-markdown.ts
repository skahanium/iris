import type { ContextPacket } from "@/types/ai";

/** Markdown 内联引用 → 可安全渲染的 hash 链接（DOMPurify 白名单友好） */
const IRIS_CITE_PREFIX = "#iris-cite-";

const BAD_CITATION_LINK =
  /\[(citation:\d+)\]\((?:citation:\d+|iris-cite:[^)]*)\)/gi;

/** 尚未成链接的方括号引用（含 citation:N、[C1]、纯数字等） */
const BARE_CITATION = /(?<!\\)\[(citation:\d+|[CTFAWL]\d+|\d+)\](?!\()/gi;

const ALREADY_LINKIFIED =
  /#iris-cite-|\\\[(?:citation:\d+|[CTFAWL]\d+|\d+)\\\]/i;

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

/**
 * 将 `[citation:3]`、`[3]`、`[C1]` 等转为 Markdown 链接，避免 `citation:` 协议被清洗。
 */
export function linkifyAiCitations(markdown: string): string {
  if (ALREADY_LINKIFIED.test(markdown)) {
    return markdown;
  }
  let text = markdown.replace(BAD_CITATION_LINK, (_, label: string) => {
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

export function findPacketByCitationRef(
  ref: string,
  packets: ContextPacket[],
): ContextPacket | undefined {
  if (packets.length === 0) return undefined;

  const trimmed = ref.trim();
  const bracketed = trimmed.startsWith("[") ? trimmed : `[${trimmed}]`;
  const inner = bracketed.replace(/^\[|\]$/g, "");

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

  const citeIndex = /^citation:(\d+)$/i.exec(trimmed);
  if (citeIndex) {
    const idx = Number(citeIndex[1]) - 1;
    if (idx >= 0 && idx < packets.length) return packets[idx];
  }

  if (/^\d+$/.test(inner)) {
    const idx = Number(inner) - 1;
    if (idx >= 0 && idx < packets.length) return packets[idx];
  }

  return undefined;
}
