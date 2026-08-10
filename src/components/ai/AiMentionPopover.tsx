import { FileText, Folder, Hash } from "lucide-react";
import {
  forwardRef,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import {
  IrisSurfaceMenuItem,
  IrisSurfaceMenuPanel,
} from "@/components/ui/iris-surface-menu";
import { ensureOptionVisible } from "@/lib/command-palette-scroll";
import type { MentionCandidate } from "@/lib/ai-context-scope";

export interface AiMentionPopoverRef {
  onKeyDown: (event: KeyboardEvent) => boolean;
}

interface AiMentionPopoverProps {
  prefix: "@" | "#";
  query: string;
  items: MentionCandidate[];
  command: (candidate: MentionCandidate) => void;
}

/** IrisSurfaceMenu-backed popup used by the TipTap mention suggestion plugin. */
export const AiMentionPopover = forwardRef<
  AiMentionPopoverRef,
  AiMentionPopoverProps
>(function AiMentionPopover({ prefix, query: _query, items, command }, ref) {
  const listRef = useRef<HTMLDivElement>(null);
  const optionRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [highlight, setHighlight] = useState(0);
  const safeHighlight = Math.min(highlight, Math.max(0, items.length - 1));

  useLayoutEffect(() => {
    setHighlight(0);
  }, [items]);

  useLayoutEffect(() => {
    const list = listRef.current;
    const option = optionRefs.current[safeHighlight];
    if (!list || !option) return;
    ensureOptionVisible(list, option, 1);
  }, [safeHighlight]);

  useImperativeHandle(
    ref,
    () => ({
      onKeyDown(event) {
        if (event.key === "ArrowDown") {
          event.preventDefault();
          setHighlight((current) => Math.min(current + 1, items.length - 1));
          return true;
        }
        if (event.key === "ArrowUp") {
          event.preventDefault();
          setHighlight((current) => Math.max(current - 1, 0));
          return true;
        }
        if (event.key === "Enter" || event.key === "Tab") {
          const item = items[safeHighlight];
          if (!item) return false;
          event.preventDefault();
          command(item);
          return true;
        }
        return false;
      },
    }),
    [command, items, safeHighlight],
  );

  return (
    <IrisSurfaceMenuPanel
      className="max-h-64 w-[min(26rem,calc(100vw-2rem))]"
      role="listbox"
      aria-label={prefix === "@" ? "@ 文件和文件夹" : "# 标签"}
    >
      <div
        ref={listRef}
        className="max-h-64 overflow-y-auto overscroll-contain"
      >
        {items.length === 0 ? (
          <p className="px-3 py-3 text-center text-xs text-muted-foreground">
            无匹配项
          </p>
        ) : (
          items.map((item, current) => {
            const icon =
              item.kind === "tag" ? (
                <Hash className="h-4 w-4" />
              ) : item.kind === "folder" ? (
                <Folder className="h-4 w-4" />
              ) : (
                <FileText className="h-4 w-4" />
              );
            return (
              <IrisSurfaceMenuItem
                key={item.id}
                id={item.id}
                label={item.label}
                subtitle={item.subtitle}
                active={safeHighlight === current}
                icon={icon}
                buttonRef={(element) => {
                  optionRefs.current[current] = element;
                }}
                onMouseEnter={() => setHighlight(current)}
                onSelect={() => command(item)}
              />
            );
          })
        )}
      </div>
    </IrisSurfaceMenuPanel>
  );
});
