/**
 * 编辑器 ingest 管线 — contract 驱动的 Markdown → TipTap 导入
 *
 * 由 contract 驱动：先通过 classifyMarkdownCapabilities 分级，
 * 再按 native/render_only/preserve_only 分别决定如何进入 TipTap。
 *
 * @module editor-ingest
 */
import {
  createMarkedInstance,
  repairTightStrongPunctuationBoundaries,
} from "@/lib/markdown";
import { classifyMarkdownCapabilities } from "@/lib/markdown-contract/contract";
import type {
  MarkdownCapabilityWarning,
  MarkdownSyntaxFragment,
  MarkdownSyntaxKind,
} from "@/lib/markdown-contract/types";
import { PRESERVE_ONLY_SYNTAX_KINDS } from "@/lib/markdown-contract/types";

// ── Internal helpers ──────────────────────────────────────────

const ingestMarked = createMarkedInstance({ gfm: true, breaks: true });

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
  const m = />\s*\[!([a-zA-Z][a-zA-Z0-9-]*)\]/.exec(raw);
  return m?.[1] ?? "info";
}

/**
 * Extract callout body (everything after `> [!type] Title`).
 */
function calloutBody(raw: string): string {
  const lines = raw.split("\n");
  const bodyLines: string[] = [];
  let headerSkipped = false;
  for (const line of lines) {
    const content = line.replace(/^>\s*/, "");
    if (!headerSkipped && /^\[![a-zA-Z][a-zA-Z0-9-]*\]/.test(content.trim())) {
      headerSkipped = true;
      continue;
    }
    if (headerSkipped) {
      bodyLines.push(content);
    }
  }
  return bodyLines.join("\n");
}

/**
 * Extract callout title text.
 */
function calloutTitle(raw: string): string {
  const m = />\s*\[![a-zA-Z][a-zA-Z0-9-]*\]\s*(.*)/.exec(raw);
  return m?.[1]?.trim() ?? "";
}

/**
 * Build a preserve-block div tag for a given fragment.
 */
function preserveBlockDiv(frag: MarkdownSyntaxFragment): string {
  const escapedRaw = escapeHtml(frag.raw);
  return `<div data-type="preserve-block" data-original-raw="${escapedRaw}" data-syntax-kind="${frag.syntaxKind}"></div>`;
}

function preserveInlineSpan(
  raw: string,
  syntaxKind: MarkdownSyntaxKind,
): string {
  const escapedRaw = escapeHtml(raw);
  const label = escapeHtml(raw.length > 48 ? `${raw.slice(0, 45)}...` : raw);
  return `<span data-type="preserve-inline" data-original-raw="${escapedRaw}" data-syntax-kind="${syntaxKind}" contenteditable="false">${label}</span>`;
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
  const repaired = repairTightStrongPunctuationBoundaries(md);
  let html = ingestMarked.parse(repaired, {
    async: false,
  }) as string;
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

function footnoteLabel(raw: string): string {
  return raw.match(/\[\^([^\]]+)\]/)?.[1] ?? "";
}

function footnoteIdSuffix(label: string): string {
  const encoded = encodeURIComponent(label.trim()).replace(
    /[!'()*]/g,
    (ch) => `%${ch.charCodeAt(0).toString(16).toUpperCase()}`,
  );
  return encoded || "note";
}

function footnoteRefHtml(raw: string): string {
  const label = footnoteLabel(raw);
  const escapedLabel = escapeHtml(label);
  const suffix = footnoteIdSuffix(label);
  const refId = `footnote-ref-${suffix}`;
  const defId = `footnote-${suffix}`;
  return `<sup data-footnote-ref="${escapedLabel}" id="${refId}"><a href="#${defId}">[${escapedLabel}]</a></sup>`;
}

function footnoteDefHtml(frag: MarkdownSyntaxFragment): string {
  const label = footnoteLabel(frag.raw);
  const escapedLabel = escapeHtml(label);
  const suffix = footnoteIdSuffix(label);
  const refId = `footnote-ref-${suffix}`;
  const defId = `footnote-${suffix}`;
  const mdContent = frag.raw.replace(/^\s*\[\^[^\]]+\]:\s*/, "");
  const contentHtml = mdContent
    ? (ingestMarked.parse(repairTightStrongPunctuationBoundaries(mdContent), {
        async: false,
      }) as string)
    : "";
  const escapedRaw = escapeHtml(frag.raw);
  return `<section data-footnote-def="${escapedLabel}" id="${defId}" data-footnote-return="${refId}" data-original-raw="${escapedRaw}">${contentHtml}</section>`;
}

const SAFE_INLINE_HTML_TAGS = new Set([
  "kbd",
  "sub",
  "sup",
  "mark",
  "small",
  "abbr",
]);

const BLOCK_KINDS = new Set<MarkdownSyntaxKind>([
  "heading",
  "code_block",
  "blockquote",
  "table",
  "horizontal_rule",
  "list",
  "task_list",
  "image",
]);

function openingSafeInlineTag(raw: string): string | null {
  const match = /^<\s*([a-z][\w-]*)\b[^>]*>$/i.exec(raw.trim());
  if (!match) return null;
  const tag = match[1]!.toLowerCase();
  return SAFE_INLINE_HTML_TAGS.has(tag) ? tag : null;
}

function isClosingTag(raw: string, tag: string): boolean {
  return new RegExp(`^<\\s*/\\s*${tag}\\s*>$`, "i").test(raw.trim());
}

function consumeInlinePreserve(
  fragments: MarkdownSyntaxFragment[],
  index: number,
): { raw: string; nextIndex: number } | null {
  const first = fragments[index];
  if (
    !first ||
    first.syntaxKind !== "raw_html" ||
    first.capability !== "preserve_only" ||
    first.inline !== true
  ) {
    return null;
  }

  const tag = openingSafeInlineTag(first.raw);
  if (!tag) return null;

  let raw = first.raw;
  for (let cursor = index + 1; cursor < fragments.length; cursor++) {
    const current = fragments[cursor]!;
    raw += current.raw;
    if (
      current.syntaxKind === "raw_html" &&
      current.inline === true &&
      isClosingTag(current.raw, tag)
    ) {
      return { raw, nextIndex: cursor + 1 };
    }
    if (
      current.syntaxKind === "space" ||
      (!current.inline && PRESERVE_ONLY_SYNTAX_KINDS.has(current.syntaxKind))
    ) {
      return null;
    }
  }

  return null;
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

  for (let i = 0; i < fragments.length; ) {
    const frag = fragments[i]!;
    const kind = frag.syntaxKind;

    if (kind === "space") {
      flushNative();
      i++;
      continue;
    }

    if (kind === "callout") {
      flushNative();
      const type = calloutType(frag.raw);
      const title = calloutTitle(frag.raw);
      const body = calloutBody(frag.raw);
      const bodyHtml = body
        ? (ingestMarked.parse(repairTightStrongPunctuationBoundaries(body), {
            async: false,
          }) as string)
        : "";
      const escapedRaw = escapeHtml(frag.raw);
      htmlParts.push(
        `<blockquote data-callout-type="${type}" data-callout-original-raw="${escapedRaw}"><p><strong>${
          title ? escapeHtml(title) : type
        }</strong></p>${bodyHtml}</blockquote>`,
      );
      i++;
      continue;
    }

    if (kind === "footnote_def") {
      flushNative();
      htmlParts.push(footnoteDefHtml(frag));
      i++;
      continue;
    }

    if (kind === "footnote_ref") {
      // Inline footnote ref — add to native buf so it stays within its parent paragraph
      nativeBuf.push(footnoteRefHtml(frag.raw));
      i++;
      continue;
    }

    if (PRESERVE_ONLY_SYNTAX_KINDS.has(kind)) {
      const inlinePreserve = consumeInlinePreserve(fragments, i);
      if (inlinePreserve) {
        nativeBuf.push(preserveInlineSpan(inlinePreserve.raw, kind));
        i = inlinePreserve.nextIndex;
        continue;
      }
      flushNative();
      htmlParts.push(preserveBlockDiv(frag));
      i++;
      continue;
    }

    if (BLOCK_KINDS.has(kind)) {
      flushNative();
      const html = markdownToTipTapHtml(frag.raw);
      if (html) htmlParts.push(html);
    } else {
      // Inline native: accumulate
      nativeBuf.push(frag.raw);
    }
    i++;
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
