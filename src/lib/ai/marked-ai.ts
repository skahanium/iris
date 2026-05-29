import { Marked } from "marked";
import { common, createLowlight } from "lowlight";

const lowlight = createLowlight(common);

/**
 * Convert a HAST node tree (from lowlight) to an HTML string.
 * lowlight.highlight() / highlightAuto() return a HAST Root:
 *   { type: "root", children: [...], data: {...} }
 * Each child is either:
 *   { type: "text", value: string }
 *   { type: "element", tagName: string, properties: { className?: string[] },
 *     children: [...] }
 */
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

function escapeText(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/**
 * Standalone marked instance for AI message rendering.
 *
 * Does NOT mutate the global marked defaults (used by the editor pipeline).
 * Uses lowlight for syntax-highlighted code blocks (same engine as TipTap).
 */
export const aiMarked = new Marked({
  gfm: true,
  breaks: true,
  hooks: {
    /**
     * Post-process: wrap <table> in <div class="ai-table-wrap"> for
     * mobile-friendly horizontal scrolling.
     */
    postprocess(html: string): string {
      return html
        .replace(/<table>/g, '<div class="ai-table-wrap"><table>')
        .replace(/<\/table>/g, "</table></div>");
    },
  },
  renderer: {
    /**
     * Code block: syntax-highlighted HTML via lowlight.
     * Falls back to escaped plain text if highlighting fails.
     */
    code({ text, lang }: { text: string; lang?: string }): string {
      const language = lang || "";
      try {
        let highlightedHtml: string;
        if (language && lowlight.registered(language)) {
          const root = lowlight.highlight(language, text);
          highlightedHtml = hastToHtml(root);
        } else {
          const root = lowlight.highlightAuto(text);
          highlightedHtml = hastToHtml(root);
        }

        // highlightAuto may return empty for very short text.
        // Fall back to plain escaped text in that case.
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
        // Highlighting failed — fall back to plain escaped code block
        const escaped = escapeText(text);
        const langAttr = language ? ` class="language-${language}"` : "";
        return `<pre><code${langAttr}>${escaped}</code></pre>`;
      }
    },

    /**
     * Links: external links open in new tab. Citation links left unchanged.
     */
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

    /**
     * Task list item: outputs a disabled checkbox input.
     */
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
