/** Cached TipTap HTML per note path to skip re-ingest when switching tabs. */
export type EditorHtmlCacheNamespace = "normal" | "classified";

interface EditorHtmlCacheEntry {
  html: string;
  digest: string;
  estimatedBytes: number;
}

const htmlByPath = new Map<string, EditorHtmlCacheEntry>();
let cachedHtmlBytes = 0;

function cacheKey(
  path: string,
  namespace: EditorHtmlCacheNamespace = "normal",
): string {
  return namespace + "\0" + path;
}

/** Maximum number of cached entries to prevent unbounded memory growth. */
const MAX_CACHE_SIZE = 30;
const MAX_CACHE_ENTRY_BYTES = 1024 * 1024;
const MAX_CACHE_TOTAL_BYTES = 8 * 1024 * 1024;

export const EDITOR_HTML_CACHE_FORMAT_VERSION =
  "editor-html-v8-unparsed-markdown-cache-guard";

const FAILED_BOLD_IN_TEXT = /\*\*[^*\n]+\*\*/u;
const UNPARSED_MARKDOWN_BLOCK_MARKER_IN_TEXT =
  /^(?:#{1,6}\s+\S|(?:\d+[.)]|[+-])\s+\S|>\s+\S)/u;

export function editorHtmlHasVisibleFailedBold(html: string): boolean {
  return cachedHtmlHasVisibleFailedBold(html);
}

function shouldSkipFailedBoldScan(node: Node): boolean {
  if (!(node instanceof Element)) return false;
  const tag = node.tagName.toLowerCase();
  if (tag === "pre" || tag === "code") return true;
  const dataType = node.getAttribute("data-type");
  return dataType === "preserve-inline" || dataType === "preserve-block";
}

function cachedHtmlHasVisibleFailedBold(html: string): boolean {
  const doc = new DOMParser().parseFromString(
    `<div>${html}</div>`,
    "text/html",
  );
  const root = doc.body.firstElementChild;
  if (!root) return false;

  const walk = (node: Node) => {
    if (node instanceof Element && shouldSkipFailedBoldScan(node)) return;
    if (node.nodeType === Node.TEXT_NODE) {
      if (FAILED_BOLD_IN_TEXT.test(node.textContent ?? "")) {
        throw new Error("visible failed bold");
      }
      return;
    }
    node.childNodes.forEach(walk);
  };

  try {
    walk(root);
    return false;
  } catch {
    return true;
  }
}

function cachedHtmlHasVisibleUnparsedMarkdownBlock(html: string): boolean {
  const doc = new DOMParser().parseFromString(
    `<div>${html}</div>`,
    "text/html",
  );
  const root = doc.body.firstElementChild;
  if (!root) return false;

  const walk = (node: Node) => {
    if (node instanceof Element && shouldSkipFailedBoldScan(node)) return;
    if (node.nodeType === Node.TEXT_NODE) {
      const text = (node.textContent ?? "").trimStart();
      if (UNPARSED_MARKDOWN_BLOCK_MARKER_IN_TEXT.test(text)) {
        throw new Error("visible unparsed markdown block marker");
      }
      return;
    }
    node.childNodes.forEach(walk);
  };

  try {
    walk(root);
    return false;
  } catch {
    return true;
  }
}

export function editorHtmlDigest(markdown: string): string {
  let hash = 0x811c9dc5;
  const source = `${EDITOR_HTML_CACHE_FORMAT_VERSION}\0${markdown}`;
  for (let i = 0; i < source.length; i++) {
    hash ^= source.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16);
}

export function getCachedEditorHtml(
  path: string,
  expectedDigest: string,
  namespace: EditorHtmlCacheNamespace = "normal",
): string | undefined {
  const key = cacheKey(path, namespace);
  const entry = htmlByPath.get(key);
  if (!entry) return undefined;
  if (entry.digest !== expectedDigest) {
    removeEntry(key);
    return undefined;
  }
  if (
    cachedHtmlHasVisibleFailedBold(entry.html) ||
    cachedHtmlHasVisibleUnparsedMarkdownBlock(entry.html)
  ) {
    removeEntry(key);
    return undefined;
  }
  // Map insertion order is our LRU order; a hit becomes most recently used.
  htmlByPath.delete(key);
  htmlByPath.set(key, entry);
  return entry.html;
}

export function setCachedEditorHtml(
  path: string,
  html: string,
  digest: string,
  namespace: EditorHtmlCacheNamespace = "normal",
): void {
  const key = cacheKey(path, namespace);
  if (
    cachedHtmlHasVisibleFailedBold(html) ||
    cachedHtmlHasVisibleUnparsedMarkdownBlock(html)
  ) {
    removeEntry(key);
    return;
  }

  const estimatedBytes = (html.length + digest.length) * 2;
  if (estimatedBytes > MAX_CACHE_ENTRY_BYTES) {
    removeEntry(key);
    return;
  }

  removeEntry(key);

  while (
    htmlByPath.size >= MAX_CACHE_SIZE ||
    cachedHtmlBytes + estimatedBytes > MAX_CACHE_TOTAL_BYTES
  ) {
    const oldestKey = htmlByPath.keys().next().value;
    if (oldestKey !== undefined) {
      removeEntry(oldestKey);
    } else {
      break;
    }
  }
  htmlByPath.set(key, { html, digest, estimatedBytes });
  cachedHtmlBytes += estimatedBytes;
}

export function clearCachedEditorHtml(
  path: string,
  namespace?: EditorHtmlCacheNamespace,
): void {
  if (namespace) {
    removeEntry(cacheKey(path, namespace));
    return;
  }
  removeEntry(cacheKey(path, "normal"));
  removeEntry(cacheKey(path, "classified"));
}

export function clearAllEditorHtmlCache(): void {
  htmlByPath.clear();
  cachedHtmlBytes = 0;
}

export function getEditorHtmlCacheStats(): {
  entryCount: number;
  estimatedBytes: number;
} {
  return { entryCount: htmlByPath.size, estimatedBytes: cachedHtmlBytes };
}

function removeEntry(key: string): void {
  const entry = htmlByPath.get(key);
  if (entry)
    cachedHtmlBytes = Math.max(0, cachedHtmlBytes - entry.estimatedBytes);
  htmlByPath.delete(key);
}
