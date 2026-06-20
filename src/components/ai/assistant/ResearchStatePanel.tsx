import { ShieldCheck } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { ResearchState } from "@/types/ai";

interface ResearchStatePanelProps {
  state: ResearchState | null;
}

function credibilityLabel(state: ResearchState): string {
  const highCount = state.sources.filter(
    (source) => source.credibility === "high",
  ).length;
  if (highCount === 0) return "暂无高可信证据";
  return `高可信证据 ${highCount} 条`;
}

function freshnessLabel(state: ResearchState): string {
  const needsCheck = state.sources.some((source) =>
    source.freshness.toLowerCase().includes("needs_check"),
  );
  return needsCheck ? "证据新鲜度需核验" : "证据新鲜度已记录";
}

function sourceFreshnessLabel(value: string): string {
  if (value.toLowerCase().includes("needs_check")) return "需核验";
  if (value.toLowerCase().includes("fresh")) return "较新";
  if (value.toLowerCase().includes("stale")) return "可能过时";
  return "已记录";
}

export function ResearchStatePanel({ state }: ResearchStatePanelProps) {
  if (!state) return null;
  const firstConclusion = state.preliminary_conclusions[0];

  return (
    <div className="ai-task-surface px-3 pt-3" data-testid="research-state">
      <Card className="border-border/60">
        <CardHeader className="p-3 pb-2">
          <CardTitle className="flex items-center gap-2 text-sm">
            <ShieldCheck className="h-4 w-4" />
            研究状态
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-2 p-3 pt-0 text-xs">
          <p className="font-medium">{state.research_question}</p>
          <div className="flex flex-wrap gap-1.5">
            <Badge variant="outline">{credibilityLabel(state)}</Badge>
            <Badge variant="outline">{freshnessLabel(state)}</Badge>
            {state.sources.slice(0, 2).map((source) => (
              <Badge key={source.evidence_id} variant="secondary">
                {source.citation_label || source.evidence_id}:{" "}
                {sourceFreshnessLabel(source.freshness)}
              </Badge>
            ))}
          </div>
          {state.conflicts
            .concat(state.counter_arguments)
            .slice(0, 3)
            .map((item) => (
              <p key={item} className="text-muted-foreground">
                {item}
              </p>
            ))}
          {firstConclusion ? (
            <div className="rounded-md border border-border/60 px-2 py-1.5">
              <p>{firstConclusion.statement}</p>
              <p className="mt-1 text-muted-foreground">
                {firstConclusion.boundary}
              </p>
            </div>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}
