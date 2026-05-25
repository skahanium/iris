import { forwardRef, useEffect, useImperativeHandle, useState } from "react";

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

const ICONS: Record<string, string> = {
  FileText: "📄",
  ListTree: "🌲",
  Lightbulb: "💡",
  Languages: "🔤",
  Globe: "🌐",
};

export const SlashCommandList = forwardRef<
  SlashCommandListRef,
  SlashCommandListProps
>(function SlashCommandList({ items, command }, ref) {
  const [selected, setSelected] = useState(0);

  useEffect(() => {
    setSelected(0);
  }, [items]);

  useImperativeHandle(ref, () => ({
    onKeyDown: ({ event }) => {
      if (event.key === "ArrowUp") {
        setSelected((i) => (i + items.length - 1) % items.length);
        return true;
      }
      if (event.key === "ArrowDown") {
        setSelected((i) => (i + 1) % items.length);
        return true;
      }
      if (event.key === "Enter") {
        const item = items[selected];
        if (item) command(item);
        return true;
      }
      return false;
    },
  }));

  return (
    <div className="z-50 min-w-[180px] overflow-hidden rounded-md border border-primary/20 bg-panel py-1 text-sm shadow-lg">
      {items.map((item, i) => (
        <button
          key={item.id}
          type="button"
          className={
            i === selected
              ? "flex w-full items-center gap-2 bg-muted px-3 py-1.5 text-left"
              : "flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-muted/60"
          }
          onClick={() => command(item)}
        >
          <span className="text-xs">{ICONS[item.icon ?? ""] ?? ""}</span>
          <span>{item.label}</span>
        </button>
      ))}
    </div>
  );
});
