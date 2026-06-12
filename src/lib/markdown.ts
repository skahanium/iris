import { Marked, type MarkedExtension } from "marked";
import TurndownService from "turndown";
import * as turndownPluginGfm from "turndown-plugin-gfm";

import {
  splitFrontmatter,
  serializeNoteMarkdown,
  titleFromFields,
} from "@/lib/frontmatter";
import { sanitizeHtml } from "@/lib/sanitize";

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

function escapeMarkdownTableCell(text: string): string {
  return text
    .replace(/\r?\n+/g, "<br>")
    .replace(/\|/g, "\\|")
    .trim();
}

function markdownFromTableCell(cell: Element): string {
  const clone = cell.cloneNode(true) as Element;
  clone.querySelectorAll("colgroup, label").forEach((node) => node.remove());
  return escapeMarkdownTableCell(turndown.turndown(clone.innerHTML));
}

turndown.addRule("gfmTableFromTipTap", {
  filter: (node) => node instanceof HTMLTableElement,
  replacement: (_content, node) => {
    const table = node as HTMLTableElement;
    const rows = Array.from(table.querySelectorAll("tr"));
    if (rows.length === 0) return "";

    const serialized = rows
      .map((row) => {
        const cells = Array.from(row.children).filter(
          (cell) => cell.tagName === "TH" || cell.tagName === "TD",
        );
        return `| ${cells.map(markdownFromTableCell).join(" | ")} |`;
      })
      .filter((row) => row !== "|  |");

    if (serialized.length === 0) return "";

    const columnCount = Array.from(rows[0]!.children).filter(
      (cell) => cell.tagName === "TH" || cell.tagName === "TD",
    ).length;
    const separator = `| ${Array.from({ length: columnCount })
      .map(() => "---")
      .join(" | ")} |`;
    return `\n\n${[serialized[0], separator, ...serialized.slice(1)].join(
      "\n",
    )}\n\n`;
  },
});

turndown.addRule("taskItemFromTipTap", {
  filter: (node) =>
    node instanceof HTMLLIElement &&
    node.getAttribute("data-type") === "taskItem",
  replacement: (_content, node) => {
    const item = node as HTMLLIElement;
    const checked = item.getAttribute("data-checked") === "true" ? "x" : " ";
    const clone = item.cloneNode(true) as HTMLLIElement;
    clone.querySelectorAll("label").forEach((label) => label.remove());
    const body = turndown
      .turndown(clone.innerHTML)
      .trim()
      .replace(/\n{2,}/g, "\n")
      .replace(/\n/g, "\n  ");
    return `- [${checked}] ${body}\n`;
  },
});

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

turndown.addRule("preserveInline", {
  filter: (node) =>
    node instanceof HTMLElement &&
    node.getAttribute("data-type") === "preserve-inline",
  replacement: (_content, node) => {
    const el = node as HTMLElement;
    return el.getAttribute("data-original-raw") ?? "";
  },
});

turndown.addRule("preserveBlock", {
  filter: (node) =>
    node instanceof HTMLElement &&
    node.getAttribute("data-type") === "preserve-block",
  replacement: (_content, node) => {
    const el = node as HTMLElement;
    const raw = el.getAttribute("data-original-raw") ?? "";
    return raw ? `\n\n${raw}\n\n` : "";
  },
});

turndown.addRule("footnoteRef", {
  filter: (node) =>
    node instanceof HTMLElement && node.hasAttribute("data-footnote-ref"),
  replacement: (_content, node) => {
    const el = node as HTMLElement;
    const label = el.getAttribute("data-footnote-ref") ?? "";
    return label ? `[^${label}]` : "";
  },
});

turndown.addRule("footnoteDef", {
  filter: (node) =>
    node instanceof HTMLElement && node.hasAttribute("data-footnote-def"),
  replacement: (_content, node) => {
    const el = node as HTMLElement;
    const raw = el.getAttribute("data-original-raw") ?? "";
    return raw ? `\n\n${raw}\n\n` : "";
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

/** Create an isolated Marked instance so project behavior never mutates the package singleton. */
export function createMarkedInstance(options?: MarkedExtension): Marked {
  const instance = new Marked();
  if (options) {
    instance.use(options);
  }
  return instance;
}

export const editorMarked = createMarkedInstance({ gfm: true, breaks: true });

function replaceWikiLinksInTextNode(textNode: Text): void {
  const value = textNode.nodeValue ?? "";
  const matches = Array.from(value.matchAll(/\[\[([^\]\n]+)\]\]/g));
  if (matches.length === 0) return;

  const doc = textNode.ownerDocument;
  const fragment = doc.createDocumentFragment();
  let cursor = 0;

  for (const match of matches) {
    const raw = match[0]!;
    const title = match[1]!.trim();
    const index = match.index ?? 0;
    if (index > cursor) {
      fragment.appendChild(doc.createTextNode(value.slice(cursor, index)));
    }
    if (title) {
      const span = doc.createElement("span");
      span.setAttribute("data-wiki-link", "");
      span.setAttribute("data-wiki-title", title);
      span.textContent = title;
      fragment.appendChild(span);
    } else {
      fragment.appendChild(doc.createTextNode(raw));
    }
    cursor = index + raw.length;
  }

  if (cursor < value.length) {
    fragment.appendChild(doc.createTextNode(value.slice(cursor)));
  }
  textNode.replaceWith(fragment);
}

function adaptTaskListsForTipTap(root: Element): void {
  root.querySelectorAll("li").forEach((item) => {
    const firstElement = item.firstElementChild;
    if (
      firstElement instanceof HTMLInputElement &&
      firstElement.type === "checkbox"
    ) {
      const parent = item.parentElement;
      if (parent?.tagName === "UL") {
        parent.setAttribute("data-type", "taskList");
      }
      item.setAttribute("data-type", "taskItem");
      item.setAttribute(
        "data-checked",
        firstElement.checked ? "true" : "false",
      );
      firstElement.remove();
    }
  });
}

function adaptWikiLinksForTipTap(root: Element): void {
  const walker = root.ownerDocument.createTreeWalker(
    root,
    NodeFilter.SHOW_TEXT,
    {
      acceptNode: (node) => {
        const parent = node.parentElement;
        if (!parent) return NodeFilter.FILTER_REJECT;
        if (parent.closest("code, pre, a, [data-wiki-link]")) {
          return NodeFilter.FILTER_REJECT;
        }
        return /\[\[[^\]\n]+\]\]/.test(node.nodeValue ?? "")
          ? NodeFilter.FILTER_ACCEPT
          : NodeFilter.FILTER_SKIP;
      },
    },
  );
  const nodes: Text[] = [];
  let current = walker.nextNode();
  while (current) {
    nodes.push(current as Text);
    current = walker.nextNode();
  }
  nodes.forEach(replaceWikiLinksInTextNode);
}

function adaptMarkdownHtmlForTipTap(html: string): string {
  const doc = new DOMParser().parseFromString(
    `<div>${html}</div>`,
    "text/html",
  );
  const root = doc.body.firstElementChild;
  if (!root) return html;
  adaptTaskListsForTipTap(root);
  adaptWikiLinksForTipTap(root);
  return root.innerHTML;
}

function removeTransientAiNodes(root: Element): void {
  root
    .querySelectorAll('[data-type="ai-stream"], [data-ai-stream]')
    .forEach((node) => {
      const originalText =
        node.getAttribute("originalText") ??
        node.getAttribute("originaltext") ??
        node.getAttribute("data-original-text") ??
        "";
      if (!originalText.trim()) {
        node.remove();
        return;
      }
      const paragraph = node.ownerDocument.createElement("p");
      paragraph.textContent = originalText;
      node.replaceWith(paragraph);
    });
}

function protectRawRoundTripNodes(root: Element): Map<string, string> {
  const replacements = new Map<string, string>();
  let index = 0;

  function nextToken(raw: string): string {
    const token = `IRISPRESERVE${index++}TOKEN`;
    replacements.set(token, raw);
    return token;
  }

  root
    .querySelectorAll(
      '[data-type="preserve-inline"], [data-type="preserve-block"], [data-footnote-ref], [data-footnote-def]',
    )
    .forEach((node) => {
      if (!(node instanceof HTMLElement)) return;

      let raw = "";
      let block = false;
      if (
        node.getAttribute("data-type") === "preserve-inline" ||
        node.getAttribute("data-type") === "preserve-block"
      ) {
        raw = node.getAttribute("data-original-raw") ?? "";
        block = node.getAttribute("data-type") === "preserve-block";
      } else if (node.hasAttribute("data-footnote-ref")) {
        const label = node.getAttribute("data-footnote-ref") ?? "";
        raw = label ? `[^${label}]` : "";
      } else if (node.hasAttribute("data-footnote-def")) {
        raw = node.getAttribute("data-original-raw") ?? "";
        block = true;
      }

      const token = raw ? nextToken(raw) : "";
      node.replaceWith(
        node.ownerDocument.createTextNode(block ? `\n\n${token}\n\n` : token),
      );
    });

  return replacements;
}

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
  return editorMarked.parse(md, { async: false }) as string;
}

export interface ParsedNoteForEditor {
  yaml: string | null;
  title: string;
  bodyMd: string;
}

/**
 * Split persisted markdown into document title + TipTap body markdown.
 * `titleFallback` used when frontmatter has no title (e.g. filename stem).
 */
export function parseNoteForEditor(
  md: string,
  titleFallback = "",
): ParsedNoteForEditor {
  const { fields, body: rawBody, yaml } = splitFrontmatter(md);
  let title = titleFromFields(fields);
  let body = rawBody;

  if (!title) {
    if (yaml === null) {
      const legacy = /^#\s+(.+?)\s*(?:\n|$)/.exec(body.trimStart());
      if (legacy) {
        title = legacy[1]!.trim();
        body = body.slice(legacy[0].length).trimStart();
      } else {
        title = titleFallback.trim();
      }
    }
  } else {
    body = stripLeadingBodyTitleHeading(body, title);
  }

  return { yaml, title, bodyMd: body };
}

/** Parse note body markdown → TipTap HTML (no document title block). */
export function markdownBodyToEditorHtml(bodyMd: string): string {
  const bodyTrimmed = bodyMd.trim();
  return bodyTrimmed
    ? adaptMarkdownHtmlForTipTap(markdownToHtml(bodyTrimmed))
    : "<p></p>";
}

/** Serialize TipTap body HTML → markdown (no frontmatter / title). */
export function editorBodyHtmlToMarkdown(html: string): string {
  const doc = new DOMParser().parseFromString(
    `<div>${html}</div>`,
    "text/html",
  );
  const root = doc.body.firstElementChild;
  if (!root) return "";
  removeTransientAiNodes(root);
  const rawRoundTripNodes = protectRawRoundTripNodes(root);
  const bodyHtml = root.innerHTML.trim();
  if (!bodyHtml) return "";
  let markdown = normalizeTurndownEscapes(turndown.turndown(bodyHtml));
  for (const [token, raw] of rawRoundTripNodes) {
    markdown = markdown.replaceAll(token, raw);
  }
  return markdown;
}

/** Assemble full note markdown from title + body + preserved YAML. */
export function buildNoteMarkdown(
  yaml: string | null,
  title: string,
  bodyMd: string,
): string {
  return serializeNoteMarkdown(yaml, title, bodyMd);
}

/**
 * @deprecated Use `parseNoteForEditor` + `markdownBodyToEditorHtml` for the editor.
 * Kept for tests comparing legacy combined HTML.
 */
export function markdownToEditorHtml(md: string, titleFallback = ""): string {
  const { title, bodyMd } = parseNoteForEditor(md, titleFallback);
  const titleHtml = `<h1 class="iris-doc-title">${escapeHtml(title)}</h1>`;
  return `${titleHtml}${markdownBodyToEditorHtml(bodyMd)}`;
}

/** Extract raw frontmatter YAML from note markdown (for round-trip preservation). */
export function extractFrontmatterYaml(md: string): string | null {
  return splitFrontmatter(md).yaml;
}

/**
 * @deprecated Use `serializeOpenNote` with separate title state + `editorBodyHtmlToMarkdown`.
 */
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

  removeTransientAiNodes(root);

  const duplicateBodyH1 = root.firstElementChild?.matches(
    "h1:not(.iris-doc-title)",
  )
    ? root.firstElementChild
    : null;
  if (
    duplicateBodyH1 &&
    title &&
    duplicateBodyH1.textContent?.trim() === title
  ) {
    duplicateBodyH1.remove();
  }

  const bodyMd = editorBodyHtmlToMarkdown(root.innerHTML);
  return serializeNoteMarkdown(
    existingYaml,
    title,
    stripLeadingBodyTitleHeading(bodyMd, title),
  );
}

/** Serialize editor HTML to Markdown (body only, legacy). */
export function htmlToMarkdown(html: string): string {
  return normalizeTurndownEscapes(turndown.turndown(html));
}

export function normalizeTurndownEscapes(markdown: string): string {
  return markdown.replace(/\\\[/g, "[").replace(/\\\]/g, "]");
}

/** Wrap HTML content in a self-contained page with paper-ink styles. */
export function markdownToHtmlPage(md: string, title?: string): string {
  const { fields, body } = splitFrontmatter(md);
  const docTitle = (title ?? titleFromFields(fields)) || "Iris Note";
  const bodyHtml = sanitizeHtml(markdownToHtml(body));
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
  return normalizeTurndownEscapes(htmlToMarkdown(markdownToHtml(md)));
}

/** Round-trip for Iris notes with frontmatter title (split title + body pipeline). */
export function noteMarkdownRoundTrip(md: string, titleFallback = ""): string {
  const { yaml, title, bodyMd } = parseNoteForEditor(md, titleFallback);
  const bodyFromHtml = editorBodyHtmlToMarkdown(
    markdownBodyToEditorHtml(bodyMd),
  );
  return buildNoteMarkdown(yaml, title, bodyFromHtml);
}
