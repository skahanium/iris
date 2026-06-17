export interface Utf8ByteRange {
  start: number;
  end: number;
}

export interface StringIndexRange {
  start: number;
  end: number;
}

const encoder = new TextEncoder();

export function utf8ByteRangeToStringRange(
  text: string,
  range: Utf8ByteRange,
): StringIndexRange | null {
  if (range.start > range.end || range.start < 0 || range.end < 0) {
    return null;
  }

  let byteOffset = 0;
  let stringOffset = 0;
  let start: number | null = range.start === 0 ? 0 : null;
  let end: number | null = range.end === 0 ? 0 : null;

  for (const segment of Array.from(text)) {
    const nextByteOffset = byteOffset + encoder.encode(segment).length;
    const nextStringOffset = stringOffset + segment.length;

    if (range.start === nextByteOffset) {
      start = nextStringOffset;
    }
    if (range.end === nextByteOffset) {
      end = nextStringOffset;
    }

    byteOffset = nextByteOffset;
    stringOffset = nextStringOffset;
  }

  if (start === null || end === null || range.end > byteOffset) {
    return null;
  }

  return { start, end };
}
