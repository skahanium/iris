/** Cached TipTap HTML per note path to skip re-ingest when switching tabs. */
const htmlByPath = new Map<string, { html: string; digest: string }>();

/** Maximum number of cached entries to prevent unbounded memory growth. */
const MAX_CACHE_SIZE = 30;

export const EDITOR_HTML_CACHE_FORMAT_VERSION =
  "editor-html-v7-escaped-strong-repair";

const FAILED_BOLD_IN_TEXT = /\*\*[^*\n]+\*\*/u;

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
): string | undefined {
  const entry = htmlByPath.get(path);
  if (!entry) return undefined;
  if (entry.digest !== expectedDigest) {
    htmlByPath.delete(path);
    return undefined;
  }
  if (cachedHtmlHasVisibleFailedBold(entry.html)) {
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
  if (cachedHtmlHasVisibleFailedBold(html)) {
    htmlByPath.delete(path);
    return;
  }

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
