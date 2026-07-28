import {
  formatCitationDisplayLabel,
  normalizeCitationLabel,
} from "@/lib/ai/citation-markdown";
import type { WebCitationEntry } from "@/types/ai";

const BARE_CITATION_IN_TEXT =
  /(?<!\\)\[(citation:\d+|[CTFAWL]\d+|\d+|[\u2070\u00B9\u00B2\u00B3\u2074-\u2079]+)\](?!\()/gi;

/** Persisted answers after Rust linkify use `[N](https://…)` without bare `[N]`. */
const MARKDOWN_HTTPS_CITATION = /\[(\d{1,3})\]\(https:\/\/[^)\s"]+\)/gi;

const MARKDOWN_IRIS_CITE =
  /\[(?:citation:\d+|[CTFAWL]\d+|\d+)\]\(#iris-cite-[^)]+\)/gi;

function addIndex(indices: Set<number>, label: string | undefined) {
  if (!label) return;
  const index = Number.parseInt(formatCitationDisplayLabel(label), 10);
  if (Number.isFinite(index) && index > 0) {
    indices.add(index);
  }
}

/** Collect 1-based web citation indices referenced in assistant markdown. */
export function referencedCitationIndices(content: string): Set<number> {
  const indices = new Set<number>();
  for (const match of content.matchAll(BARE_CITATION_IN_TEXT)) {
    addIndex(indices, match[1]);
  }
  for (const match of content.matchAll(MARKDOWN_HTTPS_CITATION)) {
    addIndex(indices, match[1]);
  }
  for (const match of content.matchAll(MARKDOWN_IRIS_CITE)) {
    const label = match[0].match(/\[([^\]]+)\]/)?.[1];
    addIndex(indices, label);
  }
  return indices;
}

/** True when markdown link text is a short numeric web footnote. */
export function isNumericFootnoteLinkText(text: string): boolean {
  const trimmed = text.trim();
  if (/^\d{1,3}$/.test(trimmed)) {
    return true;
  }
  const prefixed = trimmed.match(/^[CTFAWL](\d{1,3})$/i);
  return prefixed != null;
}

/** Parse citation index from iris-cite hash ref or raw marker label. */
export function citationIndexFromRef(ref: string): number | null {
  const normalized = normalizeCitationLabel(decodeURIComponent(ref));
  const display = formatCitationDisplayLabel(normalized);
  const index = Number.parseInt(display, 10);
  return Number.isFinite(index) && index > 0 ? index : null;
}

export function filterReferencedWebCitations(
  entries: WebCitationEntry[],
  content: string,
  referencedOnly = true,
): WebCitationEntry[] {
  const ordered = [...entries].sort((left, right) => left.index - right.index);
  if (!referencedOnly) {
    return ordered;
  }
  const referenced = referencedCitationIndices(content);
  if (referenced.size === 0) {
    return [];
  }
  return ordered.filter((entry) => referenced.has(entry.index));
}

export function resolveWebCitationUrl(
  entries: readonly WebCitationEntry[],
  ref: string,
): string | null {
  const index = citationIndexFromRef(ref);
  if (index == null) {
    return null;
  }
  return entries.find((entry) => entry.index === index)?.url ?? null;
}
