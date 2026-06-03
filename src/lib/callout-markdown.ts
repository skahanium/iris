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
  const nonEmpty = lines.map((l) => l.trim()).filter((l) => l.length > 0);
  const first = nonEmpty[0] ?? "";
  const rest = nonEmpty.slice(1);
  const out: string[] = [`> [!${trimmedType}] ${first}`];
  for (const line of rest) {
    out.push(`> ${line}`);
  }
  return out.join("\n");
}

/** Detect callout type from a blockquote DOM element (ingest sets `data-callout-type`). */
export function detectCalloutTypeFromElement(element: Element): string | null {
  const attr = element.getAttribute("data-callout-type");
  if (attr?.trim()) {
    return attr.trim();
  }
  const match = />\s*\[!([a-zA-Z]+)\]/.exec(element.outerHTML);
  return match?.[1] ?? null;
}
