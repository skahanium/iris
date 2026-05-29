import { forwardRef, useImperativeHandle, useRef } from "react";

import { CommandListOption } from "@/components/ui/command-list";
import { useListboxKeyboard } from "@/hooks/useListboxKeyboard";
import { resolveCommandIcon } from "@/lib/command-palette-icons";

export interface SlashItem {
  id: string;
  label: string;
  icon?: string;
}

interface SlashCommandListProps {
  items: SlashItem[];
  command: (item: SlashItem) => void;
}

export interface SlashCommandListRef {
  onKeyDown: (props: { event: KeyboardEvent }) => boolean;
}

export const SlashCommandList = forwardRef<
  SlashCommandListRef,
  SlashCommandListProps
>(function SlashCommandList({ items, command }, ref) {
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
    <div className="z-slash-command min-w-[200px] overflow-hidden rounded-lg border border-border/80 bg-surface-elevated py-1 shadow-floating">
      {items.map((item, i) => (
        <CommandListOption
          key={item.id}
          id={`slash-${item.id}`}
          label={item.label}
          active={i === selected}
          icon={resolveCommandIcon(item.icon)}
          className="py-0"
          onMouseEnter={() => setSelected(i)}
          onSelect={() => command(item)}
        />
      ))}
    </div>
  );
});
