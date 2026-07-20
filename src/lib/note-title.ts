/**
 * Compatibility helper for callers that formerly read a Markdown title.
 * Document titles are filename-derived; body ATX `#` headings remain sections.
 */
export function displayTitleFromMarkdown(
  _markdown: string,
  fallback = "无标题",
): string {
  return fallback;
}
