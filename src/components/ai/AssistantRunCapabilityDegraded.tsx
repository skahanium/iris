import type { AssistantRunEventPayload } from "@/types/ai";

interface AssistantRunCapabilityDegradedProps {
  degradation: Extract<
    AssistantRunEventPayload,
    { kind: "capability_degraded" }
  >;
}

interface AssistantRunWebVerificationFailedProps {
  failure: Extract<
    AssistantRunEventPayload,
    { kind: "web_verification_failed" }
  >;
  retrying: boolean;
  onRetry: () => void;
  onCheckConfiguration?: () => void;
}

/** Terminal WebRequired failure: no unverified answer was generated. */
export function AssistantRunWebVerificationFailed({
  failure,
  retrying,
  onRetry,
  onCheckConfiguration,
}: AssistantRunWebVerificationFailedProps) {
  return (
    <div
      aria-live="assertive"
      className="border-b border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-muted-foreground"
      data-testid="assistant-run-web-verification-failed"
      role="alert"
    >
      <p className="font-medium text-foreground">联网核实未完成</p>
      <p>未取得可用联网证据，因此没有生成未经核实的答复。</p>
      <p className="mt-1 font-mono text-[10px]">
        诊断 ID：{failure.diagnosticId}
      </p>
      {failure.retryable ? (
        <button
          type="button"
          className="mt-2 rounded border border-border px-2 py-1 text-foreground hover:bg-muted disabled:opacity-50"
          disabled={retrying}
          onClick={onRetry}
        >
          {retrying ? "正在重试…" : "重试联网核实"}
        </button>
      ) : null}
      {onCheckConfiguration ? (
        <button
          type="button"
          className="ml-2 mt-2 rounded border border-border px-2 py-1 text-foreground hover:bg-muted"
          onClick={onCheckConfiguration}
        >
          检查联网配置
        </button>
      ) : null}
    </div>
  );
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
