import { ChevronRight, FileSearch } from "lucide-react";

import { Button } from "@/components/ui/button";
import { MarkdownRenderable } from "@/components/ai/MarkdownRenderable";
import type { ResearchFocusPayload } from "@/types/ai";
import { cn } from "@/lib/utils";

interface ResearchResultMessageProps {
  result: ResearchFocusPayload;
  onExpandDetail: () => void;
  className?: string;
}

function summaryPreview(summary: string, maxLen = 480): string {
  const t = summary.trim();
  if (t.length <= maxLen) return t;
  return `${t.slice(0, maxLen)}…`;
}

/** 对话时间线内的研究结果卡（非占位） */
export function ResearchResultMessage({
  result,
  onExpandDetail,
  className,
}: ResearchResultMessageProps) {
  const preview = summaryPreview(result.summary);
  const evidenceCount = result.evidence_matrix.total_evidence_count;
  const coverage = Math.round(result.evidence_matrix.coverage_score * 100);

  return (
    <article
      className={cn(
        "ai-message-assistant ai-message-bubble ai-message-bubble-assistant overflow-hidden",
        className,
      )}
      data-testid="research-result-message"
    >
      <header className="flex items-start gap-2 border-b border-border/50 px-3 py-2">
        <FileSearch
          className="mt-0.5 h-4 w-4 shrink-0 text-primary"
          aria-hidden
        />
        <div className="min-w-0 flex-1">
          <h3 className="text-[13px] font-semibold leading-snug text-foreground">
            {result.topic}
          </h3>
          <p className="mt-0.5 text-[11px] text-muted-foreground">
            {result.rounds} 轮研究 · {evidenceCount} 条证据 · 覆盖率 {coverage}%
          </p>
        </div>
      </header>
      <MarkdownRenderable
        content={preview}
        profile="research_card"
        streaming={false}
        className="px-3 py-2.5"
      />
      <footer className="border-t border-border/50 px-3 py-2">
        <Button
          type="button"
          variant="secondary"
          size="sm"
          className="h-8 gap-1 text-xs"
          onClick={onExpandDetail}
        >
          展开研究详情
          <ChevronRight className="h-3.5 w-3.5" />
        </Button>
      </footer>
    </article>
  );
}
