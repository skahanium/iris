/**
 * Iris 几何 monogram v3 — 唯一路径源
 * React: src/components/brand/iris-mark-paths.ts 与此保持同步
 */

export const MARK_VIEWBOX = "0 0 32 32";

export const FRAME = { x: 2, y: 2, w: 28, h: 28, rx: 8 };

export const I_TOP = { x: 8.8, y: 7.2, w: 13.8, h: 3.4, rx: 1.7 };
export const I_BOTTOM = { x: 10.2, y: 21.4, w: 11.6, h: 3.4, rx: 1.7 };
export const I_SKEW = -7;
export const I_STEM_PATH =
  "M14.4 10.6 C16.15 16 16.05 16 14.5 21.4 L17.5 21.4 C15.95 16 15.85 16 17.6 10.6 Z";

export const BRAND_INK = {
  light: {
    frame: "#e3e3e3",
    ink: "#1a1a1a",
    frameApp: "#ececec",
    bg: "#ffffff",
    /** 任务栏 / 安装包圆角瓦片（非纯白） */
    shellBg: "#e8e8e8",
  },
  dark: {
    frame: "#333333",
    ink: "#f0f0f0",
    frameApp: "#2a2a2a",
    bg: "#191919",
  },
};
