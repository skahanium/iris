import {
  BRAND_INK,
  FRAME,
  I_BOTTOM,
  I_SKEW,
  I_STEM_PATH,
  I_TOP,
  MARK_VIEWBOX,
} from "./iris-mark-paths.mjs";

function rect(r, fill) {
  return `<rect x="${r.x}" y="${r.y}" width="${r.w}" height="${r.h}" rx="${r.rx}" fill="${fill}"/>`;
}

function iLetter(ink) {
  return `<g transform="translate(16 16) skewX(${I_SKEW}) translate(-16 -16)">
  ${rect(I_TOP, ink)}
  <path d="${I_STEM_PATH}" fill="${ink}"/>
  ${rect(I_BOTTOM, ink)}
</g>`;
}

/**
 * @param {{ frame: string; ink: string; width?: number; height?: number }} opts
 */
export function monogramSvg({ frame, ink, width = 32, height = 32 }) {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${MARK_VIEWBOX}" width="${width}" height="${height}" fill="none">
  ${rect(FRAME, frame)}
  ${iLetter(ink)}
</svg>`;
}

export function monogramTraySvg({ width = 32, height = 32 } = {}) {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${MARK_VIEWBOX}" width="${width}" height="${height}" fill="none">
  ${rect(FRAME, "currentColor")}
  <g transform="translate(16 16) skewX(${I_SKEW}) translate(-16 -16)" fill="currentColor">
  ${rect(I_TOP, "currentColor")}
  <path d="${I_STEM_PATH}" fill="currentColor"/>
  ${rect(I_BOTTOM, "currentColor")}
</g>
</svg>`;
}

/** 边距比例：越小字形越大（任务栏 16–32px 可读） */
const APP_ICON_PAD = 0.12;

/** 应用图标（含方框）：预览 / 暗色场景 */
export function appIconSvg({ bg, frame, ink, size = 1024 }) {
  const pad = size * APP_ICON_PAD;
  const scale = (size - pad * 2) / 32;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}" width="${size}" height="${size}">
  <rect width="${size}" height="${size}" fill="${bg}"/>
  <g transform="translate(${pad}, ${pad}) scale(${scale})">
  ${rect(FRAME, frame)}
  ${iLetter(ink)}
  </g>
</svg>`;
}

/**
 * 桌面壳图标：透明角 + 圆角矩形灰底 + 大「I」（任务栏显示为圆角瓦片）
 */
export function appIconShellSvg({ bg, ink, size = 1024 }) {
  const outer = size * 0.035;
  const tile = size - outer * 2;
  const rx = tile * 0.22;
  const letterPad = tile * 0.1;
  const scale = (tile - letterPad * 2) / 32;
  const origin = outer + letterPad;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}" width="${size}" height="${size}">
  <rect x="${outer}" y="${outer}" width="${tile}" height="${tile}" rx="${rx}" ry="${rx}" fill="${bg}"/>
  <g transform="translate(${origin}, ${origin}) scale(${scale})">
  ${iLetter(ink)}
  </g>
</svg>`;
}

export function exportBrandSvgs() {
  const { light, dark } = BRAND_INK;
  return {
    monogramTransparent: monogramSvg({
      frame: light.frame,
      ink: light.ink,
    }),
    monogramTray: monogramTraySvg(),
    /** Tauri / Windows 任务栏主源图 */
    appShell: appIconShellSvg({
      bg: light.shellBg,
      ink: light.ink,
    }),
    appDark: appIconSvg({
      bg: dark.bg,
      frame: dark.frameApp,
      ink: dark.ink,
    }),
    appLight: appIconSvg({
      bg: light.bg,
      frame: light.frameApp,
      ink: light.ink,
    }),
  };
}
