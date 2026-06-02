import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { ExecutionPlan } from "@/types/ai";
import { Clock, Layers, Zap } from "lucide-react";

interface ExecutionPlanPreviewProps {
  plan: ExecutionPlan;
  onApprove: () => void;
  onModify: () => void;
  approveLabel?: string;
  modifyLabel?: string;
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
  approveLabel = "继续",
  modifyLabel = "调整证据",
}: ExecutionPlanPreviewProps) {
  return (
    <div
      className="space-y-4 rounded-lg border bg-muted/50 p-4"
      data-testid="execution-plan-preview"
    >
      <div className="flex items-center gap-2">
        <Layers className="h-4 w-4 text-primary" />
        <h4 className="text-sm font-medium">执行前预览 · 检索计划</h4>
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
        <Button
          type="button"
          size="sm"
          className="h-7 text-xs"
          onClick={onApprove}
        >
          {approveLabel}
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="h-7 text-xs"
          onClick={onModify}
        >
          {modifyLabel}
        </Button>
      </div>
    </div>
  );
}
