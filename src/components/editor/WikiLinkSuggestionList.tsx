import { FileText } from "lucide-react";
import { forwardRef, useImperativeHandle, useRef } from "react";

import {
  IrisSurfaceMenuItem,
  IrisSurfaceMenuPanel,
} from "@/components/ui/iris-surface-menu";
import { useListboxKeyboard } from "@/hooks/useListboxKeyboard";
import type { WikiLinkSuggestionItem } from "@/lib/wiki-link-suggestions";

interface WikiLinkSuggestionListProps {
  items: WikiLinkSuggestionItem[];
  command: (item: WikiLinkSuggestionItem) => void;
}

export interface WikiLinkSuggestionListRef {
  onKeyDown: (props: { event: KeyboardEvent }) => boolean;
}

export const WikiLinkSuggestionList = forwardRef<
  WikiLinkSuggestionListRef,
  WikiLinkSuggestionListProps
>(function WikiLinkSuggestionList({ items, command }, ref) {
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
      className="iris-suggestion-menu z-slash-command max-h-[min(16rem,40vh)] min-w-[15rem] max-w-[min(32rem,calc(100vw-2rem))] rounded-[8px] border-border/70 bg-popover py-1 shadow-floating"
      aria-label="双链候选"
    >
      {items.length === 0 ? (
        <IrisSurfaceMenuItem
          id="wiki-link-empty"
          label="没有匹配的笔记"
          hint
          onSelect={() => {}}
        />
      ) : null}
      {items.map((item, i) => (
        <IrisSurfaceMenuItem
          key={item.id}
          id={`wiki-link-${item.id}`}
          label={item.title}
          subtitle={item.path}
          active={i === selected}
          icon={<FileText className="h-4 w-4" />}
          onMouseEnter={() => setSelected(i)}
          onSelect={() => command(item)}
        />
      ))}
    </IrisSurfaceMenuPanel>
  );
});
