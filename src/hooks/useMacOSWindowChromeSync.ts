import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";

import {
  applyDesktopChromeMetricsToDocument,
  type DesktopChromeMetrics,
} from "@/lib/chrome-metrics";
import { getDesktopChromeMetrics } from "@/lib/ipc";
import { isMacOSDesktopChrome } from "@/lib/platform-chrome";

const DEBOUNCE_MS = 48;

function setFullscreenDataset(fullscreen: boolean): void {
  const root = document.documentElement;
  if (fullscreen) {
    root.dataset.irisWindowFullscreen = "";
  } else {
    delete root.dataset.irisWindowFullscreen;
  }
}

async function syncChromeMetrics(): Promise<DesktopChromeMetrics> {
  const metrics = await getDesktopChromeMetrics();
  applyDesktopChromeMetricsToDocument(metrics);
  return metrics;
}

/**
 * macOS：全屏/缩放/聚焦后同步标题栏指标。
 * Iris Rail 使用右侧自定义窗口控件，左侧品牌轨不再为系统交通灯预留空间。
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
          setFullscreenDataset(fullscreen);
          if (!fullscreen) {
            await syncChromeMetrics();
          }
        })();
      }, DEBOUNCE_MS);
    };

    void (async () => {
      await syncChromeMetrics();
      const fullscreen = await win.isFullscreen();
      setFullscreenDataset(fullscreen);
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
