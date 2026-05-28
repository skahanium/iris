import type { ReactNode } from "react";

import { isTauriRuntime } from "@/lib/tauri-runtime";

interface DesktopFrameProps {
  children: ReactNode;
}

function usesTransparentDesktopShell(): boolean {
  if (!isTauriRuntime()) return false;
  if (/Windows/i.test(navigator.userAgent)) return false;
  return true;
}

/** Tauri 桌面壳：顶栏由 TabBar / MinimalWindowChrome 提供；非 Windows 另做透明圆角裁切 */
export function DesktopFrame({ children }: DesktopFrameProps) {
  if (!isTauriRuntime()) {
    return <>{children}</>;
  }

  const transparentShell = usesTransparentDesktopShell();

  return (
    <div
      className={
        transparentShell
          ? "iris-desktop-frame flex h-dvh flex-col overflow-hidden bg-background"
          : "flex h-dvh flex-col overflow-hidden bg-background"
      }
    >
      {children}
    </div>
  );
}
