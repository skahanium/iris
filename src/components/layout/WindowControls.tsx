import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";

import { cn } from "@/lib/utils";

function stopTitlebarDrag(event: ReactMouseEvent) {
  event.stopPropagation();
}

function WindowControlButton({
  label,
  onClick,
  className,
}: {
  label: string;
  onClick: () => void;
  className?: string;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      data-tauri-drag-region-exclude
      className={cn(
        "iris-window-control iris-traffic-light inline-flex size-3 items-center justify-center rounded-full transition-[box-shadow,transform,filter] duration-fast focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-2 focus-visible:ring-offset-panel active:scale-95",
        className,
      )}
      onMouseDown={stopTitlebarDrag}
      onPointerDown={stopTitlebarDrag}
      onClick={onClick}
    />
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
    <div
      className="iris-window-controls relative z-20 flex h-[var(--titlebar-height)] shrink-0 items-center justify-center gap-2 px-4"
      data-tauri-drag-region-exclude
      onMouseDown={stopTitlebarDrag}
      onPointerDown={stopTitlebarDrag}
    >
      <WindowControlButton
        label="关闭"
        onClick={close}
        className="iris-traffic-light--close"
      />
      <WindowControlButton
        label="最小化"
        onClick={minimize}
        className="iris-traffic-light--minimize"
      />
      <WindowControlButton
        label={maximized ? "还原" : "最大化"}
        onClick={toggleMaximize}
        className="iris-traffic-light--maximize"
      />
    </div>
  );
}
