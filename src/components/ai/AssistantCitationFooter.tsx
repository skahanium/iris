import { memo, useMemo } from "react";

import { filterReferencedWebCitations } from "@/lib/ai/citation-display";
import { cn } from "@/lib/utils";
import type { CitationBinding, WebCitationEntry } from "@/types/ai";

interface AssistantCitationFooterProps {
  content: string;
  entries: WebCitationEntry[];
  binding?: CitationBinding;
  referencedOnly?: boolean;
  className?: string;
  onOpenUrl?: (url: string) => void;
}

/** Persisted HTTPS web sources for one assistant message (below prose body). */
export const AssistantCitationFooter = memo(function AssistantCitationFooter({
  content,
  entries,
  binding,
  referencedOnly = true,
  className,
  onOpenUrl,
}: AssistantCitationFooterProps) {
  const sourceGroup = binding?.mode === "source_group_fallback";
  const visible = useMemo(
    () =>
      filterReferencedWebCitations(
        entries,
        content,
        sourceGroup ? false : referencedOnly,
      ),
    [content, entries, referencedOnly, sourceGroup],
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
      aria-label={sourceGroup ? "本轮已核验证据" : "引用来源"}
    >
      <h4 className="mb-1.5 text-caption font-medium text-muted-foreground">
        {sourceGroup ? "本轮已核验证据" : "来源"}
      </h4>
      {sourceGroup ? (
        <p className="mb-1.5 text-caption text-muted-foreground">
          本回答未提供可精确绑定的行内引用；以下为本轮核验证据范围。
        </p>
      ) : null}
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
