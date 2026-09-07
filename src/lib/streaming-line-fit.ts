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

  let lo = 0;
  let hi = text.length;
  while (lo < hi) {
    const mid = alignEndToCodePoint(text, Math.ceil((lo + hi) / 2));
    const clamped = Math.min(text.length, mid);
    if (clamped <= lo) break;
    if (measure(text.slice(0, clamped)) <= maxPx) {
      lo = clamped;
    } else {
      const previous = previousCodePointStart(text, clamped);
      hi = previous <= lo ? lo : previous;
    }
  }
  return lo;
}

export function nextRevealLength({
  current,
  target,
  remainingPx,
  lineWidthPx,
  measure,
}: {
  current: string;
  target: string;
  remainingPx: number;
  lineWidthPx: number;
  measure: (text: string) => number;
}): number {
  if (!target.startsWith(current) && current.length > 0) {
    return Math.min(current.length, target.length);
  }
  const pending = target.slice(current.length);
  if (!pending) return current.length;

  const newlineAt = pending.indexOf("\n");
  if (newlineAt === 0) {
    let end = 0;
    while (end < pending.length && pending[end] === "\n") end += 1;
    return current.length + end;
  }

  const linePending = newlineAt === -1 ? pending : pending.slice(0, newlineAt);
  const lineWidth = Math.max(1, lineWidthPx);
  const remaining = Math.max(0, remainingPx);
  const fractionPx = lineWidth * STREAMING_LINE_FILL_FRACTION;
  const pendingWidth = measure(linePending);

  if (pendingWidth <= remaining && pendingWidth <= fractionPx) {
    return current.length + linePending.length + (newlineAt === -1 ? 0 : 1);
  }

  const budgetPx =
    remaining < 1
      ? fractionPx
      : remaining <= fractionPx
        ? remaining
        : fractionPx;

  const fit = charsFittingInWidth(linePending, budgetPx, measure);
  if (fit > 0) return current.length + fit;

  return current.length + nextCodePointEnd(linePending, 0);
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
    range.selectNodeContents(textNode);
    if (typeof range.getClientRects !== "function") {
      return { remainingPx: lineWidthPx, lineWidthPx, font };
    }
    const rects = range.getClientRects();
    const last = rects.item(rects.length - 1);
    if (!last) {
      return { remainingPx: lineWidthPx, lineWidthPx, font };
    }
    const remainingPx = Math.max(0, lineWidthPx - last.width);
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

function nextCodePointEnd(value: string, start: number): number {
  if (start >= value.length) return value.length;
  const code = value.charCodeAt(start);
  if (code >= 0xd800 && code <= 0xdbff && start + 1 < value.length) {
    return start + 2;
  }
  return start + 1;
}

function previousCodePointStart(value: string, end: number): number {
  if (end <= 1) return 0;
  const code = value.charCodeAt(end - 1);
  if (code >= 0xdc00 && code <= 0xdfff && end >= 2) {
    const high = value.charCodeAt(end - 2);
    if (high >= 0xd800 && high <= 0xdbff) return end - 2;
  }
  return end - 1;
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

function canvasTextWidth(font: string, text: string): number {
  if (typeof document === "undefined" || !text) return 0;
  if (typeof navigator !== "undefined" && /jsdom/i.test(navigator.userAgent)) {
    return 0;
  }
  try {
    const canvas = document.createElement("canvas");
    const context = canvas.getContext("2d");
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
