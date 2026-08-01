import type { AssistantPresentationState } from "@/lib/assistant-presentation";
import type { AssistantRunEventState } from "@/lib/assistant-run-events";
import type { RunState } from "@/types/ai";

const TERMINAL_STATES = new Set<RunState>(["completed", "failed", "cancelled"]);

const ACTIVE_OUTPUT_STATES = new Set<RunState>([
  "accepted",
  "preparing",
  "running",
  "verifying",
]);

export function isTerminalRunState(
  state: RunState | string | null | undefined,
): boolean {
  return typeof state === "string" && TERMINAL_STATES.has(state as RunState);
}

export function isActiveOutputRunState(
  state: RunState | string | null | undefined,
): boolean {
  return (
    typeof state === "string" && ACTIVE_OUTPUT_STATES.has(state as RunState)
  );
}

/**
 * Single source of truth for Stop/Send and “正在回答”.
 * Presentation answerComplete ends outputting even if durable `completed` is late.
 */
export function deriveRunOutputting(
  run: Pick<AssistantRunEventState, "runId" | "state"> | null | undefined,
  presentation:
    | Pick<AssistantPresentationState, "runId" | "answerComplete">
    | null
    | undefined,
): boolean {
  if (!run?.state || isTerminalRunState(run.state)) return false;
  if (
    presentation &&
    presentation.runId === run.runId &&
    presentation.answerComplete
  ) {
    return false;
  }
  return isActiveOutputRunState(run.state);
}
