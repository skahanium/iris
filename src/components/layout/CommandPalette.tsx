import { Search, X } from "lucide-react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  filterCommandPaletteItems,
  groupCommandPaletteItems,
  type CommandPaletteItem,
} from "@/lib/command-palette";
import { ensureOptionVisible } from "@/lib/command-palette-scroll";
import { cn } from "@/lib/utils";

interface CommandPaletteProps {
  open: boolean;
  items: CommandPaletteItem[];
  onClose: () => void;
  onSelect: (item: CommandPaletteItem) => void;
}

function ShortcutBadge({
  children,
  active = false,
}: {
  children: string;
  active?: boolean;
}) {
  return (
    <kbd
      className={cn(
        "shrink-0 rounded-md border px-1.5 py-0.5 font-sans text-[11px] leading-none transition-colors duration-base ease-iris-out motion-reduce:transition-none",
        active
          ? "border-border bg-panel/80 text-foreground/80"
          : "border-border/80 bg-muted/40 text-muted-foreground",
      )}
    >
      {children}
    </kbd>
  );
}

export function CommandPalette({
  open,
  items,
  onClose,
  onSelect,
}: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [highlight, setHighlight] = useState(0);

  const filtered = useMemo(
    () => filterCommandPaletteItems(items, query),
    [items, query],
  );

  const itemRefs = useRef(new Map<string, HTMLButtonElement>());
  const listViewportRef = useRef<HTMLDivElement | null>(null);
  const wasOpenRef = useRef(false);
  const prevQueryRef = useRef(query);
  const highlightRef = useRef(highlight);
  const filteredRef = useRef(filtered);
  const navDeltaRef = useRef<1 | -1 | 0>(0);
  highlightRef.current = highlight;
  filteredRef.current = filtered;
  const grouped = useMemo(
    () => groupCommandPaletteItems(filtered),
    [filtered],
  );

  const flatIndex = useMemo(() => {
    const index = new Map<string, number>();
    let i = 0;
    for (const item of filtered) {
      index.set(item.id, i);
      i += 1;
    }
    return index;
  }, [filtered]);

  useEffect(() => {
    if (!open) {
      wasOpenRef.current = false;
      return;
    }
    if (!wasOpenRef.current) {
      setQuery("");
      prevQueryRef.current = "";
      setHighlight(0);
      wasOpenRef.current = true;
    }
  }, [open, items]);

  useEffect(() => {
    if (!open) return;
    if (prevQueryRef.current === query) return;
    prevQueryRef.current = query;
    setHighlight(0);
  }, [open, query]);

  const scrollHighlightIntoView = useCallback(() => {
    const index = highlightRef.current;
    const item = filteredRef.current[index];
    if (!item) return;
    const el = itemRefs.current.get(item.id);
    if (!el) return;

    const viewport =
      listViewportRef.current ??
      el.closest<HTMLElement>("[data-radix-scroll-area-viewport]");
    if (!viewport) return;

    ensureOptionVisible(viewport, el, navDeltaRef.current);
    navDeltaRef.current = 0;
  }, []);

  useEffect(() => {
    if (highlight >= filtered.length) {
      setHighlight(Math.max(0, filtered.length - 1));
    }
  }, [filtered.length, highlight]);

  useLayoutEffect(() => {
    if (!open) return;
    scrollHighlightIntoView();
  }, [open, highlight, scrollHighlightIntoView]);

  const moveHighlight = useCallback(
    (delta: 1 | -1) => {
      if (filteredRef.current.length === 0) return;
      navDeltaRef.current = delta;
      setHighlight((current) => {
        const len = filteredRef.current.length;
        const next = current + delta;
        if (next < 0 || next >= len) {
          navDeltaRef.current = 0;
          return current;
        }
        return next;
      });
    },
    [],
  );

  const runHighlighted = () => {
    const item = filtered[highlight];
    if (!item || item.disabled) return;
    onSelect(item);
  };

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      moveHighlight(1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      moveHighlight(-1);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      runHighlighted();
    }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent
        size="palette"
        showClose={false}
        className="gap-0 overflow-hidden p-0"
      >
        <DialogHeader className="sr-only">
          <DialogTitle>命令面板</DialogTitle>
        </DialogHeader>

        <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3">
          <Search
            className="h-4 w-4 shrink-0 text-muted-foreground"
            aria-hidden
          />
          <input
            type="search"
            className="min-w-0 flex-1 bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
            placeholder="搜索命令…"
            value={query}
            autoFocus
            aria-label="搜索命令"
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
          />
          <DialogClose asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-8 w-8 shrink-0"
              aria-label="关闭命令面板"
            >
              <X className="h-4 w-4" />
            </Button>
          </DialogClose>
        </div>

        <ScrollArea
          className="h-[min(28rem,58vh)]"
          viewportRef={listViewportRef}
          scrollbarVisibility="always"
        >
          {filtered.length === 0 ? (
            <p className="px-5 py-10 text-center text-sm text-muted-foreground">
              无匹配命令
            </p>
          ) : (
            <div
              className="py-2"
              role="listbox"
              aria-label="命令列表"
              aria-activedescendant={
                filtered[highlight]
                  ? `command-palette-option-${filtered[highlight].id}`
                  : undefined
              }
            >
              {grouped.map(({ group, items: groupItems }) => (
                <section key={group} role="presentation">
                  <p className="px-4 pb-1 pt-2 text-[11px] font-medium tracking-wider text-muted-foreground">
                    {group}
                  </p>
                  {groupItems.map((item) => {
                    const index = flatIndex.get(item.id) ?? 0;
                    const active = index === highlight;
                    return (
                      <div key={item.id} className="px-2.5 py-0.5">
                        <button
                          id={`command-palette-option-${item.id}`}
                          ref={(el) => {
                            if (el) itemRefs.current.set(item.id, el);
                            else itemRefs.current.delete(item.id);
                          }}
                          type="button"
                          role="option"
                          aria-disabled={item.disabled || undefined}
                          aria-selected={active}
                          className={cn(
                            "relative flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left text-[15px] leading-snug",
                            "transition-[background-color,box-shadow,color] duration-base ease-iris-out motion-reduce:transition-none",
                            item.disabled && !active && "text-muted-foreground/75",
                            active && item.disabled
                              ? "z-[1] bg-muted text-muted-foreground shadow-[inset_0_0_0_1px_hsl(var(--border))]"
                              : active && !item.disabled
                                ? "z-[1] bg-primary/[0.2] font-medium text-foreground shadow-[inset_0_0_0_1px_hsl(var(--primary)/0.3)] [.light_&]:bg-primary/[0.14] [.light_&]:shadow-[inset_0_0_0_1px_hsl(var(--primary)/0.22)]"
                                : "bg-transparent text-foreground/85",
                            !active && "hover:bg-muted/60",
                          )}
                          onMouseEnter={() => {
                            navDeltaRef.current = 0;
                            setHighlight(index);
                          }}
                          onClick={() => {
                            if (item.disabled) return;
                            onSelect(item);
                          }}
                        >
                          <span
                            className={cn(
                              "absolute bottom-2 left-2 top-2 w-0.5 rounded-full transition-opacity duration-base ease-iris-out motion-reduce:transition-none",
                              active && !item.disabled
                                ? "bg-primary opacity-100"
                                : active && item.disabled
                                  ? "bg-muted-foreground/50 opacity-100"
                                  : "bg-primary opacity-0",
                            )}
                            aria-hidden
                          />
                          <span className="min-w-0 flex-1 truncate pl-1.5">
                            {item.label}
                          </span>
                          {item.shortcut ? (
                            <ShortcutBadge active={active}>
                              {item.shortcut}
                            </ShortcutBadge>
                          ) : null}
                        </button>
                      </div>
                    );
                  })}
                </section>
              ))}
            </div>
          )}
        </ScrollArea>

        <div className="flex h-10 shrink-0 items-center justify-between gap-3 border-t border-border bg-muted/20 px-4 text-[11px] text-muted-foreground">
          <span>
            {filtered.length} 条命令 · ↑↓ 选择 · Enter 执行
          </span>
          <span className="shrink-0">Esc 关闭</span>
        </div>
      </DialogContent>
    </Dialog>
  );
}
