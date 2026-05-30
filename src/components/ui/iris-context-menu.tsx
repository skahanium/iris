import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";

import {
  IrisSurfaceMenuGroup,
  IrisSurfaceMenuItem,
  IrisSurfaceMenuPanel,
} from "@/components/ui/iris-surface-menu";
import { resolveCommandIcon } from "@/lib/command-palette-icons";
import { cn } from "@/lib/utils";

export interface IrisContextMenuItem {
  id: string;
  label: string;
  icon?: string;
  disabled?: boolean;
}

export interface IrisContextMenuGroup {
  group: string;
  items: IrisContextMenuItem[];
}

interface IrisContextMenuProps {
  open: boolean;
  x: number;
  y: number;
  groups: IrisContextMenuGroup[];
  onSelect: (id: string) => void;
  onClose: () => void;
  className?: string;
}

const MENU_MAX_H = 320;
const PADDING = 8;

function clampPosition(x: number, y: number, width: number, height: number) {
  const maxX = window.innerWidth - width - PADDING;
  const maxY = window.innerHeight - height - PADDING;
  return {
    left: Math.max(PADDING, Math.min(x, maxX)),
    top: Math.max(PADDING, Math.min(y, maxY)),
  };
}

/** 完全自定义右键菜单（屏蔽原生菜单后使用） */
export function IrisContextMenu({
  open,
  x,
  y,
  groups,
  onSelect,
  onClose,
  className,
}: IrisContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (e: MouseEvent) => {
      const el = menuRef.current;
      if (el?.contains(e.target as Node)) return;
      onClose();
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const onScroll = () => onClose();
    window.addEventListener("mousedown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      window.removeEventListener("mousedown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [open, onClose]);

  if (!open || groups.every((g) => g.items.length === 0)) return null;

  const estimatedH = Math.min(
    MENU_MAX_H,
    groups.reduce((sum, g) => sum + g.items.length * 36 + 24, 0),
  );
  const pos = clampPosition(x, y, 220, estimatedH);

  return createPortal(
    <div
      ref={menuRef}
      className={cn("fixed z-[var(--z-context-menu,9500)]", className)}
      style={{
        left: pos.left,
        top: pos.top,
        maxHeight: MENU_MAX_H,
      }}
      onContextMenu={(e) => e.preventDefault()}
    >
      <IrisSurfaceMenuPanel
        className="max-h-[inherit] min-w-[12.5rem] max-w-[16rem] overflow-auto"
        aria-label="上下文菜单"
      >
        {groups.map(({ group, items }) => (
          <IrisSurfaceMenuGroup key={group} title={group}>
            {items.map((item) => {
              const Icon = resolveCommandIcon(item.icon);
              return (
                <IrisSurfaceMenuItem
                  key={item.id}
                  id={`ctx-${item.id}`}
                  label={item.label}
                  disabled={item.disabled}
                  icon={Icon ? <Icon className="h-4 w-4" /> : undefined}
                  onSelect={() => {
                    if (item.disabled) return;
                    onSelect(item.id);
                    onClose();
                  }}
                />
              );
            })}
          </IrisSurfaceMenuGroup>
        ))}
      </IrisSurfaceMenuPanel>
    </div>,
    document.body,
  );
}
