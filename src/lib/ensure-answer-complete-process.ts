import type { AssistantProcessItem } from "@/lib/assistant-process";
import {
  ANSWER_COMPLETE_PROCESS_ID,
  ANSWER_COMPLETE_PROCESS_LABEL,
} from "@/lib/assistant-presentation";

/**
 * Ensures a completed run's process timeline ends with the terminal answer label
 * when presentation-frozen items omit the durable answer_complete stage.
 */
export function ensureTerminalAnswerComplete(
  items: readonly AssistantProcessItem[] | undefined,
  runState: string | null | undefined,
): AssistantProcessItem[] {
  const list = items ? [...items] : [];
  if (runState !== "completed") {
    return list;
  }
  if (
    list.some(
      (item) =>
        item.id === ANSWER_COMPLETE_PROCESS_ID ||
        item.label === ANSWER_COMPLETE_PROCESS_LABEL,
    )
  ) {
    return list;
  }
  list.push({
    id: ANSWER_COMPLETE_PROCESS_ID,
    kind: "stage",
    label: ANSWER_COMPLETE_PROCESS_LABEL,
    status: "completed",
    createdAt: list.at(-1)?.createdAt ?? 0,
  });
  return list;
}
