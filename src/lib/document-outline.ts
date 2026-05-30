import type { Node as ProseMirrorNode } from "@tiptap/pm/model";

export interface OutlineEntry {
  level: 1 | 2 | 3;
  text: string;
  /** Document position at start of heading content (for selection / scroll). */
  pos: number;
}

const MAX_LEVEL = 3;

function isSectionHeading(node: ProseMirrorNode): boolean {
  return (
    node.type.name === "heading" &&
    typeof node.attrs.level === "number" &&
    node.attrs.level >= 1 &&
    node.attrs.level <= MAX_LEVEL
  );
}

/** Extract H1–H3 section headings from a ProseMirror document. */
export function outlineFromDoc(doc: ProseMirrorNode): OutlineEntry[] {
  const items: OutlineEntry[] = [];
  doc.forEach((node, offset) => {
    if (!isSectionHeading(node)) return;
    const level = node.attrs.level as number;
    const text = node.textContent.trim();
    if (!text) return;
    items.push({
      level: level as 1 | 2 | 3,
      text,
      pos: offset + 1,
    });
  });
  return items;
}

/** Index of the heading entry that contains `head`, or -1. */
export function activeOutlineIndex(
  entries: OutlineEntry[],
  head: number,
): number {
  if (entries.length === 0) return -1;
  let active = -1;
  for (let i = 0; i < entries.length; i++) {
    if (head >= entries[i]!.pos) active = i;
    else break;
  }
  return active;
}
