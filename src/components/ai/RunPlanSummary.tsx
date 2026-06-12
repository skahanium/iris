import { Activity, ChevronRight } from "lucide-react";

import { Button } from "@/components/ui/button";
import type {
  AgentRunPlanSummary,
  IntentDetectionResult,
  PermissionPreflightSummary,
} from "@/types/ai";

interface RunPlanSummaryProps {
  runPlanSummary: AgentRunPlanSummary | null;
  intentDetection: IntentDetectionResult | null;
  permissionPreflightSummary: PermissionPreflightSummary | null;
  onOpen: () => void;
}

function confidenceLabel(value: number | null): string {
  if (value === null) return "unknown";
  return `${Math.round(value * 100)}%`;
}

export function RunPlanSummary({
  runPlanSummary,
  intentDetection,
  permissionPreflightSummary,
  onOpen,
}: RunPlanSummaryProps) {
  const intent =
    intentDetection?.detectedIntent ?? runPlanSummary?.detectedIntent ?? "chat";
  const confidence = intentDetection?.confidence ?? null;
  const permission =
    permissionPreflightSummary?.summary ??
    runPlanSummary?.permissionSummary ??
    "waiting for plan";
  const modelRoute = runPlanSummary?.modelRoute;
  const modelLabel = modelRoute
    ? `${modelRoute.slot} ${modelRoute.providerId}/${modelRoute.model}`
    : "model pending";
  const skillActivationPlan = runPlanSummary?.skillActivationPlan;
  const blockedCapabilities = runPlanSummary?.blockedCapabilities ?? [];
  const skillLabel = skillActivationPlan
    ? `skills ${skillActivationPlan.activatedSkills.length}/${blockedCapabilities.length}`
    : "skills 0/0";

  return (
    <div
      data-testid="run-plan-summary"
      className="border-b border-border/50 px-3 py-2"
    >
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="h-auto w-full justify-between gap-3 rounded-md px-2 py-1.5 text-left"
        onClick={onOpen}
      >
        <span className="flex min-w-0 items-center gap-2">
          <Activity className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <span className="min-w-0 truncate text-xs text-muted-foreground">
            {intent} / {confidenceLabel(confidence)} / {modelLabel} /{" "}
            {skillLabel} / {permission}
          </span>
        </span>
        <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
      </Button>
    </div>
  );
}
