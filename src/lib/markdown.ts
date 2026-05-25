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

marked.setOptions({ gfm: true, breaks: true });

/** Parse Markdown string to HTML for TipTap initial content. */
export function markdownToHtml(md: string): string {
  return marked.parse(md, { async: false }) as string;
}

/** Serialize editor HTML to Markdown. */
export function htmlToMarkdown(html: string): string {
  return turndown.turndown(html);
}

/** Round-trip for tests: md → html → md */
export function markdownRoundTrip(md: string): string {
  return htmlToMarkdown(markdownToHtml(md));
}
