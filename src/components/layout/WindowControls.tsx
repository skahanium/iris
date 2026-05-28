import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

import { cn } from "@/lib/utils";

function WindowControlButton({
  label,
  onClick,
  className,
  children,
}: {
  label: string;
  onClick: () => void;
  className?: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      data-tauri-drag-region-exclude
      className={cn(
        "inline-flex h-9 w-10 items-center justify-center text-muted-foreground transition-colors duration-fast hover:bg-muted/80 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary",
        className,
      )}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

/** 无边框窗口：最小化 / 最大化 / 关闭（仅 Tauri） */
export function WindowControls() {
  const win = useMemo(() => getCurrentWindow(), []);
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void win.isMaximized().then((v) => {
      if (!cancelled) setMaximized(v);
    });
    const unlisten = win.onResized(() => {
      void win.isMaximized().then((v) => {
        if (!cancelled) setMaximized(v);
      });
    });
    return () => {
      cancelled = true;
      void unlisten.then((fn) => fn());
    };
  }, [win]);

  const minimize = useCallback(() => {
    void win.minimize();
  }, [win]);

  const toggleMaximize = useCallback(() => {
    void win.toggleMaximize();
  }, [win]);

  const close = useCallback(() => {
    void win.close();
  }, [win]);

  return (
    <div className="flex shrink-0 items-stretch" data-tauri-drag-region-exclude>
      <WindowControlButton label="最小化" onClick={minimize}>
        <Minus className="h-3.5 w-3.5" strokeWidth={1.75} />
      </WindowControlButton>
      <WindowControlButton
        label={maximized ? "还原" : "最大化"}
        onClick={toggleMaximize}
      >
        <Square className="h-3 w-3" strokeWidth={1.75} />
      </WindowControlButton>
      <WindowControlButton
        label="关闭"
        onClick={close}
        className="hover:bg-destructive hover:text-destructive-foreground"
      >
        <X className="h-3.5 w-3.5" strokeWidth={1.75} />
      </WindowControlButton>
    </div>
  );
}
