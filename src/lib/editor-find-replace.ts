import type { Node as ProseMirrorNode } from "@tiptap/pm/model";

export interface TextRange {
  from: number;
  to: number;
}

export interface FindTextOptions {
  caseSensitive?: boolean;
}

export function findTextRanges(
  text: string,
  query: string,
  options: FindTextOptions = {},
): TextRange[] {
  if (!query) {
    return [];
  }
  const haystack = options.caseSensitive ? text : text.toLocaleLowerCase();
  const needle = options.caseSensitive ? query : query.toLocaleLowerCase();
  const ranges: TextRange[] = [];
  let index = 0;
  while (index <= haystack.length - needle.length) {
    const found = haystack.indexOf(needle, index);
    if (found === -1) {
      break;
    }
    ranges.push({ from: found, to: found + needle.length });
    index = found + Math.max(needle.length, 1);
  }
  return ranges;
}

export function replaceTextRange(
  text: string,
  range: TextRange,
  replacement: string,
): string {
  return `${text.slice(0, range.from)}${replacement}${text.slice(range.to)}`;
}

export function replaceAllTextRanges(
  text: string,
  ranges: TextRange[],
  replacement: string,
): string {
  return [...ranges]
    .sort((a, b) => b.from - a.from)
    .reduce((next, range) => replaceTextRange(next, range, replacement), text);
}

export function findTextRangesInDoc(
  doc: ProseMirrorNode,
  query: string,
  options: FindTextOptions = {},
): TextRange[] {
  const ranges: TextRange[] = [];
  doc.descendants((node, pos) => {
    if (!node.isText || !node.text) {
      return;
    }
    for (const range of findTextRanges(node.text, query, options)) {
      ranges.push({ from: pos + range.from, to: pos + range.to });
    }
  });
  return ranges;
}
