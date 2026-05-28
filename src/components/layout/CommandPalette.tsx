import { Command } from "lucide-react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import {
  CommandListGroup,
  CommandListOption,
} from "@/components/ui/command-list";
import { IrisOverlay } from "@/components/ui/iris-overlay";
import { Kbd, OverlayFooterHints } from "@/components/ui/kbd";
import {
  OverlayChrome,
  OverlaySearchHeader,
} from "@/components/ui/overlay-chrome";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useListboxKeyboard } from "@/hooks/useListboxKeyboard";
import {
  filterCommandPaletteItems,
  groupCommandPaletteItems,
  type CommandPaletteItem,
} from "@/lib/command-palette";
import { resolveCommandIcon } from "@/lib/command-palette-icons";
import { ensureOptionVisible } from "@/lib/command-palette-scroll";

interface CommandPaletteProps {
  open: boolean;
  items: CommandPaletteItem[];
  onClose: () => void;
  onSelect: (item: CommandPaletteItem) => void;
}

export function CommandPalette({
  open,
  items,
  onClose,
  onSelect,
}: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const itemRefs = useRef(new Map<string, HTMLButtonElement>());
  const listViewportRef = useRef<HTMLDivElement | null>(null);
  const wasOpenRef = useRef(false);
  const filteredRef = useRef<CommandPaletteItem[]>([]);
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;

  const filtered = useMemo(
    () => filterCommandPaletteItems(items, query),
    [items, query],
  );
  filteredRef.current = filtered;

  const { highlight, setHighlight, handleKeyDown, navDeltaRef } =
    useListboxKeyboard({
      length: filtered.length,
      enabled: open,
      resetKey: open ? query : "__closed__",
      onActivate: (index) => {
        const item = filteredRef.current[index];
        if (item && !item.disabled) onSelectRef.current(item);
      },
      isIndexDisabled: (index) =>
        Boolean(filteredRef.current[index]?.disabled),
    });

  const grouped = useMemo(() => groupCommandPaletteItems(filtered), [filtered]);

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
      wasOpenRef.current = true;
    }
  }, [open, items]);

  const scrollHighlightIntoView = useCallback(() => {
    const item = filteredRef.current[highlight];
    if (!item) return;
    const el = itemRefs.current.get(item.id);
    if (!el) return;

    const viewport =
      listViewportRef.current ??
      el.closest<HTMLElement>("[data-radix-scroll-area-viewport]");
    if (!viewport) return;

    ensureOptionVisible(viewport, el, navDeltaRef.current);
    navDeltaRef.current = 0;
  }, [highlight, navDeltaRef]);

  useLayoutEffect(() => {
    if (!open) return;
    const frame = requestAnimationFrame(() => {
      scrollHighlightIntoView();
    });
    return () => cancelAnimationFrame(frame);
  }, [open, highlight, scrollHighlightIntoView]);

  return (
    <IrisOverlay
      open={open}
      onClose={onClose}
      title="命令面板"
      size="palette"
      showTitleBar={false}
      bodyClassName="overflow-hidden"
    >
      <OverlayChrome
        header={
          <OverlaySearchHeader
            placeholder="搜索命令…"
            value={query}
            inputAriaLabel="搜索命令"
            onChange={setQuery}
            onKeyDown={handleKeyDown}
            onClose={onClose}
          />
        }
        footer={
          <OverlayFooterHints
            left={
              <>
                {filtered.length} 条命令 · <Kbd active>↑</Kbd>{" "}
                <Kbd active>↓</Kbd> 选择 · <Kbd active>Enter</Kbd> 执行
              </>
            }
            right={<Kbd>Esc</Kbd>}
          />
        }
      >
        <ScrollArea
          className="h-[min(28rem,58vh)]"
          viewportRef={listViewportRef}
          scrollbarVisibility="always"
        >
          {filtered.length === 0 ? (
            <div className="flex flex-col items-center gap-3 px-6 py-14 text-center">
              <div className="flex h-12 w-12 items-center justify-center rounded-xl border border-border/80 bg-surface-inset">
                <Command
                  className="h-6 w-6 text-muted-foreground"
                  strokeWidth={1.5}
                />
              </div>
              <div>
                <p className="text-sm font-medium text-foreground">
                  无匹配命令
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                  试试「搜索」「设置」或快捷键{" "}
                  <Kbd className="mx-0.5">Ctrl+Shift+P</Kbd>
                </p>
              </div>
            </div>
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
                  <CommandListGroup title={group} />
                  {groupItems.map((item) => {
                    const index = flatIndex.get(item.id) ?? 0;
                    const active = index === highlight;
                    return (
                      <CommandListOption
                        key={item.id}
                        id={`command-palette-option-${item.id}`}
                        label={item.label}
                        query={query}
                        active={active}
                        disabled={item.disabled}
                        shortcut={item.shortcut}
                        icon={resolveCommandIcon(item.icon)}
                        buttonRef={(el) => {
                          if (el) itemRefs.current.set(item.id, el);
                          else itemRefs.current.delete(item.id);
                        }}
                        onMouseEnter={() => {
                          navDeltaRef.current = 0;
                          setHighlight(index);
                        }}
                        onSelect={() => {
                          if (item.disabled) return;
                          onSelect(item);
                        }}
                      />
                    );
                  })}
                </section>
              ))}
            </div>
          )}
        </ScrollArea>
      </OverlayChrome>
    </IrisOverlay>
  );
}
