import { memo, useMemo } from "react";

import { filterReferencedWebCitations } from "@/lib/ai/citation-display";
import { cn } from "@/lib/utils";
import type { WebCitationEntry } from "@/types/ai";

interface AssistantCitationFooterProps {
  content: string;
  entries: WebCitationEntry[];
  referencedOnly?: boolean;
  className?: string;
  onOpenUrl?: (url: string) => void;
}

/** Persisted HTTPS web sources for one assistant message (below prose body). */
export const AssistantCitationFooter = memo(function AssistantCitationFooter({
  content,
  entries,
  referencedOnly = true,
  className,
  onOpenUrl,
}: AssistantCitationFooterProps) {
  const visible = useMemo(
    () => filterReferencedWebCitations(entries, content, referencedOnly),
    [content, entries, referencedOnly],
  );

  if (visible.length === 0) {
    return null;
  }

  return (
    <section
      className={cn(
        "assistant-citation-footer mt-3 border-t border-border-subtle pt-2.5",
        className,
      )}
      aria-label="引用来源"
    >
      <h4 className="mb-1.5 text-caption font-medium text-muted-foreground">
        来源
      </h4>
      <ol className="m-0 list-none space-y-1 p-0 text-caption text-foreground/90">
        {visible.map((entry) => (
          <li key={entry.index} className="flex gap-1.5 leading-snug">
            <span
              className="shrink-0 tabular-nums text-muted-foreground"
              aria-hidden="true"
            >
              {entry.index}.
            </span>
            {onOpenUrl ? (
              <button
                type="button"
                className="min-w-0 text-left text-foreground/90 underline decoration-border-subtle underline-offset-2 hover:text-foreground hover:decoration-foreground/40"
                onClick={() => onOpenUrl(entry.url)}
              >
                {entry.title.trim() || entry.url}
              </button>
            ) : (
              <span className="min-w-0">{entry.title.trim() || entry.url}</span>
            )}
          </li>
        ))}
      </ol>
    </section>
  );
});
