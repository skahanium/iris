/** Cached TipTap HTML per note path to skip re-ingest when switching tabs. */
const htmlByPath = new Map<string, string>();

/** Maximum number of cached entries to prevent unbounded memory growth. */
const MAX_CACHE_SIZE = 30;

export function getCachedEditorHtml(path: string): string | undefined {
  return htmlByPath.get(path);
}

export function setCachedEditorHtml(path: string, html: string): void {
  // Evict oldest entries if cache is full
  if (htmlByPath.size >= MAX_CACHE_SIZE && !htmlByPath.has(path)) {
    const oldestKey = htmlByPath.keys().next().value;
    if (oldestKey !== undefined) {
      htmlByPath.delete(oldestKey);
    }
  }
  htmlByPath.set(path, html);
}

export function clearCachedEditorHtml(path: string): void {
  htmlByPath.delete(path);
}

export function clearAllEditorHtmlCache(): void {
  htmlByPath.clear();
}
