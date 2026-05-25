import { marked } from "marked";
import TurndownService from "turndown";
import * as turndownPluginGfm from "turndown-plugin-gfm";

/**
 * Markdown ↔ HTML 往返（编辑器加载/保存）。
 * TipTap schema 支持范围见 `components/editor/gfm-schema.ts`（核心 GFM，非完整）。
 */
const turndown = new TurndownService({
  headingStyle: "atx",
  codeBlockStyle: "fenced",
  bulletListMarker: "-",
  hr: "---",
});
turndown.use(turndownPluginGfm.gfm);

// Wiki-link: convert <span data-wiki-link data-wiki-title="X"> to [[X]]
turndown.addRule("wikiLink", {
  filter: (node) =>
    node instanceof HTMLElement && node.hasAttribute("data-wiki-link"),
  replacement: (_content, node) => {
    const el = node as HTMLElement;
    const title = el.getAttribute("data-wiki-title") ?? "";
    return `[[${title}]]`;
  },
});

marked.setOptions({ gfm: true, breaks: true });

/** Parse Markdown string to HTML for TipTap initial content. */
export function markdownToHtml(md: string): string {
  return marked.parse(md, { async: false }) as string;
}

/** Serialize editor HTML to Markdown. */
export function htmlToMarkdown(html: string): string {
  return turndown.turndown(html);
}

/** Wrap HTML content in a self-contained page with paper-ink styles. */
export function markdownToHtmlPage(md: string, title?: string): string {
  const body = markdownToHtml(md);
  const cleanTitle = title ?? "Iris Note";
  return `<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>${cleanTitle}</title>
<style>
  body { max-width: 65ch; margin: 2rem auto; padding: 0 1rem; font-family: "Noto Serif SC", "Source Han Serif SC", Georgia, serif; font-size: 1.0625rem; line-height: 1.75; color: #2c2926; background: #f4f0e8; }
  h1 { font-size: 1.875rem; margin: 2rem 0 1rem; }
  h2 { font-size: 1.5rem; margin: 1.5rem 0 0.75rem; }
  h3 { font-size: 1.25rem; margin: 1.25rem 0 0.5rem; }
  pre { background: #1c1917; color: #e5e5e5; padding: 1rem; border-radius: 0.375rem; overflow-x: auto; }
  code { background: #e8e4dc; padding: 0.125rem 0.25rem; border-radius: 0.25rem; font-size: 0.9em; }
  pre code { background: none; padding: 0; }
  blockquote { border-left: 2px solid #b8956a; padding-left: 1rem; font-style: italic; color: #6b5e4f; }
  table { border-collapse: collapse; width: 100%; }
  th, td { border: 1px solid #d4c9b8; padding: 0.5rem; text-align: left; }
  a { color: #8b6914; }
  hr { border: none; border-top: 1px solid #d4c9b8; margin: 2rem 0; }
</style>
</head>
<body>${body}</body>
</html>`;
}

/** Round-trip for tests: md → html → md */
export function markdownRoundTrip(md: string): string {
  return htmlToMarkdown(markdownToHtml(md))
    .replace(/\\\[/g, "[")
    .replace(/\\\]/g, "]");
}
