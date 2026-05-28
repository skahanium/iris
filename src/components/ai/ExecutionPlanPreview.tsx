import { Badge } from "@/components/ui/badge";
import type { ExecutionPlan } from "@/types/ai";
import { Clock, Layers, Zap } from "lucide-react";

interface ExecutionPlanPreviewProps {
  plan: ExecutionPlan;
  onApprove: () => void;
  onModify: () => void;
}

const LAYER_LABELS: Record<string, string> = {
  fts: "全文搜索",
  vector: "语义搜索",
  graph: "图谱关联",
  exact: "精确匹配",
  template: "模板匹配",
};

export function ExecutionPlanPreview({
  plan,
  onApprove,
  onModify,
}: ExecutionPlanPreviewProps) {
  return (
    <div className="space-y-4 rounded-lg border bg-muted/50 p-4">
      <div className="flex items-center gap-2">
        <Layers className="h-4 w-4 text-primary" />
        <h4 className="text-sm font-medium">检索计划</h4>
      </div>

      <div className="space-y-2">
        {plan.steps.map((step, index) => (
          <div key={index} className="flex items-center gap-2 text-xs">
            <Badge variant="outline">{LAYER_LABELS[step.layer]}</Badge>
            <span className="truncate text-muted-foreground">{step.query}</span>
          </div>
        ))}
      </div>

      <div className="flex items-center gap-4 text-xs text-muted-foreground">
        <div className="flex items-center gap-1">
          <Zap className="h-3 w-3" />
          <span>~{plan.estimated_tokens} tokens</span>
        </div>
        <div className="flex items-center gap-1">
          <Clock className="h-3 w-3" />
          <span>~{plan.estimated_duration_ms}ms</span>
        </div>
      </div>

      <div className="flex items-center gap-2">
        <button
          onClick={onApprove}
          className="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90"
        >
          执行
        </button>
        <button
          onClick={onModify}
          className="rounded-md border px-3 py-1.5 text-xs hover:bg-muted"
        >
          修改
        </button>
      </div>
    </div>
  );
}
