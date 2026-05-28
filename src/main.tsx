import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { isTauriRuntime } from "@/lib/tauri-runtime";
import "tippy.js/dist/tippy.css";
import "./styles/globals.css";

if (isTauriRuntime()) {
  document.documentElement.dataset.irisDesktop = "";
  // Windows 11：非透明窗口 + shadow 由 DWM 提供圆角；macOS/Linux 用透明 WebView + CSS 裁切
  if (!/Windows/i.test(navigator.userAgent)) {
    document.documentElement.dataset.irisDesktopTransparent = "";
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </StrictMode>,
);
