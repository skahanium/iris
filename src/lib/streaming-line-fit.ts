//! Layout-aware streaming reveal: fill the current visual line, then wrap.

export const STREAMING_LINE_FILL_FRACTION = 0.4;
export const STREAMING_FALLBACK_FONT_SIZE_PX = 14;
export const STREAMING_FALLBACK_LINE_WIDTH_PX = 240;

export interface StreamingLineBudget {
  remainingPx: number;
  lineWidthPx: number;
  font: string;
}

export function fontSizePxFromFont(
  font: string,
  fallback = STREAMING_FALLBACK_FONT_SIZE_PX,
): number {
  const match = font.match(/(\d+(?:\.\d+)?)px/);
  if (!match) return fallback;
  const size = Number(match[1]);
  return Number.isFinite(size) && size > 0 ? size : fallback;
}

export function estimateTextWidth(text: string, fontSizePx: number): number {
  if (!text || fontSizePx <= 0) return 0;
  let width = 0;
  for (const point of text) {
    width += isWideCodePoint(point.codePointAt(0) ?? 0)
      ? fontSizePx
      : fontSizePx * 0.55;
  }
  return width;
}

export function measureTextForBudget(text: string, font: string): number {
  const canvasWidth = canvasTextWidth(font, text);
  if (canvasWidth > 0) return canvasWidth;
  return estimateTextWidth(text, fontSizePxFromFont(font));
}

export function charsFittingInWidth(
  text: string,
  maxPx: number,
  measure: (slice: string) => number,
): number {
  if (maxPx <= 0 || text.length === 0) return 0;
  if (measure(text) <= maxPx) return text.length;

  const ends = graphemeEnds(text);
  let lo = 0;
  let hi = ends.length;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (measure(text.slice(0, ends[mid - 1])) <= maxPx) lo = mid;
    else hi = mid - 1;
  }
  return lo === 0 ? 0 : ends[lo - 1]!;
}

// Segment only the bounded pending prefix, not the growing answer on each frame.
const graphemeSegmenter = new Intl.Segmenter(undefined, {
  granularity: "grapheme",
});

/** The final network chunk may still gain a combining mark, modifier or ZWJ. */
export function stableGraphemePrefix(value: string): string {
  if (!value) return value;
  let end =
    graphemeSegmenter.segment(value).containing(value.length - 1)?.index ?? 0;
  const last = value.charCodeAt(value.length - 1);
  // A half surrogate may become an emoji modifier attached to the prior cluster.
  if (last >= 0xd800 && last <= 0xdbff && end > 0)
    end = graphemeSegmenter.segment(value).containing(end - 1)?.index ?? 0;
  return value.slice(0, end);
}

function graphemeEnds(value: string): number[] {
  return Array.from(
    graphemeSegmenter.segment(value),
    ({ index, segment }) => index + segment.length,
  );
}

export function nextRevealLength({
  current,
  target,
  remainingPx,
  lineWidthPx,
  measure,
  maxAdvancePx,
}: {
  current: string;
  target: string;
  remainingPx: number;
  lineWidthPx: number;
  measure: (text: string) => number;
  maxAdvancePx?: number;
}): number {
  if (!target.startsWith(current) && current.length > 0) {
    return Math.min(current.length, target.length);
  }
  const pending = target.slice(current.length);
  if (!pending) return current.length;

  const leadingBreak = pending.startsWith("\r\n")
    ? 2
    : pending.startsWith("\n")
      ? 1
      : 0;
  if (leadingBreak) {
    return (
      current.length +
      (maxAdvancePx === undefined || maxAdvancePx >= lineWidthPx
        ? leadingBreak
        : 0)
    );
  }

  // Stop at a grapheme boundary. Slicing at a numeric character limit can divide
  // a joined emoji or combining sequence even when the binary search is safe.
  let prefixEnd = 0;
  for (const { index, segment } of graphemeSegmenter.segment(pending)) {
    if (segment.includes("\n")) break;
    prefixEnd = index + segment.length;
    if (prefixEnd >= 512) break;
  }
  const linePending = pending.slice(0, prefixEnd);
  const trailingBreak = pending.startsWith("\r\n", prefixEnd)
    ? 2
    : pending.startsWith("\n", prefixEnd)
      ? 1
      : 0;
  const lineWidth = Math.max(1, lineWidthPx);
  const remaining = Math.max(0, remainingPx);
  const fractionPx = Math.min(
    lineWidth * STREAMING_LINE_FILL_FRACTION,
    maxAdvancePx ?? Infinity,
  );
  const pendingWidth = measure(linePending);

  if (pendingWidth <= remaining && pendingWidth <= fractionPx) {
    return (
      current.length +
      linePending.length +
      (maxAdvancePx === undefined && trailingBreak ? trailingBreak : 0)
    );
  }

  const budgetPx =
    remaining < 1
      ? fractionPx
      : remaining <= fractionPx
        ? remaining
        : fractionPx;

  const fit = charsFittingInWidth(linePending, budgetPx, measure);
  if (fit > 0) return current.length + fit;

  const first = graphemeEnds(linePending)[0] ?? 0;
  // Never force a character through a depleted time budget. Wide graphemes may
  // exceed the remaining space and wrap, but they must still fit the time credit.
  if (
    maxAdvancePx !== undefined &&
    Math.min(lineWidth, measure(linePending.slice(0, first))) > maxAdvancePx
  )
    return current.length;
  return current.length + first;
}

export function fallbackLineBudget(): StreamingLineBudget {
  return {
    remainingPx: STREAMING_FALLBACK_LINE_WIDTH_PX,
    lineWidthPx: STREAMING_FALLBACK_LINE_WIDTH_PX,
    font: `${STREAMING_FALLBACK_FONT_SIZE_PX}px sans-serif`,
  };
}

/** Remaining pixels on the last wrapped line of a streaming tail node. */
export function readTailLineBudget(
  tail: HTMLElement,
): StreamingLineBudget | null {
  const lineWidthPx = tail.clientWidth;
  if (lineWidthPx <= 0) return null;
  const font = computedFont(tail);

  const textNode = lastTextNode(tail);
  if (!textNode?.textContent) {
    return { remainingPx: lineWidthPx, lineWidthPx, font };
  }

  try {
    const range = document.createRange();
    const end = textNode.length;
    range.setStart(textNode, Math.max(0, end - 1));
    range.setEnd(textNode, end);
    if (typeof range.getClientRects !== "function") {
      return { remainingPx: lineWidthPx, lineWidthPx, font };
    }
    const rects = range.getClientRects();
    const last = rects.item(rects.length - 1);
    if (!last) {
      return { remainingPx: lineWidthPx, lineWidthPx, font };
    }
    const box = tail.getBoundingClientRect();
    const remainingPx = Math.max(
      0,
      Math.min(
        lineWidthPx,
        box.width > 0 ? box.right - last.right : lineWidthPx - last.width,
      ),
    );
    return { remainingPx, lineWidthPx, font };
  } catch {
    return { remainingPx: lineWidthPx, lineWidthPx, font };
  }
}

export function alignEndToCodePoint(value: string, end: number): number {
  if (end <= 0 || end >= value.length) return end;
  const code = value.charCodeAt(end - 1);
  if (code >= 0xd800 && code <= 0xdbff) return end + 1;
  return end;
}

function computedFont(element: HTMLElement): string {
  const fallback = `${STREAMING_FALLBACK_FONT_SIZE_PX}px sans-serif`;
  if (
    typeof window === "undefined" ||
    typeof window.getComputedStyle !== "function"
  ) {
    return fallback;
  }
  const style = window.getComputedStyle(element);
  const shorthand = style.font?.trim();
  if (shorthand) return shorthand;
  const size = style.fontSize?.trim() || `${STREAMING_FALLBACK_FONT_SIZE_PX}px`;
  const family = style.fontFamily?.trim() || "sans-serif";
  const weight = style.fontWeight?.trim();
  return weight ? `${weight} ${size} ${family}` : `${size} ${family}`;
}

function lastTextNode(root: Node): Text | null {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let last: Text | null = null;
  let current = walker.nextNode();
  while (current) {
    if (current instanceof Text && current.textContent) last = current;
    current = walker.nextNode();
  }
  return last;
}

let textMeasureContext: CanvasRenderingContext2D | null | undefined;

function canvasTextWidth(font: string, text: string): number {
  if (typeof document === "undefined" || !text) return 0;
  if (typeof navigator !== "undefined" && /jsdom/i.test(navigator.userAgent)) {
    return 0;
  }
  try {
    textMeasureContext ??= document.createElement("canvas").getContext("2d");
    const context = textMeasureContext;
    if (!context) return 0;
    context.font = font;
    const width = context.measureText(text).width;
    return Number.isFinite(width) ? width : 0;
  } catch {
    return 0;
  }
}

function isWideCodePoint(code: number): boolean {
  return (
    (code >= 0x1100 && code <= 0x115f) ||
    (code >= 0x2e80 && code <= 0xa4cf) ||
    (code >= 0xac00 && code <= 0xd7a3) ||
    (code >= 0xf900 && code <= 0xfaff) ||
    (code >= 0xfe10 && code <= 0xfe19) ||
    (code >= 0xfe30 && code <= 0xfe6f) ||
    (code >= 0xff00 && code <= 0xff60) ||
    (code >= 0xffe0 && code <= 0xffe6) ||
    (code >= 0x1f300 && code <= 0x1f64f) ||
    (code >= 0x20000 && code <= 0x3ffff)
  );
}
