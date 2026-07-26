/** Markdown citation helpers → hash links that survive DOMPurify. */
const IRIS_CITE_PREFIX = "#iris-cite-";

const BAD_CITATION_LINK =
  /\[(citation:\d+)\]\((?:citation:\d+|iris-cite:[^)]*)\)/gi;

/** Bare markers: `[citation:N]`, `[C1]`, `[W1]`, `[1]`, and Unicode superscript forms `[¹]`. */
const BARE_CITATION =
  /(?<!\\)\[(citation:\d+|[CTFAWL]\d+|\d+|[\u2070\u00B9\u00B2\u00B3\u2074-\u2079]+)\](?!\()/gi;

/** Over-escaped citation links like `[\\[citation:4\\]](#iris-cite-...)`. */
const OVER_ESCAPED_CITE_LINK =
  /\[(?:\\+)+\[(citation:\d+)\](?:\\+)+\]\((#iris-cite-[^)]+)\)/gi;

const SUPERSCRIPT_DIGIT: Record<string, string> = {
  "\u2070": "0",
  "\u00B9": "1",
  "\u00B2": "2",
  "\u00B3": "3",
  "\u2074": "4",
  "\u2075": "5",
  "\u2076": "6",
  "\u2077": "7",
  "\u2078": "8",
  "\u2079": "9",
};

/** Normalize Unicode superscript digits in a citation label to ASCII. */
export function normalizeCitationLabel(label: string): string {
  return Array.from(label, (ch) => SUPERSCRIPT_DIGIT[ch] ?? ch).join("");
}

/** Visible footnote digit for inline badges (no brackets or citation: prefix). */
export function formatCitationDisplayLabel(label: string): string {
  const normalized = normalizeCitationLabel(label.trim());
  if (normalized.startsWith("citation:")) {
    return normalized.slice("citation:".length);
  }
  const prefixed = normalized.match(/^[CTFAWL](\d+)$/i);
  if (prefixed) {
    return prefixed[1] ?? normalized;
  }
  return normalized;
}

export function encodeCitationRef(label: string): string {
  return encodeURIComponent(normalizeCitationLabel(label));
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

/** Markdown link with a clean `[label]` visible text (no double-escaped brackets). */
export function citationMarkdownLink(label: string): string {
  const normalized = normalizeCitationLabel(label);
  const display = formatCitationDisplayLabel(normalized);
  return `[${display}](${citationHrefForLabel(normalized)})`;
}

/** Repair over-escaped citation links before markdown rendering. */
export function repairOverEscapedCitationLinks(markdown: string): string {
  return markdown.replace(OVER_ESCAPED_CITE_LINK, (_, label: string) => {
    return citationMarkdownLink(label);
  });
}

/**
 * Turn bare `[citation:3]` / `[3]` / `[¹]` into Markdown links.
 * Does not re-linkify already-linked citations.
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

/** Tag iris-cite anchors with class after marked. */
export function tagCitationLinksInHtml(html: string): string {
  return html.replace(
    /href="(#iris-cite-[^"]+)"/g,
    (_, href: string) =>
      `href="${href}" class="ai-citation" data-cite-ref="${href.slice(IRIS_CITE_PREFIX.length)}"`,
  );
}

const BAD_CITATION_LINK_HTML =
  /\[((?:citation:\d+|[CTFAWL]\d+|\d+|[\u2070\u00B9\u00B2\u00B3\u2074-\u2079]+))\]\((?:citation:\d+|iris-cite:[^)]*)\)/gi;

const BARE_CITATION_IN_TEXT =
  /(?<!\\)\[(citation:\d+|[CTFAWL]\d+|\d+|[\u2070\u00B9\u00B2\u00B3\u2074-\u2079]+)\](?!\()/gi;

function citationHtmlAnchor(label: string): string {
  const normalized = normalizeCitationLabel(label);
  const href = citationHrefForLabel(normalized);
  const display = formatCitationDisplayLabel(normalized);
  const aria = `引用来源 ${display}`;
  return `<sup class="ai-citation-wrap"><a href="${href}" class="ai-citation" data-cite-ref="${encodeCitationRef(normalized)}" aria-label="${aria}">${display}</a></sup>`;
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

/** True when an href should open in the system browser. */
export function isExternalHttpsHref(href: string | null | undefined): boolean {
  if (!href) return false;
  try {
    const parsed = new URL(href);
    return parsed.protocol === "https:";
  } catch {
    return false;
  }
}
