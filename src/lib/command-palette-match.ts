/** 在 label 中查找 query 的匹配区间（用于高亮） */
export function getLabelMatchRange(
  label: string,
  query: string,
): { start: number; end: number } | null {
  const q = query.trim().toLowerCase();
  if (!q) return null;
  const lower = label.toLowerCase();
  const idx = lower.indexOf(q);
  if (idx < 0) return null;
  return { start: idx, end: idx + q.length };
}

export interface LabelSegment {
  text: string;
  highlighted: boolean;
}

export function splitLabelByMatch(
  label: string,
  query: string,
): LabelSegment[] {
  const range = getLabelMatchRange(label, query);
  if (!range) return [{ text: label, highlighted: false }];
  const parts: LabelSegment[] = [];
  if (range.start > 0) {
    parts.push({ text: label.slice(0, range.start), highlighted: false });
  }
  parts.push({
    text: label.slice(range.start, range.end),
    highlighted: true,
  });
  if (range.end < label.length) {
    parts.push({ text: label.slice(range.end), highlighted: false });
  }
  return parts;
}
