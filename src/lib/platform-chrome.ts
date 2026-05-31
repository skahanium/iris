import { isTauriRuntime } from "@/lib/tauri-runtime";

/** macOS 桌面 Tauri（Overlay 标题栏 + 系统交通灯） */
export function isMacOSDesktopChrome(): boolean {
  return isTauriRuntime() && /Mac/i.test(navigator.userAgent);
}

/** Windows / Linux：无边框 + 自定义最小化/最大化/关闭 */
export function showCustomWindowControls(): boolean {
  return isTauriRuntime() && !isMacOSDesktopChrome();
}
