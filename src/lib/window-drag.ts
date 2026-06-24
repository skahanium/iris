import { getCurrentWindow } from "@tauri-apps/api/window";
import type { MouseEvent as ReactMouseEvent } from "react";

import { isTauriRuntime } from "@/lib/tauri-runtime";
import { toggleWindowMaximize } from "@/lib/window-actions";

const INTERACTIVE =
  "button, a, input, select, textarea, [contenteditable='true'], [data-tauri-drag-region-exclude]";

function isInteractiveTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return Boolean(target.closest(INTERACTIVE));
}

/**
 * 无边框窗口拖动（Windows WebView2 上比仅靠 data-tauri-drag-region 更可靠）。
 * 挂在标题栏/Tab 栏容器上；按钮等交互元素自动排除。
 */
export function createWindowDragMouseDown(
  win: ReturnType<typeof getCurrentWindow>,
) {
  return (event: ReactMouseEvent) => {
    if (event.button !== 0) return;
    if (isInteractiveTarget(event.target)) return;

    if (event.detail === 2) {
      void toggleWindowMaximize(win);
      return;
    }

    event.preventDefault();
    void win.startDragging();
  };
}

export function windowDragMouseDownProps():
  | { onMouseDown: ReturnType<typeof createWindowDragMouseDown> }
  | Record<string, never> {
  if (!isTauriRuntime()) return {};
  const win = getCurrentWindow();
  return { onMouseDown: createWindowDragMouseDown(win) };
}
