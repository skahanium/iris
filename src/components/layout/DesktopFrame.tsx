import type { ReactNode } from "react";

import { isTauriRuntime } from "@/lib/tauri-runtime";

interface DesktopFrameProps {
  children: ReactNode;
}

/** Tauri 桌面壳：圆角裁切容器（顶栏由 TabBar / MinimalWindowChrome 提供） */
export function DesktopFrame({ children }: DesktopFrameProps) {
  if (!isTauriRuntime()) {
    return <>{children}</>;
  }

  return (
    <div className="iris-desktop-frame flex h-dvh flex-col overflow-hidden bg-background">
      {children}
    </div>
  );
}
