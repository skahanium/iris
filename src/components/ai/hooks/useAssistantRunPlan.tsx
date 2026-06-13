import { useState } from "react";

import type {
  AgentRunPlanSummary,
  IntentDetectionResult,
  PermissionPreflightSummary,
} from "@/types/ai";

export function useAssistantRunPlan() {
  const [, setIntentDetection] = useState<IntentDetectionResult | null>(null);
  const [, setRunPlanSummary] = useState<AgentRunPlanSummary | null>(null);
  const [, setPermissionPreflightSummary] =
    useState<PermissionPreflightSummary | null>(null);

  return {
    layer: null,
    setIntentDetection,
    setPermissionPreflightSummary,
    setRunPlanSummary,
  };
}
