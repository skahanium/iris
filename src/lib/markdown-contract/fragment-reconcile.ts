import type { MarkdownSyntaxFragment } from "./types";

/**
 * Reconcile lexer fragments with exact source offsets and fill any source gaps.
 * Marked consumes link-reference-like footnote definitions; this restores them
 * without mutating the fragment array during traversal.
 */
export function reconcileFragmentsWithSource(
  source: string,
  fragments: MarkdownSyntaxFragment[],
): MarkdownSyntaxFragment[] {
  const reconciled: MarkdownSyntaxFragment[] = [];
  let cursor = 0;

  for (const fragment of fragments) {
    const index = source.indexOf(fragment.raw, cursor);
    if (index === -1) continue;
    if (index > cursor) {
      reconciled.push(...fragmentsFromGap(source.slice(cursor, index), cursor));
    }
    reconciled.push({
      ...fragment,
      offset: index,
      endOffset: index + fragment.raw.length,
    });
    cursor = index + fragment.raw.length;
  }

  if (cursor < source.length) {
    reconciled.push(...fragmentsFromGap(source.slice(cursor), cursor));
  }

  return reconciled.sort((a, b) => a.offset - b.offset);
}

function fragmentsFromGap(
  gapText: string,
  gapOffset: number,
): MarkdownSyntaxFragment[] {
  const fragments: MarkdownSyntaxFragment[] = [];
  const defRegex = /(^|\n)([ \t]*\[\^[^\]\n]+\]:[^\n]*)/g;
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = defRegex.exec(gapText)) !== null) {
    const leading = match[1] ?? "";
    const defRaw = match[2] ?? "";
    const beforeEnd = match.index + leading.length;
    pushGapPlainFragment(
      fragments,
      gapText.slice(cursor, beforeEnd),
      gapOffset + cursor,
    );
    const defOffset = gapOffset + beforeEnd;
    fragments.push({
      raw: defRaw,
      syntaxKind: "footnote_def",
      offset: defOffset,
      endOffset: defOffset + defRaw.length,
      capability: "render_only",
    });
    cursor = match.index + match[0].length;
  }

  pushGapPlainFragment(fragments, gapText.slice(cursor), gapOffset + cursor);
  return fragments;
}

function pushGapPlainFragment(
  fragments: MarkdownSyntaxFragment[],
  raw: string,
  offset: number,
): void {
  if (!raw) return;
  fragments.push({
    raw,
    syntaxKind: /^\s+$/.test(raw) ? "space" : "text",
    offset,
    endOffset: offset + raw.length,
    capability: "native",
  });
}
