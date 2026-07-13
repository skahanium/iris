/** Markdown 閸愬懓浠堝鏇犳暏 閳?閸欘垰鐣ㄩ崗銊﹁閺屾挾娈?hash 闁剧偓甯撮敍鍦朞MPurify 閻ц棄鎮曢崡鏇炲几婵傛枻绱?*/
const IRIS_CITE_PREFIX = "#iris-cite-";

const BAD_CITATION_LINK =
  /\[(citation:\d+)\]\((?:citation:\d+|iris-cite:[^)]*)\)/gi;

/** 鐏忔碍婀幋鎰版懠閹恒儳娈戦弬瑙勫閸欏嘲绱╅悽顭掔礄閸?citation:N閵嗕箷C1]閵嗕胶鍑介弫鏉跨摟缁涘绱?*/
const BARE_CITATION = /(?<!\\)\[(citation:\d+|[CTFAWL]\d+|\d+)\](?!\()/gi;

/** 濡€崇€锋径宥堫嚢閻ㄥ嫯绻冩惔锕佹祮娑斿绱╅悽銊╂懠閹恒儻绱濇俊?`[\\[citation:4\\]](#iris-cite-...)` */
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

/** 闁剧偓甯撮弬鍥攳娣囨繄鏆€閺傝瀚崣鍑ょ礉濞撳弶鐓嬫稉鍝勫讲閻愮懓鍤惃?`[citation:3]` */
export function citationMarkdownLink(label: string): string {
  return `[\\[${label}\\]](${citationHrefForLabel(label)})`;
}

/** 鐏忓棜绻冩惔锕佹祮娑斿娈戝鏇犳暏闁剧偓甯撮幁銏狀槻娑?marked 閸欘垵袙閺嬫劗娈戦弽鍥у櫙瑜般垹绱?*/
export function repairOverEscapedCitationLinks(markdown: string): string {
  return markdown.replace(OVER_ESCAPED_CITE_LINK, (_, label: string) => {
    return citationMarkdownLink(label);
  });
}

/**
 * 鐏?`[citation:3]`閵嗕梗[3]`閵嗕梗[C1]` 缁涘娴嗘稉?Markdown 闁剧偓甯撮敍宀勪缉閸?`citation:` 閸楀繗顔呯悮顐ｇ濞叉ぜ鈧? */
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

/** 娑?marked 鏉堟挸鍤惃鍕穿閻劑鎽奸幒銉ㄋ夋稉?class閿涘奔绌舵禍搴㈢壉瀵繋绗岄悙鐟板毊鐠囧棗鍩?*/
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
