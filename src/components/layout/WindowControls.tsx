import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";

import {
  isMacOSDesktopChrome,
  isWindowsDesktopChrome,
} from "@/lib/platform-chrome";
import { cn } from "@/lib/utils";
import {
  toggleNativeFullscreen,
  toggleWindowMaximize,
} from "@/lib/window-actions";

function stopTitlebarDrag(event: ReactMouseEvent) {
  event.stopPropagation();
}

function MacTrafficLightButton({
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
        "iris-focus-soft iris-window-control iris-traffic-light inline-flex size-3.5 items-center justify-center rounded-full transition-[box-shadow,transform,filter] duration-fast focus:outline-none active:scale-95",
        className,
      )}
      onMouseDown={stopTitlebarDrag}
      onPointerDown={stopTitlebarDrag}
      onClick={onClick}
    />
  );
}

function WindowsControlButton({
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
        "iris-focus-soft iris-window-control iris-window-control--windows inline-flex h-[var(--titlebar-height)] w-11 items-center justify-center text-muted-foreground transition-[background-color,color,box-shadow] duration-fast focus:outline-none",
        className,
      )}
      onMouseDown={stopTitlebarDrag}
      onPointerDown={stopTitlebarDrag}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

/** 无边框窗口：最小化 / 最大化 / 关闭（仅 Tauri） */
export function WindowControls() {
  const win = useMemo(() => getCurrentWindow(), []);
  const [fullscreen, setFullscreen] = useState(false);
  const [maximized, setMaximized] = useState(false);
  const macOSControls = isMacOSDesktopChrome();
  const windowsControls = isWindowsDesktopChrome();

  useEffect(() => {
    if (macOSControls) return;

    let cancelled = false;
    const syncWindowState = () => {
      void Promise.all([win.isMaximized(), win.isFullscreen()]).then(
        ([nextMaximized, nextFullscreen]) => {
          if (cancelled) return;
          setMaximized(nextMaximized);
          setFullscreen(nextFullscreen);
        },
      );
    };

    syncWindowState();
    const unlisten = win.onResized(() => {
      syncWindowState();
    });
    return () => {
      cancelled = true;
      void unlisten.then((fn) => fn());
    };
  }, [macOSControls, win]);

  const minimize = useCallback(() => {
    void win.minimize();
  }, [win]);

  const toggleMaximize = useCallback(() => {
    void toggleWindowMaximize(win);
  }, [win]);

  const toggleFullscreen = useCallback(() => {
    void toggleNativeFullscreen(win).then(setFullscreen);
  }, [win]);

  const close = useCallback(() => {
    void win.close();
  }, [win]);

  if (macOSControls) {
    return null;
  }

  if (windowsControls) {
    return (
      <div
        className="iris-window-controls relative z-20 flex h-[var(--titlebar-height)] shrink-0 items-stretch justify-center"
        data-tauri-drag-region-exclude
        onMouseDown={stopTitlebarDrag}
        onPointerDown={stopTitlebarDrag}
      >
        <WindowsControlButton label="最小化" onClick={minimize}>
          <Minus className="h-4 w-4" strokeWidth={1.75} />
        </WindowsControlButton>
        <WindowsControlButton
          label={maximized ? "还原" : "最大化"}
          onClick={toggleMaximize}
        >
          {maximized ? (
            <Copy className="h-3.5 w-3.5" strokeWidth={1.7} />
          ) : (
            <Square className="h-3.5 w-3.5" strokeWidth={1.7} />
          )}
        </WindowsControlButton>
        <WindowsControlButton
          label="关闭"
          onClick={close}
          className="iris-window-control--close"
        >
          <X className="h-4 w-4" strokeWidth={1.75} />
        </WindowsControlButton>
      </div>
    );
  }

  return (
    <div
      className="iris-window-controls relative z-20 flex h-[var(--titlebar-height)] shrink-0 items-center justify-center gap-2.5 px-4"
      data-tauri-drag-region-exclude
      onMouseDown={stopTitlebarDrag}
      onPointerDown={stopTitlebarDrag}
    >
      <MacTrafficLightButton
        label={fullscreen ? "退出全屏" : "进入全屏"}
        onClick={toggleFullscreen}
        className="iris-traffic-light--maximize"
      />
      <MacTrafficLightButton
        label="最小化"
        onClick={minimize}
        className="iris-traffic-light--minimize"
      />
      <MacTrafficLightButton
        label="关闭"
        onClick={close}
        className="iris-traffic-light--close"
      />
    </div>
  );
}
