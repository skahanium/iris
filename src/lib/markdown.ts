import { marked } from "marked";
import TurndownService from "turndown";
import * as turndownPluginGfm from "turndown-plugin-gfm";

import {
  splitFrontmatter,
  serializeNoteMarkdown,
  titleFromFields,
} from "@/lib/frontmatter";

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

// Iris doc title is stored in frontmatter, not body markdown
turndown.addRule("irisDocTitle", {
  filter: (node) =>
    node instanceof HTMLElement &&
    node.tagName === "H1" &&
    node.classList.contains("iris-doc-title"),
  replacement: () => "",
});

marked.setOptions({ gfm: true, breaks: true });

/**
 * 正文开头的 ATX `# 标题` 若与 frontmatter 主标题相同则去掉，避免编辑器出现两个标题。
 */
export function stripLeadingBodyTitleHeading(
  body: string,
  title: string,
): string {
  const trimmedTitle = title.trim();
  if (!trimmedTitle) return body;
  const normalized = body.trimStart();
  const match = /^#\s+(.+?)\s*(?:\n|$)/.exec(normalized);
  if (!match || match[1]!.trim() !== trimmedTitle) return body;
  return normalized.slice(match[0].length).trimStart();
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** Parse Markdown string to HTML for TipTap initial content (body only, legacy). */
export function markdownToHtml(md: string): string {
  return marked.parse(md, { async: false }) as string;
}

/**
 * Parse full note markdown → editor HTML (noteTitle h1 + body).
 * `titleFallback` used when frontmatter has no title (e.g. filename stem).
 */
export function markdownToEditorHtml(
  md: string,
  titleFallback = "",
): string {
  const { fields, body: rawBody } = splitFrontmatter(md);
  let title = titleFromFields(fields);
  let body = rawBody;

  if (!title) {
    const legacy = /^#\s+(.+?)\s*(?:\n|$)/.exec(body);
    if (legacy) {
      title = legacy[1]!.trim();
      body = body.slice(legacy[0].length).trimStart();
    } else {
      title = titleFallback.trim();
    }
  } else {
    body = stripLeadingBodyTitleHeading(body, title);
  }

  const titleHtml = `<h1 class="iris-doc-title">${escapeHtml(title)}</h1>`;
  const bodyTrimmed = body.trim();
  const bodyHtml = bodyTrimmed ? markdownToHtml(bodyTrimmed) : "<p></p>";
  return `${titleHtml}${bodyHtml}`;
}

/** Extract raw frontmatter YAML from note markdown (for round-trip preservation). */
export function extractFrontmatterYaml(md: string): string | null {
  return splitFrontmatter(md).yaml;
}

/** Serialize editor HTML + preserved frontmatter → full note markdown. */
export function editorHtmlToMarkdown(
  html: string,
  existingYaml: string | null,
): string {
  const doc = new DOMParser().parseFromString(
    `<div>${html}</div>`,
    "text/html",
  );
  const root = doc.body.firstElementChild;
  if (!root) {
    return serializeNoteMarkdown(existingYaml, "", "");
  }

  const titleEl = root.querySelector("h1.iris-doc-title");
  const title = titleEl?.textContent?.trim() ?? "";
  titleEl?.remove();

  const duplicateBodyH1 = root.querySelector(
    ":scope > h1:not(.iris-doc-title)",
  );
  if (
    duplicateBodyH1 &&
    title &&
    duplicateBodyH1.textContent?.trim() === title
  ) {
    duplicateBodyH1.remove();
  }

  const bodyHtml = root.innerHTML.trim();
  let bodyMd = bodyHtml ? turndown.turndown(bodyHtml) : "";
  bodyMd = stripLeadingBodyTitleHeading(bodyMd, title);
  return serializeNoteMarkdown(existingYaml, title, bodyMd);
}

/** Serialize editor HTML to Markdown (body only, legacy). */
export function htmlToMarkdown(html: string): string {
  return turndown.turndown(html);
}

/** Wrap HTML content in a self-contained page with paper-ink styles. */
export function markdownToHtmlPage(md: string, title?: string): string {
  const { fields, body } = splitFrontmatter(md);
  const docTitle = (title ?? titleFromFields(fields)) || "Iris Note";
  const bodyHtml = markdownToHtml(body);
  const cleanTitle = escapeHtml(docTitle);
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
<body>${bodyHtml}</body>
</html>`;
}

/** Round-trip for tests: md → html → md (body-only legacy). */
export function markdownRoundTrip(md: string): string {
  return htmlToMarkdown(markdownToHtml(md))
    .replace(/\\\[/g, "[")
    .replace(/\\\]/g, "]");
}

/** Round-trip for Iris notes with frontmatter title. */
export function noteMarkdownRoundTrip(
  md: string,
  titleFallback = "",
): string {
  const yaml = extractFrontmatterYaml(md);
  const html = markdownToEditorHtml(md, titleFallback);
  return editorHtmlToMarkdown(html, yaml);
}
