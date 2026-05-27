/**
 * Iris 几何 monogram v3（与 scripts/iris-mark-paths.mjs 保持同步）
 */

export const MARK_VIEWBOX = "0 0 32 32";

export const FRAME = { x: 2, y: 2, w: 28, h: 28, rx: 8 } as const;

export const I_TOP = { x: 8.8, y: 7.2, w: 13.8, h: 3.4, rx: 1.7 } as const;
export const I_BOTTOM = { x: 10.2, y: 21.4, w: 11.6, h: 3.4, rx: 1.7 } as const;
export const I_SKEW = -7;
export const I_STEM_PATH =
  "M14.4 10.6 C16.15 16 16.05 16 14.5 21.4 L17.5 21.4 C15.95 16 15.85 16 17.6 10.6 Z";
