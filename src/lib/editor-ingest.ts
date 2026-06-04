/**
 * 编辑器 ingest 管线 — contract 驱动的 Markdown → TipTap 导入
 *
 * 由 contract 驱动：先通过 classifyMarkdownCapabilities 分级，
 * 再按 native/render_only/preserve_only 分别决定如何进入 TipTap。
 *
 * @module editor-ingest
 */
import { marked } from "marked";

import { classifyMarkdownCapabilities } from "@/lib/markdown-contract/contract";
import type {
  MarkdownCapabilityWarning,
  MarkdownSyntaxFragment,
} from "@/lib/markdown-contract/types";
import { PRESERVE_ONLY_SYNTAX_KINDS } from "@/lib/markdown-contract/types";

// ── Internal helpers ──────────────────────────────────────────

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/**
 * Extract callout type from raw markdown like `> [!note] Title\n> Body`.
 */
function calloutType(raw: string): string {
  const m = />\s*\[!([a-zA-Z]+)\]/.exec(raw);
  return m?.[1] ?? "info";
}

/**
 * Extract callout body (everything after `> [!type] Title`).
 */
function calloutBody(raw: string): string {
  const lines = raw.split("\n");
  return lines
    .map((l) => l.replace(/^>\s*/, ""))
    .filter((content) => content && !/^\[![a-zA-Z]+\]/.test(content.trim()))
    .join("\n");
}

/**
 * Extract callout title text.
 */
function calloutTitle(raw: string): string {
  const m = />\s*\[![a-zA-Z]+\]\s*(.*)/.exec(raw);
  return m?.[1]?.trim() ?? "";
}

/**
 * Build a preserve-block div tag for a given fragment.
 */
function preserveBlockDiv(frag: MarkdownSyntaxFragment): string {
  const escapedRaw = escapeHtml(frag.raw);
  return `<div data-type="preserve-block" data-original-raw="${escapedRaw}" data-syntax-kind="${frag.syntaxKind}"></div>`;
}

/**
 * Adapt wiki-links [[...]] in HTML to TipTap data attributes.
 */
function adaptWikiLinks(html: string): string {
  return html.replace(
    /\[\[([^\]\n]+)\]\]/g,
    (_m, title: string) =>
      `<span data-wiki-link="" data-wiki-title="${escapeHtml(title.trim())}">${escapeHtml(title.trim())}</span>`,
  );
}

/**
 * Convert Markdown to TipTap-compatible HTML.
 */
function markdownToTipTapHtml(md: string): string {
  if (!md.trim()) return "";
  let html = marked.parse(md, { async: false }) as string;
  html = adaptWikiLinks(html);
  html = adaptTaskLists(html);
  return html;
}

/**
 * Adapt task list HTML from marked (checkbox inputs) to TipTap
 * data attributes (data-type="taskItem", data-checked).
 */
function adaptTaskLists(html: string): string {
  const doc = new DOMParser().parseFromString(
    `<div>${html}</div>`,
    "text/html",
  );
  const root = doc.body.firstElementChild;
  if (!root) return html;

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

  return root.innerHTML;
}

// ── Public API ────────────────────────────────────────────────

export interface EditorIngestOptions {
  bodyMarkdown: string;
}

export interface EditorIngestResult {
  tipTapHtml: string;
  preserveFragments: MarkdownSyntaxFragment[];
  warnings: MarkdownCapabilityWarning[];
}

/**
 * 由 contract 驱动，生成适合 TipTap 的编辑器初始内容。
 *
 * 流程：
 * 1. classifyMarkdownCapabilities(bodyMd) → fragments
 * 2. native fragments → TipTap HTML
 * 3. render_only fragments → 带属性标记的 HTML
 * 4. preserve_only fragments → PreserveBlock 标签
 * 5. 合并为统一 HTML + 返回片段映射
 */
export function ingestMarkdownForEditor(
  options: EditorIngestOptions,
): EditorIngestResult {
  const { bodyMarkdown } = options;

  if (!bodyMarkdown.trim()) {
    return {
      tipTapHtml: "<p></p>",
      preserveFragments: [],
      warnings: [],
    };
  }

  const fragments = classifyMarkdownCapabilities(bodyMarkdown);

  const preserveFragments = fragments.filter((f) =>
    PRESERVE_ONLY_SYNTAX_KINDS.has(f.syntaxKind),
  );

  const warnings: MarkdownCapabilityWarning[] = [];
  for (const f of fragments) {
    if (f.capability === "unsupported") {
      warnings.push({
        fragment: f,
        message: `Unsupported syntax in editor: ${f.syntaxKind}`,
        severity: "warn",
      });
    }
  }

  // Build HTML by processing fragments in order.
  // We accumulate consecutive native inline fragments into a buffer,
  // flush them as a single paragraph when we hit a block boundary.
  const htmlParts: string[] = [];
  let nativeBuf: string[] = [];

  function flushNative() {
    if (nativeBuf.length === 0) return;
    const joined = nativeBuf.join("");
    const html = markdownToTipTapHtml(joined);
    if (html) htmlParts.push(html);
    nativeBuf = [];
  }

  for (const frag of fragments) {
    const kind = frag.syntaxKind;

    if (kind === "space") {
      flushNative();
      continue;
    }

    if (kind === "callout") {
      flushNative();
      const type = calloutType(frag.raw);
      const title = calloutTitle(frag.raw);
      const body = calloutBody(frag.raw);
      const bodyHtml = body
        ? (marked.parse(body, { async: false }) as string)
        : "";
      const escapedRaw = escapeHtml(frag.raw);
      htmlParts.push(
        `<blockquote data-callout-type="${type}" data-callout-original-raw="${escapedRaw}"><p><strong>${
          title ? escapeHtml(title) : type
        }</strong></p>${bodyHtml}</blockquote>`,
      );
      continue;
    }

    if (kind === "footnote_def") {
      flushNative();
      const mdContent = frag.raw.replace(/^\[\^[^\]]+\]:\s*/, "");
      const contentHtml = mdContent
        ? (marked.parse(mdContent, { async: false }) as string)
        : "";
      htmlParts.push(
        `<p data-footnote-def="${frag.raw.match(/\[\^([^\]]+)\]/)?.[1] ?? ""}">${contentHtml}</p>`,
      );
      continue;
    }

    if (kind === "footnote_ref") {
      // Inline footnote ref — add to native buf so it stays within its parent paragraph
      const label = frag.raw.match(/\[\^([^\]]+)\]/)?.[1] ?? "";
      nativeBuf.push(`<sup data-footnote-ref="${label}">[${label}]</sup>`);
      continue;
    }

    if (PRESERVE_ONLY_SYNTAX_KINDS.has(kind)) {
      flushNative();
      htmlParts.push(preserveBlockDiv(frag));
      continue;
    }

    // Native: check if block-level
    const blockKinds = new Set([
      "heading",
      "code_block",
      "blockquote",
      "table",
      "horizontal_rule",
      "list",
      "task_list",
      "image",
    ]);

    if (blockKinds.has(kind)) {
      flushNative();
      const html = markdownToTipTapHtml(frag.raw);
      if (html) htmlParts.push(html);
    } else {
      // Inline native: accumulate
      nativeBuf.push(frag.raw);
    }
  }

  // Flush any remaining inline content
  flushNative();

  const tipTapHtml = htmlParts.join("");

  return {
    tipTapHtml,
    preserveFragments,
    warnings,
  };
}
