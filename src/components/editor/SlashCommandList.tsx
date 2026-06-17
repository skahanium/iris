import { forwardRef, useImperativeHandle, useRef } from "react";

import {
  IrisSurfaceMenuItem,
  IrisSurfaceMenuPanel,
} from "@/components/ui/iris-surface-menu";
import { useListboxKeyboard } from "@/hooks/useListboxKeyboard";
import { resolveCommandIcon } from "@/lib/command-palette-icons";

export interface SlashItem {
  id: string;
  label: string;
  icon?: string;
  keywords?: string;
}

interface SlashCommandListProps {
  items: SlashItem[];
  command: (item: SlashItem) => void;
  selectionHint?: boolean;
}

export interface SlashCommandListRef {
  onKeyDown: (props: { event: KeyboardEvent }) => boolean;
}

export const SlashCommandList = forwardRef<
  SlashCommandListRef,
  SlashCommandListProps
>(function SlashCommandList({ items, command, selectionHint = false }, ref) {
  const commandRef = useRef(command);
  commandRef.current = command;
  const itemsRef = useRef(items);
  itemsRef.current = items;

  const itemsKey = items.map((item) => item.id).join(",");

  const {
    highlight: selected,
    setHighlight: setSelected,
    handleKeyDown,
  } = useListboxKeyboard({
    length: items.length,
    wrap: true,
    resetKey: itemsKey,
    onActivate: (index) => {
      const item = itemsRef.current[index];
      if (item) commandRef.current(item);
    },
  });

  useImperativeHandle(
    ref,
    () => ({
      onKeyDown: ({ event }) => handleKeyDown(event),
    }),
    [handleKeyDown],
  );

  return (
    <IrisSurfaceMenuPanel
      className="iris-suggestion-menu z-slash-command max-h-[min(16rem,40vh)] min-w-[12.5rem] max-w-[min(24rem,calc(100vw-2rem))] rounded-[8px] border-border/70 bg-popover py-1 shadow-floating"
      aria-label="斜杠命令"
    >
      {items.map((item, i) => {
        const Icon = resolveCommandIcon(item.icon);
        return (
          <IrisSurfaceMenuItem
            key={item.id}
            id={`slash-${item.id}`}
            label={item.label}
            active={i === selected}
            icon={Icon ? <Icon className="h-4 w-4" /> : undefined}
            onMouseEnter={() => setSelected(i)}
            onSelect={() => command(item)}
          />
        );
      })}
      {selectionHint ? (
        <IrisSurfaceMenuItem
          id="slash-selection-hint"
          label="选区操作请使用右键菜单"
          hint
          onSelect={() => {}}
        />
      ) : null}
    </IrisSurfaceMenuPanel>
  );
});
