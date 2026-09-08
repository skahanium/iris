//! Source offsets, rather than token counts, identify continuous Markdown blocks.
import { Lexer, type Token, type TokensList } from "marked";
import { proseMarked } from "@/lib/markdown-render";

export interface StreamingMarkdownBlock {
  start: number;
  end: number;
  source: string;
  type: string;
  confirmed: boolean;
}
export interface StreamingMarkdownDocument {
  source: string;
  blocks: StreamingMarkdownBlock[];
  definitions: string;
  /** Only definitions preceding the frozen source offset survive a suffix re-lex. */
  confirmedLinks?: TokensList["links"];
}
export interface StreamingMarkdownSplit {
  stableMarkdown: string;
  tailMarkdown: string;
  stableBlockCount: number;
}

function closedFence(raw: string): boolean {
  const opener = /^ {0,3}(`{3,}|~{3,})[^\n]*\n/.exec(raw);
  if (!opener) return false;
  const fence = opener[1]!;
  return new RegExp(
    `^ {0,3}${fence[0]}{${fence.length},}[ \\t]*(?:\\n|$)`,
    "m",
  ).test(raw.slice(opener[0].length));
}
function complete(token: Token, trailing: string): boolean {
  if (token.type === "code")
    return token.raw.endsWith("\n") && closedFence(token.raw);
  if (token.type === "heading" || token.type === "hr")
    return token.raw.endsWith("\n");
  // Lists, blockquotes and tables can absorb later lines across blank lines.
  return token.type === "paragraph" && /\n[ \t]*\n/.test(trailing);
}

/** Skip omitted source with marked's own grammar before locating the next token. */
function skipNonRenderingSource(source: string, from: number): number {
  let offset = from;
  while (offset < source.length) {
    const rest = source.slice(offset);
    const raw =
      Lexer.rules.block.gfm.newline.exec(rest)?.[0] ||
      Lexer.rules.block.gfm.def.exec(rest)?.[0];
    if (!raw) break;
    offset += raw.length;
  }
  return offset;
}

/** Re-lex only the active suffix. Confirmed blocks keep their source identity. */
export function updateStreamingMarkdown(
  source: string,
  previous?: StreamingMarkdownDocument,
): StreamingMarkdownDocument {
  const active = previous?.blocks.at(-1);
  if (
    previous &&
    active &&
    !active.confirmed &&
    active.source.length > 8192 &&
    active.end === previous.source.length &&
    !previous.source.endsWith("\n") &&
    source.startsWith(previous.source) &&
    !source.slice(previous.source.length).includes("\n")
  ) {
    return {
      ...previous,
      source,
      blocks: [
        ...previous.blocks.slice(0, -1),
        { ...active, source: source.slice(active.start), end: source.length },
      ],
    };
  }
  const prefix =
    previous && source.startsWith(previous.source)
      ? previous.blocks.filter((block) => block.confirmed)
      : [];
  const offset = prefix.at(-1)?.end ?? 0;
  const suffix = source.slice(offset);
  const tokens = proseMarked.lexer(suffix);
  const blocks: StreamingMarkdownBlock[] = [...prefix];
  let cursor = 0;
  const contentTokens = tokens.filter((token) => token.type !== "space");
  contentTokens.forEach((token, index) => {
    const rawStart = suffix.indexOf(
      token.raw,
      skipNonRenderingSource(suffix, cursor),
    );
    if (rawStart < 0) return;
    const end = rawStart + token.raw.length;
    const next = contentTokens[index + 1];
    const nextStart = next
      ? suffix.indexOf(next.raw, skipNonRenderingSource(suffix, end))
      : suffix.length;
    // marked omits reference definitions from its token array. Their gap must
    // not extend a rendered block's frozen offset, even if another token follows.
    const gapEnd = nextStart < 0 ? end : nextStart;
    const whitespace =
      /^[ \t\r\n]*/.exec(suffix.slice(end, gapEnd))?.[0].length ?? 0;
    const blockEnd = end + whitespace;
    blocks.push({
      start: offset + rawStart,
      end: offset + blockEnd,
      source: token.raw,
      type: token.type,
      confirmed: !!next || complete(token, suffix.slice(end - 1, blockEnd)),
    });
    cursor = blockEnd;
  });
  const confirmedEnd =
    blocks.filter((block) => block.confirmed).at(-1)?.end ?? offset;
  // Only newly confirmed source is inspected here; never re-lex the old prefix.
  // Provisional links remain replaceable when the next delta completes a URL/title.
  const newlyConfirmed =
    confirmedEnd > offset
      ? proseMarked.lexer(source.slice(offset, confirmedEnd)).links
      : {};
  const confirmedLinks = Object.assign(
    Object.create(null) as TokensList["links"],
    prefix.length ? previous?.confirmedLinks : undefined,
  );
  for (const [label, link] of Object.entries(newlyConfirmed)) {
    if (!Object.hasOwn(confirmedLinks, label)) confirmedLinks[label] = link;
  }
  // First definition wins, including definitions frozen in an earlier suffix.
  const links = Object.assign(
    Object.create(null) as TokensList["links"],
    confirmedLinks,
  );
  for (const [label, link] of Object.entries(tokens.links)) {
    if (!Object.hasOwn(links, label)) links[label] = link;
  }
  const definitions = Object.entries(links)
    .map(
      ([label, link]) =>
        `[${label}]: <${link.href}>${link.title ? ` ${JSON.stringify(link.title)}` : ""}`,
    )
    .join("\n");
  return { source, blocks, definitions, confirmedLinks };
}

export function splitStreamingMarkdown(
  content: string,
): StreamingMarkdownSplit {
  const document = updateStreamingMarkdown(content);
  const stable = document.blocks.filter((block) => block.confirmed);
  const end = stable.at(-1)?.end ?? 0;
  return {
    stableMarkdown: content.slice(0, end),
    tailMarkdown: content.slice(end),
    stableBlockCount: stable.length,
  };
}

/** Patch sanitized nodes in place so growing headings, lists and tables keep their containers. */
export function reconcileMarkdownChildren(parent: Node, next: Node): void {
  const desired = Array.from(next.childNodes);
  desired.forEach((node, index) => {
    const current = parent.childNodes[index];
    if (
      !current ||
      current.nodeType !== node.nodeType ||
      current.nodeName !== node.nodeName
    ) {
      if (current) parent.replaceChild(node.cloneNode(true), current);
      else parent.appendChild(node.cloneNode(true));
    } else if (node.nodeType === Node.TEXT_NODE) {
      if (current.nodeValue !== node.nodeValue)
        current.nodeValue = node.nodeValue;
    } else if (current instanceof Element && node instanceof Element) {
      for (const attribute of Array.from(current.attributes)) {
        if (!node.hasAttribute(attribute.name))
          current.removeAttribute(attribute.name);
      }
      for (const attribute of Array.from(node.attributes)) {
        if (current.getAttribute(attribute.name) !== attribute.value)
          current.setAttribute(attribute.name, attribute.value);
      }
      reconcileMarkdownChildren(current, node);
    }
  });
  while (parent.childNodes.length > desired.length) parent.lastChild?.remove();
}
