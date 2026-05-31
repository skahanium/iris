import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";

import {
  applyDesktopChromeMetricsToDocument,
  type DesktopChromeMetrics,
} from "@/lib/chrome-metrics";
import { getDesktopChromeMetrics, reapplyWindowChrome } from "@/lib/ipc";
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
 * macOS：全屏/缩放/聚焦后系统会重置 Overlay 交通灯位置，需在 Web 侧防抖重应用。
 * 全屏时交通灯在系统菜单栏，不与应用顶栏对齐；退出全屏后恢复 32px 契约。
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
            await reapplyWindowChrome();
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
