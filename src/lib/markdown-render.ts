import { postProcessCitations } from "@/lib/ai/citation-markdown";
import { Marked } from "marked";
import { common, createLowlight } from "lowlight";

const lowlight = createLowlight(common);

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
 * Close unbalanced Markdown fences and inline marks so streaming partial content parses cleanly.
 */
export function repairStreamingMarkdown(md: string): string {
  let repaired = md;
  const fenceMatches = repaired.match(/```/g);
  if (fenceMatches && fenceMatches.length % 2 !== 0) {
    repaired += "\n```";
  }
  if (countDelimiter(repaired, "**") % 2 !== 0) {
    repaired += "**";
  }
  if (countDelimiter(repaired, "~~") % 2 !== 0) {
    repaired += "~~";
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

    listitem({
      text,
      task,
      checked,
    }: {
      text: string;
      task: boolean;
      checked?: boolean;
    }): string {
      if (task) {
        const checkedAttr = checked ? " checked" : "";
        return `<li class="task-list-item"><input type="checkbox" disabled${checkedAttr} /> ${text}</li>`;
      }
      return `<li>${text}</li>`;
    },
  },
});

/** Parse Markdown to HTML for AI message bubbles. */
export function parseMarkdownToHtml(
  markdown: string,
  options?: { streaming?: boolean },
): string {
  const source = options?.streaming
    ? repairStreamingMarkdown(markdown)
    : markdown;
  return proseMarked.parse(source, { async: false }) as string;
}

/** Markdown → HTML with post-render citation linkification. */
export function renderAiMarkdownToHtml(
  markdown: string,
  options?: { streaming?: boolean },
): string {
  return postProcessCitations(parseMarkdownToHtml(markdown, options));
}
