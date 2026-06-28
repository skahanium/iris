import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";

import {
  applyDesktopChromeFullscreenStateToDocument,
  applyDesktopChromeMetricsToDocument,
  type DesktopChromeMetrics,
} from "@/lib/chrome-metrics";
import { getDesktopChromeMetrics } from "@/lib/ipc";
import { isMacOSDesktopChrome } from "@/lib/platform-chrome";

const DEBOUNCE_MS = 48;

async function syncChromeMetrics(): Promise<DesktopChromeMetrics> {
  const metrics = await getDesktopChromeMetrics();
  applyDesktopChromeMetricsToDocument(metrics);
  return metrics;
}

async function syncFullscreenChromeState(fullscreen: boolean): Promise<void> {
  if (fullscreen) {
    applyDesktopChromeFullscreenStateToDocument(true);
    return;
  }
  const metrics = await syncChromeMetrics();
  applyDesktopChromeFullscreenStateToDocument(false, metrics);
}

/**
 * macOS：全屏/缩放/聚焦后同步标题栏指标。
 * Iris Rail 为系统原生红黄绿预留左侧安全区，不再动态切换窗口装饰。
 */
export function useMacOSWindowChromeSync(): void {
  useEffect(() => {
    if (!isMacOSDesktopChrome()) return;

    const win = getCurrentWindow();
    let debounce: ReturnType<typeof setTimeout> | undefined;

    const scheduleReapply = () => {
      if (debounce !== undefined) clearTimeout(debounce);
      debounce = setTimeout(() => {
        void (async () => {
          const fullscreen = await win.isFullscreen();
          await syncFullscreenChromeState(fullscreen);
        })();
      }, DEBOUNCE_MS);
    };

    void (async () => {
      await syncChromeMetrics();
      const fullscreen = await win.isFullscreen();
      await syncFullscreenChromeState(fullscreen);
    })();

    const unlistenPromise = Promise.all([
      win.onResized(scheduleReapply),
      win.onScaleChanged(scheduleReapply),
      win.onFocusChanged((focused) => {
        if (focused) scheduleReapply();
      }),
    ]);

    return () => {
      if (debounce !== undefined) clearTimeout(debounce);
      void unlistenPromise.then((unlisteners) => {
        for (const unlisten of unlisteners) unlisten();
      });
    };
  }, []);
}
