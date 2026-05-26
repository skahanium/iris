/** 是否在 Tauri WebView 内运行（非纯浏览器 Vite 预览） */
export function isTauriRuntime(): boolean {
  if (typeof window === "undefined") return false;
  return "__TAURI_INTERNALS__" in window || "__TAURI__" in window;
}
