import { isTauriRuntime } from "@/lib/tauri-runtime";

/** macOS 桌面 Tauri（Overlay 标题栏 + Iris Rail 自定义窗口控件） */
export function isMacOSDesktopChrome(): boolean {
  return isTauriRuntime() && /Mac/i.test(navigator.userAgent);
}

/** Iris Rail: all desktop platforms use right-side custom controls. */
export function showCustomWindowControls(): boolean {
  return isTauriRuntime();
}
