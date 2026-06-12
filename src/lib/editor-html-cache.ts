/** Cached TipTap HTML per note path to skip re-ingest when switching tabs. */
const htmlByPath = new Map<string, { html: string; digest: string }>();

/** Maximum number of cached entries to prevent unbounded memory growth. */
const MAX_CACHE_SIZE = 30;

export function editorHtmlDigest(markdown: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < markdown.length; i++) {
    hash ^= markdown.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16);
}

export function getCachedEditorHtml(
  path: string,
  expectedDigest: string,
): string | undefined {
  const entry = htmlByPath.get(path);
  if (!entry) return undefined;
  if (entry.digest !== expectedDigest) {
    htmlByPath.delete(path);
    return undefined;
  }
  return entry.html;
}

export function setCachedEditorHtml(
  path: string,
  html: string,
  digest: string,
): void {
  // Evict oldest entries if cache is full
  if (htmlByPath.size >= MAX_CACHE_SIZE && !htmlByPath.has(path)) {
    const oldestKey = htmlByPath.keys().next().value;
    if (oldestKey !== undefined) {
      htmlByPath.delete(oldestKey);
    }
  }
  htmlByPath.set(path, { html, digest });
}

export function clearCachedEditorHtml(path: string): void {
  htmlByPath.delete(path);
}

export function clearAllEditorHtmlCache(): void {
  htmlByPath.clear();
}
