import { FileText, Folder } from "lucide-react";
import { useLayoutEffect, useRef } from "react";

import {
  CommandListGroup,
  CommandListOption,
} from "@/components/ui/command-list";
import { ensureOptionVisible } from "@/lib/command-palette-scroll";
import type { MentionCandidate } from "@/lib/ai-context-scope";
import { cn } from "@/lib/utils";

interface AiMentionPopoverProps {
  open: boolean;
  query: string;
  candidates: MentionCandidate[];
  highlight: number;
  onHighlight: (index: number) => void;
  navDeltaRef: React.MutableRefObject<1 | -1 | 0>;
  onSelect: (candidate: MentionCandidate) => void;
  className?: string;
}

/** 嵌在输入框上方的 @ 补全列表（与输入区同宽，非 fixed 定位）。 */
export function AiMentionPopover({
  open,
  query,
  candidates,
  highlight,
  onHighlight,
  navDeltaRef,
  onSelect,
  className,
}: AiMentionPopoverProps) {
  const listRef = useRef<HTMLDivElement>(null);
  const optionRefs = useRef<(HTMLButtonElement | null)[]>([]);

  useLayoutEffect(() => {
    if (!open || navDeltaRef.current === 0) return;
    const list = listRef.current;
    const el = optionRefs.current[highlight];
    if (!list || !el) return;
    requestAnimationFrame(() => {
      ensureOptionVisible(list, el, navDeltaRef.current);
      navDeltaRef.current = 0;
    });
  }, [highlight, open, candidates.length, navDeltaRef]);

  if (!open) return null;

  const folders = candidates.filter((c) => c.kind === "folder");
  const files = candidates.filter((c) => c.kind === "file");

  let index = 0;

  return (
    <div
      className={cn(
        "overflow-hidden rounded-lg border border-border/80 bg-popover shadow-md",
        "ring-1 ring-border/30",
        className,
      )}
      role="listbox"
      aria-label="@ 范围补全"
    >
      <div
        ref={listRef}
        className="max-h-52 overflow-y-auto overscroll-contain py-1 outline-none"
      >
        {candidates.length === 0 ? (
          <p className="px-4 py-3 text-center text-xs text-muted-foreground">
            无匹配项
          </p>
        ) : (
          <>
            {folders.length > 0 && (
              <CommandListGroup
                title="文件夹"
                className="px-3 pb-0.5 pt-1.5 text-[10px]"
              />
            )}
            {folders.map((c) => {
              const i = index++;
              return (
                <CommandListOption
                  key={c.id}
                  id={c.id}
                  label={c.label}
                  query={query}
                  active={highlight === i}
                  icon={Folder}
                  subtitle={
                    c.subtitle && c.subtitle !== c.label ? c.subtitle : undefined
                  }
                  className="px-1.5 py-0"
                  buttonRef={(el) => {
                    optionRefs.current[i] = el;
                  }}
                  onMouseEnter={() => onHighlight(i)}
                  onSelect={() => onSelect(c)}
                />
              );
            })}
            {files.length > 0 && (
              <CommandListGroup
                title="文档"
                className="px-3 pb-0.5 pt-1.5 text-[10px]"
              />
            )}
            {files.map((c) => {
              const i = index++;
              return (
                <CommandListOption
                  key={c.id}
                  id={c.id}
                  label={c.label}
                  query={query}
                  active={highlight === i}
                  icon={FileText}
                  subtitle={c.subtitle}
                  className="px-1.5 py-0 [&_button]:text-sm"
                  buttonRef={(el) => {
                    optionRefs.current[i] = el;
                  }}
                  onMouseEnter={() => onHighlight(i)}
                  onSelect={() => onSelect(c)}
                />
              );
            })}
          </>
        )}
      </div>
    </div>
  );
}
