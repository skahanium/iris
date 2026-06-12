import { useState } from "react";

import { RunPlanDrawer } from "@/components/ai/RunPlanDrawer";
import { RunPlanSummary } from "@/components/ai/RunPlanSummary";
import type {
  AgentRunPlanSummary,
  IntentDetectionResult,
  PermissionPreflightSummary,
} from "@/types/ai";

export function useAssistantRunPlan() {
  const [open, setOpen] = useState(false);
  const [intentDetection, setIntentDetection] =
    useState<IntentDetectionResult | null>(null);
  const [runPlanSummary, setRunPlanSummary] =
    useState<AgentRunPlanSummary | null>(null);
  const [permissionPreflightSummary, setPermissionPreflightSummary] =
    useState<PermissionPreflightSummary | null>(null);

  return {
    layer: (
      <>
        <RunPlanSummary
          runPlanSummary={runPlanSummary}
          intentDetection={intentDetection}
          permissionPreflightSummary={permissionPreflightSummary}
          onOpen={() => setOpen(true)}
        />
        <RunPlanDrawer
          open={open}
          onOpenChange={setOpen}
          runPlanSummary={runPlanSummary}
          intentDetection={intentDetection}
          permissionPreflightSummary={permissionPreflightSummary}
        />
      </>
    ),
    setIntentDetection,
    setPermissionPreflightSummary,
    setRunPlanSummary,
  };
}
