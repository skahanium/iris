import type { FileListItem } from "@/types/ipc";

export type WikiLinkTrigger = "[[" | "【【";

export interface WikiLinkSuggestionMatch {
  trigger: WikiLinkTrigger;
  query: string;
  text: string;
  index: number;
}

export interface WikiLinkSuggestionItem {
  id: string;
  title: string;
  path: string;
  keywords: string;
}

const WIKI_LINK_TRIGGERS: WikiLinkTrigger[] = ["[[", "【【"];

function inferTitleFromPath(path: string): string {
  const filename = path.split(/[\\/]/).pop() ?? path;
  return filename.replace(/\.md$/i, "");
}

export function buildWikiLinkSuggestionItems(
  files: FileListItem[],
): WikiLinkSuggestionItem[] {
  return files
    .filter((file) => !file.path.startsWith(".classified/"))
    .map((file) => {
      const title = file.title.trim() || inferTitleFromPath(file.path);
      return {
        id: file.path,
        title,
        path: file.path,
        keywords: `${title} ${file.path}`,
      };
    });
}

export function filterWikiLinkSuggestionItems(
  items: WikiLinkSuggestionItem[],
  query: string,
  limit = 8,
): WikiLinkSuggestionItem[] {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);

  if (terms.length === 0) return items.slice(0, limit);

  return items
    .map((item) => {
      const title = item.title.toLowerCase();
      const path = item.path.toLowerCase();
      const haystack = item.keywords.toLowerCase();
      if (!terms.every((term) => haystack.includes(term))) return null;

      const joined = terms.join(" ");
      let score = 4;
      if (title === joined) score = 0;
      else if (title.startsWith(joined)) score = 1;
      else if (title.includes(joined)) score = 2;
      else if (path.includes(joined)) score = 3;

      return { item, score };
    })
    .filter((entry): entry is { item: WikiLinkSuggestionItem; score: number } =>
      Boolean(entry),
    )
    .sort(
      (a, b) => a.score - b.score || a.item.title.localeCompare(b.item.title),
    )
    .slice(0, limit)
    .map((entry) => entry.item);
}

export function findWikiLinkSuggestionMatch(
  textBeforeCursor: string,
): WikiLinkSuggestionMatch | null {
  const candidates = WIKI_LINK_TRIGGERS.map((trigger) => ({
    trigger,
    index: textBeforeCursor.lastIndexOf(trigger),
  })).filter((candidate) => candidate.index >= 0);

  if (candidates.length === 0) return null;

  const latest = candidates.reduce((best, candidate) =>
    candidate.index > best.index ? candidate : best,
  );
  const text = textBeforeCursor.slice(latest.index);
  const query = text.slice(latest.trigger.length);

  if (query.includes("\n")) return null;
  if (latest.trigger === "[[" && query.includes("]]")) return null;
  if (latest.trigger === "【【" && query.includes("】】")) return null;

  return {
    trigger: latest.trigger,
    query,
    text,
    index: latest.index,
  };
}
