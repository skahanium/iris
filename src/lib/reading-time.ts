/** Non-whitespace character count for status bar display. */
export function characterCountExcludingWhitespace(text: string): number {
  return text.replace(/\s+/g, "").length;
}

function isCjkCodePoint(code: number): boolean {
  return (
    (code >= 0x4e00 && code <= 0x9fff) ||
    (code >= 0x3040 && code <= 0x30ff) ||
    (code >= 0xac00 && code <= 0xd7af)
  );
}

/** Estimate reading time in whole minutes (minimum 1). */
export function readingMinutes(text: string): number {
  let cjk = 0;
  let asciiWords = 0;
  let inAsciiWord = false;
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i);
    if (isCjkCodePoint(code)) {
      cjk++;
      inAsciiWord = false;
      continue;
    }
    const isSpace =
      code <= 0x20 ||
      code === 0xa0 ||
      (code >= 0x2000 && code <= 0x200a) ||
      code === 0x2028 ||
      code === 0x2029 ||
      code === 0x3000;
    if (isSpace) {
      if (inAsciiWord) asciiWords++;
      inAsciiWord = false;
      continue;
    }
    inAsciiWord = true;
  }
  if (inAsciiWord) asciiWords++;
  return Math.max(1, Math.ceil(cjk / 500 + asciiWords / 250));
}
