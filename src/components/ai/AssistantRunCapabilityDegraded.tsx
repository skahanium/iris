import type { AssistantRunEventPayload } from "@/types/ai";

interface AssistantRunCapabilityDegradedProps {
  degradation: Extract<
    AssistantRunEventPayload,
    { kind: "capability_degraded" }
  >;
}

/** Nonterminal, conversation-local notice for a safely degraded capability. */
export function AssistantRunCapabilityDegraded({
  degradation,
}: AssistantRunCapabilityDegradedProps) {
  const retryHint = degradation.retryable
    ? "可稍后重试联网核实。"
    : "请检查联网配置或稍后重新发起核实。";

  return (
    <div
      aria-live="polite"
      className="border-b border-amber-500/20 bg-amber-500/5 px-3 py-2 text-xs text-muted-foreground"
      data-testid="assistant-run-capability-degraded"
      role="status"
    >
      <p className="font-medium text-foreground/80">联网能力已降级</p>
      <p>
        {degradation.message} {retryHint}
      </p>
    </div>
  );
}
