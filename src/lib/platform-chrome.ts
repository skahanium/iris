import { isTauriRuntime } from "@/lib/tauri-runtime";

/** macOS 桌面 Tauri（Decorated Overlay 标题栏 + 系统原生红黄绿） */
export function isMacOSDesktopChrome(): boolean {
  return isTauriRuntime() && /Mac/i.test(navigator.userAgent);
}

/** Windows 桌面 Tauri（无边框壳层 + Windows 风格自绘窗口控件） */
export function isWindowsDesktopChrome(): boolean {
  return isTauriRuntime() && /Windows/i.test(navigator.userAgent);
}

/** Iris Rail: Windows/Linux use right-side custom controls; macOS uses native traffic lights. */
export function showCustomWindowControls(): boolean {
  return isTauriRuntime() && !isMacOSDesktopChrome();
}
