import { ReplaceStep } from "@tiptap/pm/transform";
import type { Transaction } from "@tiptap/pm/state";

import { characterCountExcludingWhitespace } from "@/lib/reading-time";

export interface SessionCharDelta {
  added: number;
  removed: number;
}

function sliceTextContent(slice: ReplaceStep["slice"]): string {
  let text = "";
  slice.content.forEach((node) => {
    text += node.textContent;
  });
  return text;
}

function countNonWhitespace(text: string): number {
  return characterCountExcludingWhitespace(text);
}

export function sessionCharDeltaFromTransaction(
  tr: Pick<Transaction, "before" | "docs" | "steps">,
): SessionCharDelta {
  let added = 0;
  let removed = 0;
  let doc = tr.before;

  for (let i = 0; i < tr.steps.length; i++) {
    const step = tr.steps[i];
    if (!step) {
      continue;
    }
    if (step instanceof ReplaceStep) {
      const deleted = doc.textBetween(step.from, step.to, "", "");
      const inserted = sliceTextContent(step.slice);
      removed += countNonWhitespace(deleted);
      added += countNonWhitespace(inserted);
    }
    const result = step.apply(doc);
    if (result.doc) {
      doc = result.doc;
    }
  }

  return { added, removed };
}

export function mergeSessionCharDelta(
  a: SessionCharDelta,
  b: SessionCharDelta,
): SessionCharDelta {
  return {
    added: a.added + b.added,
    removed: a.removed + b.removed,
  };
}

export function clampSessionCharDeltaForDisplay(
  delta: SessionCharDelta,
): SessionCharDelta {
  return {
    added: Math.max(0, delta.added),
    removed: Math.max(0, delta.removed),
  };
}
