export const EDITOR_ZOOM_MIN = 0.75;
export const EDITOR_ZOOM_MAX = 1.5;
export const EDITOR_ZOOM_STEP = 0.1;
export const EDITOR_ZOOM_DEFAULT = 1;

const STORAGE_KEY = "iris-editor-zoom";

/** Clamp zoom to allowed range (two decimal places). */
export function clampEditorZoom(value: number): number {
  const clamped = Math.min(EDITOR_ZOOM_MAX, Math.max(EDITOR_ZOOM_MIN, value));
  return Math.round(clamped * 100) / 100;
}

export function loadEditorZoom(): number {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw == null) return EDITOR_ZOOM_DEFAULT;
    const parsed = Number.parseFloat(raw);
    if (!Number.isFinite(parsed)) return EDITOR_ZOOM_DEFAULT;
    return clampEditorZoom(parsed);
  } catch {
    return EDITOR_ZOOM_DEFAULT;
  }
}

export function saveEditorZoom(value: number): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(clampEditorZoom(value)));
  } catch {
    /* ignore quota / private mode */
  }
}

export function stepEditorZoom(current: number, direction: 1 | -1): number {
  return clampEditorZoom(current + direction * EDITOR_ZOOM_STEP);
}

export function formatEditorZoomPercent(zoom: number): string {
  return `${Math.round(zoom * 100)}%`;
}
