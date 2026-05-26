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
  body { max-width: 42rem; margin: clamp(2rem, 6vh, 3rem) auto; padding: 0 clamp(1.25rem, 4vw, 2rem); font-family: "Noto Serif SC", "Source Han Serif SC", Georgia, serif; font-size: 1.0625rem; line-height: 1.65; letter-spacing: 0.012em; color: #1a1c20; background: #fafaf9; }
  h1 { font-size: 1.75rem; margin: 2.5rem 0 1.25rem; line-height: 1.25; }
  h2 { font-size: 1.375rem; margin: 2rem 0 1rem; line-height: 1.3; }
  h3 { font-size: 1.125rem; margin: 1.5rem 0 0.75rem; }
  p { margin-bottom: 1.15em; }
  pre { background: #f0f1f3; color: #1a1c20; padding: 1rem; border-radius: 0.5rem; overflow-x: auto; border: 1px solid #e4e6ea; }
  code { background: #f0f1f3; padding: 0.125rem 0.375rem; border-radius: 0.25rem; font-size: 0.88em; }
  pre code { background: none; padding: 0; }
  blockquote { border-left: 3px solid #9a7b5a; padding-left: 1rem; color: #5c6068; margin: 1.25rem 0; }
  table { border-collapse: collapse; width: 100%; }
  th, td { border: 1px solid #e4e6ea; padding: 0.5rem; text-align: left; }
  a { color: #7a5c38; }
  hr { border: none; border-top: 1px solid #e4e6ea; margin: 2.5rem 0; }
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
