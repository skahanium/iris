import { postProcessCitations } from "@/lib/ai/citation-markdown";
import { Marked, type Renderer, type Tokens } from "marked";
import { common, createLowlight } from "lowlight";

const lowlight = createLowlight(common);

interface AiMarkdownRenderOptions {
  streaming?: boolean;
  codeCopy?: boolean;
}

function escapeText(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function hastToHtml(node: unknown): string {
  if (!node || typeof node !== "object") return "";
  const n = node as Record<string, unknown>;

  if (n.type === "text") {
    return escapeText(String(n.value ?? ""));
  }

  if (n.type === "element") {
    const tag = String(n.tagName ?? "span");
    const props = n.properties as Record<string, unknown> | undefined;
    const cls = Array.isArray(props?.className)
      ? (props!.className as string[]).join(" ")
      : "";
    const classAttr = cls ? ` class="${cls}"` : "";
    const children = Array.isArray(n.children)
      ? n.children.map((c: unknown) => hastToHtml(c)).join("")
      : "";
    return `<${tag}${classAttr}>${children}</${tag}>`;
  }

  if (n.type === "root") {
    const children = Array.isArray(n.children)
      ? n.children.map((c: unknown) => hastToHtml(c)).join("")
      : "";
    return children;
  }

  return "";
}

/** Count occurrences of a literal delimiter (non-regex). */
function countDelimiter(text: string, delimiter: string): number {
  let count = 0;
  let pos = 0;
  while (pos < text.length) {
    const idx = text.indexOf(delimiter, pos);
    if (idx === -1) break;
    count += 1;
    pos = idx + delimiter.length;
  }
  return count;
}

/**
 * Count single `_` occurrences (underscore not part of `__`).
 */
function countSingleUnderscore(text: string): number {
  let count = 0;
  for (let i = 0; i < text.length; i++) {
    if (text[i] === "_" && text[i + 1] !== "_" && text[i - 1] !== "_") {
      count++;
    }
  }
  return count;
}

/**
 * Count single `*` occurrences (asterisk not part of `**`, not a list marker).
 */
function countSingleAsterisk(text: string): number {
  let count = 0;
  const lines = text.split("\n");
  for (const line of lines) {
    const trimmed = line.trimStart();
    // Skip if entire trimmed line starts with * followed by space (list marker)
    if (!trimmed) continue;
    for (let i = 0; i < line.length; i++) {
      if (line[i] === "*" && line[i + 1] !== "*" && line[i - 1] !== "*") {
        // Check if this is a list marker: at line start (after optional indent) and followed by space
        const prefix = line.slice(0, i);
        if (/^\s*$/.test(prefix) && line[i + 1] === " ") {
          // This is likely a list marker, skip
          continue;
        }
        count++;
      }
    }
  }
  return count;
}

function addCodeCopyButtons(html: string): string {
  return html.replace(/<pre><code\b[\s\S]*?<\/code><\/pre>/g, (block) => {
    return `<div class="ai-code-block"><button type="button" class="ai-code-copy-button" data-ai-code-copy aria-label="复制代码" title="复制代码">复制</button>${block}</div>`;
  });
}

function repairInlineDestinationAtLineEnd(markdown: string): string {
  return markdown.replace(
    /(!?\[[^\]\n]+\]\([^)\n\s]+)$/u,
    (match) => `${match})`,
  );
}

function repairObviousTableRowAtLineEnd(markdown: string): string {
  return markdown.replace(/(^|\n)([ \t]*\|[^\n|]+(?:\|[^\n|]+)+)$/u, "$1$2 |");
}

/**
 * Close unbalanced Markdown fences and inline marks so streaming partial
 * content parses cleanly. Also handles incomplete lists, blockquotes,
 * callouts, and footnotes.
 *
 * New in Phase 4:
 * - Unclosed italic (`_`, `*`)
 * - Mid-stream interrupted list items
 * - Mid-stream interrupted blockquote lines
 * - Mid-stream interrupted callout blocks
 * - Mid-stream interrupted footnote references
 */
export function repairStreamingMarkdown(md: string): string {
  let repaired = md;

  // ── remove incomplete structural elements first ────────────
  // These must be trimmed before delimiter balancing to avoid
  // clashes (e.g., list `-` vs italic `*` closers).

  // Incomplete list items (bullet: `- `, `* `)
  repaired = repaired.replace(/\n[ \t]*[-*]\s+$/m, "\n");

  // Incomplete ordered list items (`1. `, `42. `)
  repaired = repaired.replace(/\n[ \t]*\d+\.\s+$/m, "\n");

  // Incomplete blockquote lines (`> `, `>   `)
  repaired = repaired.replace(/\n[ \t]*>\s*$/m, "\n");

  repaired = repairInlineDestinationAtLineEnd(repaired);
  repaired = repairObviousTableRowAtLineEnd(repaired);

  // ── close unbalanced delimiters ────────────────────────────

  // Fences
  const fenceMatches = repaired.match(/```/g);
  if (fenceMatches && fenceMatches.length % 2 !== 0) {
    repaired += "\n```";
  }

  // Bold
  if (countDelimiter(repaired, "**") % 2 !== 0) {
    repaired += "**";
  }

  // Strikethrough
  if (countDelimiter(repaired, "~~") % 2 !== 0) {
    repaired += "~~";
  }

  // Italic (_)
  if (countSingleUnderscore(repaired) % 2 !== 0) {
    repaired += "_";
  }

  // Italic (*)
  if (countSingleAsterisk(repaired) % 2 !== 0) {
    repaired += "*";
  }

  // ── unterminated footnote reference ────────────────────────

  const lastOpen = repaired.lastIndexOf("[^");
  if (lastOpen !== -1) {
    const after = repaired.slice(lastOpen);
    if (!after.includes("]")) {
      repaired += "]";
    }
  }

  return repaired;
}

/**
 * Shared marked instance for AI + preview (editor load uses `lib/markdown.ts` global).
 */
export const proseMarked = new Marked({
  gfm: true,
  breaks: true,
  hooks: {
    postprocess(html: string): string {
      return html
        .replace(/<table>/g, '<div class="ai-table-wrap"><table>')
        .replace(/<\/table>/g, "</table></div>");
    },
  },
  renderer: {
    code({ text, lang }: { text: string; lang?: string }): string {
      const language = lang || "";
      try {
        let highlightedHtml: string;
        if (language && lowlight.registered(language)) {
          highlightedHtml = hastToHtml(lowlight.highlight(language, text));
        } else {
          highlightedHtml = hastToHtml(lowlight.highlightAuto(text));
        }

        if (!highlightedHtml.trim()) {
          const escaped = escapeText(text);
          const langAttr = language ? ` class="language-${language}"` : "";
          return `<pre><code${langAttr}>${escaped}</code></pre>`;
        }

        const langAttr = language
          ? ` class="hljs language-${language}"`
          : ' class="hljs"';
        return `<pre><code${langAttr}>${highlightedHtml}</code></pre>`;
      } catch {
        const escaped = escapeText(text);
        const langAttr = language ? ` class="language-${language}"` : "";
        return `<pre><code${langAttr}>${escaped}</code></pre>`;
      }
    },

    link({
      href,
      title,
      tokens,
    }: {
      href: string;
      title?: string | null;
      tokens: Array<{ type: string; text?: string; raw?: string }>;
    }): string {
      const text = tokens.map((t) => t.text ?? t.raw ?? "").join("");
      const titleAttr = title ? ` title="${title}"` : "";
      if (href.startsWith("#iris-cite-")) {
        return `<a href="${href}"${titleAttr}>${text}</a>`;
      }
      return `<a href="${href}"${titleAttr} target="_blank" rel="noopener noreferrer">${text}</a>`;
    },

    listitem(this: Renderer, item: Tokens.ListItem): string {
      let itemBody = "";
      if (item.task) {
        const checkedAttr = item.checked ? " checked" : "";
        itemBody += `<input type="checkbox" disabled${checkedAttr} /> `;
      }
      itemBody += this.parser.parse(item.tokens, !!item.loose);
      if (item.task) {
        return `<li class="task-list-item">${itemBody}</li>`;
      }
      return `<li>${itemBody}</li>`;
    },
  },
});

/** Parse Markdown to HTML for AI message bubbles. */
export function parseMarkdownToHtml(
  markdown: string,
  options?: AiMarkdownRenderOptions,
): string {
  const source = options?.streaming
    ? repairStreamingMarkdown(markdown)
    : markdown;
  const html = proseMarked.parse(source, { async: false }) as string;
  return options?.codeCopy ? addCodeCopyButtons(html) : html;
}

/** Markdown → HTML with post-render citation linkification. */
export function renderAiMarkdownToHtml(
  markdown: string,
  options?: AiMarkdownRenderOptions,
): string {
  return postProcessCitations(parseMarkdownToHtml(markdown, options));
}
