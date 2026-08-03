import { memo, useId, useMemo, useState } from "react";

import { ChevronDown } from "lucide-react";

import { filterReferencedWebCitations } from "@/lib/ai/citation-display";
import { cn } from "@/lib/utils";
import type {
  CitationBinding,
  SourceSummaryEntry,
  WebCitationEntry,
} from "@/types/ai";

interface AssistantCitationFooterProps {
  content: string;
  entries: WebCitationEntry[];
  binding?: CitationBinding;
  sourceSummary?: SourceSummaryEntry[];
  referencedOnly?: boolean;
  className?: string;
  onOpenUrl?: (url: string) => void;
}

/** Persisted HTTPS web sources for one assistant message (below prose body). */
export const AssistantCitationFooter = memo(function AssistantCitationFooter({
  content,
  entries,
  binding,
  sourceSummary = [],
  referencedOnly = true,
  className,
  onOpenUrl,
}: AssistantCitationFooterProps) {
  const [open, setOpen] = useState(false);
  const detailsId = useId();
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

  if (visible.length === 0 && sourceSummary.length === 0) {
    return null;
  }

  const title = sourceGroup ? "本次检索来源" : "来源";
  const summary = `${visible.length} 个来源`;
  const categorySummary = sourceSummary
    .map((entry) => `${sourceCategoryLabel(entry.category)} ${entry.count}`)
    .join(" · ");

  return (
    <section
      className={cn(
        "assistant-citation-footer mt-3 border-t border-border-subtle pt-2.5",
        className,
      )}
      aria-label={sourceGroup ? title : "引用来源"}
    >
      <button
        type="button"
        className="flex w-full min-w-0 items-center gap-1.5 text-left text-caption"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        aria-controls={detailsId}
        aria-label={open ? `折叠${title}` : `展开${title}`}
      >
        <ChevronDown
          className={cn(
            "h-3.5 w-3.5 shrink-0 transition-transform",
            !open && "-rotate-90",
          )}
        />
        <span className="shrink-0 font-medium text-foreground/75">{title}</span>
        <span className="min-w-0 truncate text-muted-foreground">
          {categorySummary || summary}
        </span>
      </button>
      {open ? (
        <div id={detailsId} className="mt-2">
          {sourceGroup ? (
            <p className="mb-1.5 text-caption text-muted-foreground">
              本回答未提供可精确绑定的行内引用；以下仅为本次检索来源，不表示已逐段核验。
            </p>
          ) : null}
          {categorySummary ? (
            <p className="mb-1.5 text-caption text-muted-foreground">
              {categorySummary}
            </p>
          ) : null}
          {visible.length > 0 ? (
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
                    <span className="min-w-0">
                      {entry.title.trim() || entry.url}
                    </span>
                  )}
                </li>
              ))}
            </ol>
          ) : null}
        </div>
      ) : null}
    </section>
  );
});

function sourceCategoryLabel(category: SourceSummaryEntry["category"]): string {
  const labels: Record<SourceSummaryEntry["category"], string> = {
    user_input: "用户输入",
    authorized_material: "授权材料",
    conversation_history: "对话历史",
    local_retrieval: "本地检索",
    web: "网页",
    external_tool: "外部工具",
    runtime_fact: "运行时事实",
    model_inference: "推断",
  };
  return labels[category];
}
