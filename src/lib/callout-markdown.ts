/**
 * Obsidian-style callout (`> [!type] Title`) markdown helpers.
 * Shared by ProseMirror export and HTML-based editor export.
 */

/** Build callout markdown from logical lines (title line first, then body lines). */
export function calloutMarkdownFromLines(
  calloutType: string,
  lines: string[],
): string {
  const trimmedType = calloutType.trim() || "note";
  const out: string[] = [];
  for (let i = 0; i < lines.length; i++) {
    const content = lines[i]!.trim();
    if (i === 0) {
      out.push(`> [!${trimmedType}] ${content}`);
    } else if (content) {
      out.push(`> ${content}`);
    } else {
      out.push(">");
    }
  }
  return out.join("\n");
}

/** Detect callout type from a blockquote DOM element (ingest sets `data-callout-type`). */
export function detectCalloutTypeFromElement(element: Element): string | null {
  const attr = element.getAttribute("data-callout-type");
  if (attr?.trim()) {
    return attr.trim();
  }
  const match = />\s*\[!([a-zA-Z][a-zA-Z0-9-]*)\]/.exec(element.outerHTML);
  return match?.[1] ?? null;
}
