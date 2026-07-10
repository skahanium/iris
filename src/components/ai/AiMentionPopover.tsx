import { FileText, Folder, Hash } from "lucide-react";
import { useLayoutEffect, useRef } from "react";

import {
  IrisSurfaceMenuGroup,
  IrisSurfaceMenuItem,
  IrisSurfaceMenuPanel,
} from "@/components/ui/iris-surface-menu";
import { ensureOptionVisible } from "@/lib/command-palette-scroll";
import type { MentionCandidate } from "@/lib/ai-context-scope";
import { cn } from "@/lib/utils";

interface AiMentionPopoverProps {
  open: boolean;
  query: string;
  prefix: "@" | "#";
  candidates: MentionCandidate[];
  highlight: number;
  onHighlight: (index: number) => void;
  navDeltaRef: React.MutableRefObject<1 | -1 | 0>;
  onSelect: (candidate: MentionCandidate) => void;
  className?: string;
}

/** @ 补全（文件夹/文档）和 # 补全（标签）列表。 */
export function AiMentionPopover({
  open,
  query: _query,
  prefix,
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

  const tags = candidates.filter((c) => c.kind === "tag");
  const folders = candidates.filter((c) => c.kind === "folder");
  const files = candidates.filter((c) => c.kind === "file");

  let index = 0;

  return (
    <IrisSurfaceMenuPanel
      className={cn("max-h-52 w-full", className)}
      role="listbox"
      aria-label={prefix === "@" ? "@ 范围补全" : "# 标签补全"}
    >
      <div
        ref={listRef}
        className="max-h-52 overflow-y-auto overscroll-contain outline-none"
      >
        {candidates.length === 0 ? (
          <p className="px-3 py-3 text-center text-xs text-muted-foreground">
            无匹配项
          </p>
        ) : (
          <>
            {tags.length > 0 ? (
              <IrisSurfaceMenuGroup title="标签">
                {tags.map((c) => {
                  const i = index++;
                  return (
                    <IrisSurfaceMenuItem
                      key={c.id}
                      id={c.id}
                      label={c.label}
                      active={highlight === i}
                      icon={<Hash className="h-4 w-4" />}
                      buttonRef={(el) => {
                        optionRefs.current[i] = el;
                      }}
                      onMouseEnter={() => onHighlight(i)}
                      onSelect={() => onSelect(c)}
                    />
                  );
                })}
              </IrisSurfaceMenuGroup>
            ) : null}
            {folders.length > 0 ? (
              <IrisSurfaceMenuGroup title="文件夹">
                {folders.map((c) => {
                  const i = index++;
                  return (
                    <IrisSurfaceMenuItem
                      key={c.id}
                      id={c.id}
                      label={c.label}
                      subtitle={
                        c.subtitle && c.subtitle !== c.label
                          ? c.subtitle
                          : undefined
                      }
                      active={highlight === i}
                      icon={<Folder className="h-4 w-4" />}
                      buttonRef={(el) => {
                        optionRefs.current[i] = el;
                      }}
                      onMouseEnter={() => onHighlight(i)}
                      onSelect={() => onSelect(c)}
                    />
                  );
                })}
              </IrisSurfaceMenuGroup>
            ) : null}
            {files.length > 0 ? (
              <IrisSurfaceMenuGroup title="文档">
                {files.map((c) => {
                  const i = index++;
                  return (
                    <IrisSurfaceMenuItem
                      key={c.id}
                      id={c.id}
                      label={c.label}
                      subtitle={c.subtitle}
                      active={highlight === i}
                      icon={<FileText className="h-4 w-4" />}
                      buttonRef={(el) => {
                        optionRefs.current[i] = el;
                      }}
                      onMouseEnter={() => onHighlight(i)}
                      onSelect={() => onSelect(c)}
                    />
                  );
                })}
              </IrisSurfaceMenuGroup>
            ) : null}
          </>
        )}
      </div>
    </IrisSurfaceMenuPanel>
  );
}
