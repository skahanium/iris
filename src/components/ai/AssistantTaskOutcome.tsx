import type { TaskOutcome } from "@/types/ai";

interface AssistantTaskOutcomeProps {
  outcome: TaskOutcome | null | undefined;
}

/** Terminal K15 task-result strip. Not a run-lifecycle error and not a capability downgrade. */
export function AssistantTaskOutcome({ outcome }: AssistantTaskOutcomeProps) {
  if (outcome !== "partial" && outcome !== "blocked") {
    return null;
  }
  return (
    <section
      className="border-b border-warning/30 bg-warning-bg px-3 py-2"
      data-testid="assistant-task-outcome"
      data-outcome={outcome}
      aria-live="polite"
    >
      <p className="text-xs font-medium">
        {outcome === "partial" ? "部分完成" : "未能完成"}
      </p>
    </section>
  );
}
