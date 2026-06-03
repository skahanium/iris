/** Cached TipTap HTML per note path to skip re-ingest when switching tabs. */
const htmlByPath = new Map<string, string>();

export function getCachedEditorHtml(path: string): string | undefined {
  return htmlByPath.get(path);
}

export function setCachedEditorHtml(path: string, html: string): void {
  htmlByPath.set(path, html);
}

export function clearCachedEditorHtml(path: string): void {
  htmlByPath.delete(path);
}

export function clearAllEditorHtmlCache(): void {
  htmlByPath.clear();
}
