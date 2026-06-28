/** Cached TipTap HTML per note path to skip re-ingest when switching tabs. */
export type EditorHtmlCacheNamespace = "normal" | "classified";

const htmlByPath = new Map<string, { html: string; digest: string }>();

function cacheKey(
  path: string,
  namespace: EditorHtmlCacheNamespace = "normal",
): string {
  return namespace + "\0" + path;
}

/** Maximum number of cached entries to prevent unbounded memory growth. */
const MAX_CACHE_SIZE = 30;

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
    htmlByPath.delete(key);
    return undefined;
  }
  if (
    cachedHtmlHasVisibleFailedBold(entry.html) ||
    cachedHtmlHasVisibleUnparsedMarkdownBlock(entry.html)
  ) {
    htmlByPath.delete(key);
    return undefined;
  }
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
    htmlByPath.delete(key);
    return;
  }

  // Evict oldest entries if cache is full
  if (htmlByPath.size >= MAX_CACHE_SIZE && !htmlByPath.has(key)) {
    const oldestKey = htmlByPath.keys().next().value;
    if (oldestKey !== undefined) {
      htmlByPath.delete(oldestKey);
    }
  }
  htmlByPath.set(key, { html, digest });
}

export function clearCachedEditorHtml(
  path: string,
  namespace?: EditorHtmlCacheNamespace,
): void {
  if (namespace) {
    htmlByPath.delete(cacheKey(path, namespace));
    return;
  }
  htmlByPath.delete(cacheKey(path, "normal"));
  htmlByPath.delete(cacheKey(path, "classified"));
}

export function clearAllEditorHtmlCache(): void {
  htmlByPath.clear();
}
