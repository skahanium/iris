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
  AgentTaskStepDto,
} from "@/types/ipc";

interface UseAgentTaskStatusParams {
  taskId: string | null;
  setLastError: Dispatch<SetStateAction<string | null>>;
  setPausedTaskId: Dispatch<SetStateAction<string | null>>;
}

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
    void Promise.all([
      agentTaskGet(taskId),
      agentTaskSteps(taskId),
      agentTaskEvents(taskId),
    ])
      .then(([task, steps, events]) => {
        if (cancelled) return;
        setAgentTask(task);
        setAgentTaskStepsState(steps);
        setAgentTaskEventsState(events);
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        const message = error instanceof Error ? error.message : String(error);
        setLastError(message);
      });

    return () => {
      cancelled = true;
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
