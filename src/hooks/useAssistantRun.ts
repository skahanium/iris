import { useCallback, useMemo, useState } from "react";

import type { AssistantIntent, AssistantTaskStatus } from "@/types/ai";

/** Unified assistant run lifecycle (harness modernization). */
export type AssistantRunState =
  | "idle"
  | "assembling_context"
  | "awaiting_plan_approval"
  | "running"
  | "awaiting_tool_confirmation"
  | "streaming_final"
  | "completed"
  | "error"
  | "aborted";

export interface AssistantRunSnapshot {
  runState: AssistantRunState;
  intent: AssistantIntent;
  activityHint: string | null;
  harnessRequestId: string | null;
}

/** 是否阻塞模型/发送；工具确认等待不阻塞侧栏 chrome。 */
export function isAssistantRunBusy(runState: AssistantRunState): boolean {
  return [
    "assembling_context",
    "awaiting_plan_approval",
    "running",
    "streaming_final",
  ].includes(runState);
}

function taskStatusToRunState(
  status: AssistantTaskStatus,
  activityHint: string | null,
): AssistantRunState {
  if (status === "awaiting_confirmation") {
    return "awaiting_tool_confirmation";
  }
  if (status === "running") {
    if (activityHint?.includes("组装")) return "assembling_context";
    if (activityHint?.includes("计划")) return "awaiting_plan_approval";
    if (activityHint?.includes("流式") || activityHint?.includes("最终")) {
      return "streaming_final";
    }
    return "running";
  }
  if (status === "completed") return "completed";
  if (status === "error") return "error";
  return "idle";
}

export function useAssistantRun(initialIntent: AssistantIntent = "chat") {
  const [intent, setIntent] = useState<AssistantIntent>(initialIntent);
  const [taskStatus, setTaskStatus] = useState<AssistantTaskStatus>("idle");
  const [activityHint, setActivityHint] = useState<string | null>(null);
  const [harnessRequestId, setHarnessRequestId] = useState<string | null>(null);
  const [evidenceRefreshNotice, setEvidenceRefreshNotice] = useState<
    string | null
  >(null);

  const runState = useMemo(
    () => taskStatusToRunState(taskStatus, activityHint),
    [taskStatus, activityHint],
  );

  const snapshot: AssistantRunSnapshot = useMemo(
    () => ({ runState, intent, activityHint, harnessRequestId }),
    [runState, intent, activityHint, harnessRequestId],
  );

  /** 阻塞输入/发送；工具确认由弹窗单独处理，不应锁死侧栏 chrome（历史、新对话）。 */
  const isBusy = useMemo(() => isAssistantRunBusy(runState), [runState]);

  const setFromTaskStatus = useCallback(
    (next: AssistantTaskStatus, nextIntent?: AssistantIntent) => {
      if (nextIntent) setIntent(nextIntent);
      setTaskStatus(next);
    },
    [],
  );

  const reset = useCallback(() => {
    setTaskStatus("idle");
    setActivityHint(null);
    setHarnessRequestId(null);
    setEvidenceRefreshNotice(null);
  }, []);

  return {
    intent,
    setIntent,
    taskStatus,
    runState,
    snapshot,
    isBusy,
    activityHint,
    setActivityHint,
    harnessRequestId,
    setHarnessRequestId,
    evidenceRefreshNotice,
    setEvidenceRefreshNotice,
    setFromTaskStatus,
    reset,
  };
}
