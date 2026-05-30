const STORAGE_KEY = "iris.aiPanelWidth";
export const AI_PANEL_WIDTH_DEFAULT = 360;
export const AI_PANEL_WIDTH_MIN = 280;
export const AI_PANEL_WIDTH_MAX = 560;

export function loadAiPanelWidth(): number {
  if (typeof localStorage === "undefined") {
    return AI_PANEL_WIDTH_DEFAULT;
  }
  const raw = localStorage.getItem(STORAGE_KEY);
  const n = raw ? Number.parseInt(raw, 10) : NaN;
  if (!Number.isFinite(n)) {
    return AI_PANEL_WIDTH_DEFAULT;
  }
  return Math.min(AI_PANEL_WIDTH_MAX, Math.max(AI_PANEL_WIDTH_MIN, n));
}

export function saveAiPanelWidth(width: number): void {
  if (typeof localStorage === "undefined") return;
  const clamped = Math.min(
    AI_PANEL_WIDTH_MAX,
    Math.max(AI_PANEL_WIDTH_MIN, width),
  );
  localStorage.setItem(STORAGE_KEY, String(clamped));
}
