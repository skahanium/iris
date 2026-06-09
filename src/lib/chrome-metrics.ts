/**
 * 与 `src-tauri/src/chrome_metrics.rs` 保持一致的镜像常量（供测试与类型文档引用）。
 * 运行时 CSS 变量以 `getDesktopChromeMetrics` IPC 为准。
 */
export const DEFAULT_TITLEBAR_HEIGHT_PX = 44;
export const MACOS_TITLEBAR_HEIGHT_PX = 44;
export const MACOS_TRAFFIC_INSET_PX = 0;

export interface DesktopChromeMetrics {
  titlebarHeightLogical: number;
  trafficInsetLogical: number;
  scaleFactor: number;
}

export function logicalPxToRem(px: number): string {
  return `${px / 16}rem`;
}

export function applyDesktopChromeMetricsToDocument(
  metrics: DesktopChromeMetrics,
): void {
  const root = document.documentElement;
  root.style.setProperty(
    "--titlebar-height",
    logicalPxToRem(metrics.titlebarHeightLogical),
  );
  root.style.setProperty(
    "--titlebar-traffic-inset",
    `${metrics.trafficInsetLogical}px`,
  );
}
