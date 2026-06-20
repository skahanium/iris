import {
  useCallback,
  useEffect,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";

import {
  agentTaskAbort,
  agentTaskEvents,
  agentTaskGet,
  agentTaskSteps,
} from "@/lib/ipc";
import type {
  AgentTaskDto,
  AgentTaskEventDto,
  AgentTaskStatus,
  AgentTaskStepDto,
} from "@/types/ipc";

interface UseAgentTaskStatusParams {
  taskId: string | null;
  setLastError: Dispatch<SetStateAction<string | null>>;
  setPausedTaskId: Dispatch<SetStateAction<string | null>>;
}

const POLL_INTERVAL_MS = 2500;

const POLLABLE_TASK_STATUSES = new Set<AgentTaskStatus>([
  "queued",
  "running",
  "awaiting_confirmation",
  "paused_budget",
  "paused_recoverable",
]);

const TERMINAL_TASK_STATUSES = new Set<AgentTaskStatus>([
  "completed",
  "failed_safe",
  "aborted",
]);

export function useAgentTaskStatus({
  taskId,
  setLastError,
  setPausedTaskId,
}: UseAgentTaskStatusParams) {
  const [agentTask, setAgentTask] = useState<AgentTaskDto | null>(null);
  const [agentTaskStepsState, setAgentTaskStepsState] = useState<
    AgentTaskStepDto[]
  >([]);
  const [agentTaskEventsState, setAgentTaskEventsState] = useState<
    AgentTaskEventDto[]
  >([]);

  useEffect(() => {
    if (!taskId) {
      setAgentTask(null);
      setAgentTaskStepsState([]);
      setAgentTaskEventsState([]);
      return;
    }

    let cancelled = false;
    let intervalId: ReturnType<typeof window.setInterval> | null = null;

    const refresh = async () => {
      try {
        const [task, steps, events] = await Promise.all([
          agentTaskGet(taskId),
          agentTaskSteps(taskId),
          agentTaskEvents(taskId),
        ]);
        if (cancelled) return;
        if (!task) {
          setAgentTask(null);
          setAgentTaskStepsState([]);
          setAgentTaskEventsState([]);
          return;
        }
        setAgentTask(task);
        setAgentTaskStepsState(steps);
        setAgentTaskEventsState(events);

        if (TERMINAL_TASK_STATUSES.has(task.status) && intervalId) {
          window.clearInterval(intervalId);
          intervalId = null;
        }
        if (POLLABLE_TASK_STATUSES.has(task.status) && !intervalId) {
          intervalId = window.setInterval(() => {
            void refresh();
          }, POLL_INTERVAL_MS);
        }
      } catch (error: unknown) {
        if (cancelled) return;
        const message = error instanceof Error ? error.message : String(error);
        setLastError(message);
      }
    };

    void refresh();

    return () => {
      cancelled = true;
      if (intervalId) window.clearInterval(intervalId);
    };
  }, [taskId, setLastError]);

  const abortAgentTask = useCallback(async () => {
    const currentTaskId = taskId ?? agentTask?.task_id;
    if (!currentTaskId) return;
    try {
      await agentTaskAbort(currentTaskId);
      setPausedTaskId(null);
      setAgentTask((prev) =>
        prev && prev.task_id === currentTaskId
          ? {
              ...prev,
              status: "aborted",
              completed_at: new Date().toISOString(),
            }
          : prev,
      );
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setLastError(message);
    }
  }, [agentTask?.task_id, taskId, setLastError, setPausedTaskId]);

  return {
    abortAgentTask,
    agentTask,
    agentTaskEvents: agentTaskEventsState,
    agentTaskSteps: agentTaskStepsState,
  };
}
