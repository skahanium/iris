import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { ToastProvider } from "./components/ui/toast";
import { isTauriRuntime } from "@/lib/tauri-runtime";
import "./styles/globals.css";

function bootstrapStoredTheme() {
  try {
    const storedTheme = localStorage.getItem("iris-theme");
    document.documentElement.classList.toggle("light", storedTheme === "light");
  } catch {
    document.documentElement.classList.remove("light");
  }
}

bootstrapStoredTheme();

if (isTauriRuntime()) {
  document.documentElement.dataset.irisDesktop = "";
  // Windows/macOS 使用系统非透明窗口；Linux 用透明 WebView + CSS 裁切。
  if (
    !/Windows/i.test(navigator.userAgent) &&
    !/Mac/i.test(navigator.userAgent)
  ) {
    document.documentElement.dataset.irisDesktopTransparent = "";
  }
  if (/Mac/i.test(navigator.userAgent)) {
    document.documentElement.dataset.irisPlatformMacos = "";
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary>
      <ToastProvider>
        <App />
      </ToastProvider>
    </ErrorBoundary>
  </StrictMode>,
);
