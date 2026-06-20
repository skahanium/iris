import { useMemo, useState } from "react";

import type {
  AgentRunPlanSummary,
  IntentDetectionResult,
  PermissionPreflightSummary,
} from "@/types/ai";

export function useAssistantRunPlan() {
  const [intentDetection, setIntentDetection] =
    useState<IntentDetectionResult | null>(null);
  const [runPlanSummary, setRunPlanSummary] =
    useState<AgentRunPlanSummary | null>(null);
  const [permissionPreflightSummary, setPermissionPreflightSummary] =
    useState<PermissionPreflightSummary | null>(null);
  const counts = useMemo(() => {
    const blockedCount =
      (runPlanSummary?.blockedReasons.length ?? 0) +
      (permissionPreflightSummary?.blockedCapabilities.length ?? 0) +
      (permissionPreflightSummary?.missingUserGrants.length ?? 0);
    const confirmationCount =
      permissionPreflightSummary?.requiredConfirmations.length ?? 0;
    return { blockedCount, confirmationCount };
  }, [permissionPreflightSummary, runPlanSummary]);

  return {
    intentDetection,
    permissionPreflightSummary,
    runPlanSummary,
    blockedCount: counts.blockedCount,
    confirmationCount: counts.confirmationCount,
    setIntentDetection,
    setPermissionPreflightSummary,
    setRunPlanSummary,
  };
}
