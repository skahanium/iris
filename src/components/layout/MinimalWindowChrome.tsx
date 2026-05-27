import { useMemo } from "react";

import { isTauriRuntime } from "@/lib/tauri-runtime";
import { createWindowDragMouseDown } from "@/lib/window-drag";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { AppBrandZone } from "./AppBrandZone";
import { WindowControls } from "./WindowControls";

/** 无 Tab 时的单行顶栏（库选择 / 加载） */
export function MinimalWindowChrome() {
  const onDragMouseDown = useMemo(() => {
    if (!isTauriRuntime()) return undefined;
    return createWindowDragMouseDown(getCurrentWindow());
  }, []);

  if (!isTauriRuntime()) return null;

  return (
    <header
      className="flex h-9 shrink-0 cursor-default select-none items-stretch border-b border-border bg-panel"
      data-tauri-drag-region
      onMouseDown={onDragMouseDown}
    >
      <AppBrandZone className="min-w-0 flex-1 justify-start px-5" />
      <WindowControls />
    </header>
  );
}
