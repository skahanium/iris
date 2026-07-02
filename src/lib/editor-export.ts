/**
 * 编辑器 export 管线 — contract 驱动的 TipTap HTML → Markdown 导出
 *
 * **注意**：生产保存热路径为 `editorDocToMarkdown`（`editor-pm-serialize.ts`）。
 * 本模块仅用于 contract 测试与 HTML 片段级导出，不保证与 PM 路径字节级一致。
 *
 * @deprecated 新功能请扩展 `editor-pm-serialize`；本模块保留给 contract 套件。
 * @module editor-export
 */
import {
  calloutMarkdownFromLines,
  detectCalloutTypeFromElement,
} from "@/lib/callout-markdown";
import { TurndownService, turndownPluginGfm } from "@/lib/markdown-vendor";
import {
  editorBodyHtmlToMarkdown,
  normalizeTurndownEscapes,
} from "@/lib/markdown";
import type {
  MarkdownCapabilityWarning,
  MarkdownSyntaxFragment,
} from "@/lib/markdown-contract/types";

// ── Internal turndown setup (used only for callout content) ─────

// Guard for contract tests: production save/reopen must use editor-pm-serialize instead.
export const EDITOR_EXPORT_CONTRACT_ONLY = true;

const turndown = new TurndownService({
  headingStyle: "atx",
  codeBlockStyle: "fenced",
  bulletListMarker: "-",
  hr: "---",
});
turndown.use(turndownPluginGfm.gfm);

function mdFromHtml(html: string): string {
  return normalizeTurndownEscapes(editorBodyHtmlToMarkdown(html));
}

/** Lightweight inline markdown conversion for callout inner content. */
function inlineToMd(html: string): string {
  if (!html.trim()) return "";
  return normalizeTurndownEscapes(turndown.turndown(html));
}

// ── Public API ─────────────────────────────────────────────────

export interface EditorExportOptions {
  editorHtml: string;
  originalMarkdown: string;
  classifiedFragments: MarkdownSyntaxFragment[];
}

export interface EditorExportResult {
  bodyMarkdown: string;
  preservedCount: number;
  warnings: MarkdownCapabilityWarning[];
}

/**
 * 统一的编辑器导出入口。
 *
 * 流程：
 * 1. 从 editorHtml 提取 PreserveBlock 的 originalRaw → 恢复 preserve_only 原文
 * 2. 从 editorHtml 提取 callout blockquote → 校验 > [!type] 前缀
 * 3. 对剩余 native 内容：turndown.turndown(html) → MD
 * 4. 将 preserve 片段按位置插回
 */
export function exportEditorToMarkdown(
  options: EditorExportOptions,
): EditorExportResult {
  const { editorHtml, classifiedFragments } = options;

  if (!editorHtml.trim() && classifiedFragments.length === 0) {
    return { bodyMarkdown: "", preservedCount: 0, warnings: [] };
  }

  // Parse the editor HTML into a document fragment.
  const doc = new DOMParser().parseFromString(
    `<div>${editorHtml || "<p></p>"}</div>`,
    "text/html",
  );
  const root = doc.body.firstElementChild;
  if (!root) {
    return { bodyMarkdown: "", preservedCount: 0, warnings: [] };
  }

  // Walk children, collecting content segments.
  // For each child: if it's a preserve-block div, extract its originalRaw.
  // Otherwise, convert to markdown.
  const parts: string[] = [];
  let preservedCount = 0;

  for (const child of Array.from(root.children)) {
    // Check if this is a preserve block
    if (
      child instanceof HTMLElement &&
      child.getAttribute("data-type") === "preserve-block"
    ) {
      const raw = child.getAttribute("data-original-raw") ?? "";
      if (raw) {
        parts.push(raw);
        preservedCount++;
      }
      continue;
    }

    // Check if this is a callout blockquote
    if (child.tagName === "BLOCKQUOTE") {
      const originalRaw = child.getAttribute("data-callout-original-raw");
      if (originalRaw?.trim()) {
        parts.push(originalRaw);
        continue;
      }
      const calloutType = detectCalloutTypeFromElement(child);
      if (calloutType) {
        const innerMarkdown = inlineToMd(child.innerHTML);
        const lines = innerMarkdown.split("\n");
        parts.push(calloutMarkdownFromLines(calloutType, lines));
        continue;
      }
    }

    // Native content: convert to markdown via shared pipeline
    const md = mdFromHtml(child.outerHTML);
    if (md.trim()) {
      parts.push(md.trim());
    }
  }

  // Inject preserve-only fragments from classifiedFragments that
  // weren't found in the editor HTML (e.g. after a patch that
  // only included native content).
  const existingPreserves = new Set(parts);
  for (const frag of classifiedFragments) {
    if (
      frag.capability === "preserve_only" &&
      !existingPreserves.has(frag.raw)
    ) {
      parts.push(frag.raw);
      preservedCount++;
    }
  }

  const bodyMarkdown = parts.join("\n\n");

  const warnings: MarkdownCapabilityWarning[] = [];
  // No warnings for export-specific issues in this implementation

  return {
    bodyMarkdown,
    preservedCount,
    warnings,
  };
}
