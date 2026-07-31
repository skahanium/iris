import {
  WORKSPACE_CHROME_AGENT_MAX_REM,
  WORKSPACE_CHROME_AGENT_MIN_REM,
  WORKSPACE_CHROME_AGENT_TARGET_REM,
} from "./workspace-chrome-layout";

const STORAGE_KEY = "iris.aiPanelWidth";
const DEFAULT_ROOT_FONT_SIZE_PX = 16;

/**
 * v1.2.19 起 Agent 侧车宽度预算以 rem 定义（25/30/45rem，见 workspace-chrome-layout）。
 * 以下常量是默认根字号 16px 下的换算值，供既有壳层使用；动态换算用 aiPanelWidthBounds()。
 */
export const AI_PANEL_WIDTH_MIN =
  WORKSPACE_CHROME_AGENT_MIN_REM * DEFAULT_ROOT_FONT_SIZE_PX; // 400
export const AI_PANEL_WIDTH_DEFAULT =
  WORKSPACE_CHROME_AGENT_TARGET_REM * DEFAULT_ROOT_FONT_SIZE_PX; // 480
export const AI_PANEL_WIDTH_MAX = 720; // 45rem @ 16px；既有 Rail 契约字面量

/** 按实际根字号把 Agent 宽度预算（rem）换算为 px 边界。 */
export function aiPanelWidthBounds(rootFontSizePx: number): {
  minPx: number;
  maxPx: number;
} {
  const root =
    Number.isFinite(rootFontSizePx) && rootFontSizePx > 0
      ? rootFontSizePx
      : DEFAULT_ROOT_FONT_SIZE_PX;
  return {
    minPx: Math.round(WORKSPACE_CHROME_AGENT_MIN_REM * root),
    maxPx: Math.round(WORKSPACE_CHROME_AGENT_MAX_REM * root),
  };
}

export function loadAiPanelWidth(
  rootFontSizePx = DEFAULT_ROOT_FONT_SIZE_PX,
): number {
  if (typeof localStorage === "undefined") {
    return AI_PANEL_WIDTH_DEFAULT;
  }
  const raw = localStorage.getItem(STORAGE_KEY);
  const n = raw ? Number.parseInt(raw, 10) : NaN;
  if (!Number.isFinite(n)) {
    return AI_PANEL_WIDTH_DEFAULT;
  }
  const bounds = aiPanelWidthBounds(rootFontSizePx);
  return Math.min(bounds.maxPx, Math.max(bounds.minPx, n));
}

export function saveAiPanelWidth(
  width: number,
  rootFontSizePx = DEFAULT_ROOT_FONT_SIZE_PX,
): void {
  if (typeof localStorage === "undefined") return;
  const bounds = aiPanelWidthBounds(rootFontSizePx);
  const clamped = Math.min(bounds.maxPx, Math.max(bounds.minPx, width));
  localStorage.setItem(STORAGE_KEY, String(clamped));
}
