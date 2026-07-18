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

function webFailureReasonMessage(
  reason: AssistantRunWebVerificationFailedProps["failure"]["failureReason"],
): string {
  switch (reason) {
    case "provider_output_too_large":
      return "提供方返回内容超过安全上限；请重试。若持续发生，请检查搜索结果数量限制。";
    case "provider_transport":
      return "MCP 调用在传输阶段未完成；请检查实时诊断与网络连接。";
    case "provider_timeout":
      return "MCP 调用在限定时间内未完成；可稍后重试。";
    case "provider_authentication":
      return "联网 API Key 无效或配置不正确，请重新输入原始 Key。";
    case "search_result_unparseable":
      return "MCP 返回的搜索结果格式无法安全解析。";
    case "search_result_no_usable_https":
      return "搜索结果中没有可安全使用的 HTTPS 证据。";
    case "evidence_content_empty":
      return "搜索结果缺少可注册的正文或摘要。";
    case "provider_rate_limited":
      return "搜索服务触发限流，可稍后重试。";
    case "provider_quota_exhausted":
      return "搜索服务额度已耗尽。";
    case "provider_invalid_arguments":
      return "搜索工具参数映射无效，请检查联网配置。";
    case "provider_unavailable":
      return "当前没有可用的联网证据提供方。";
    case "unknown":
      return "未取得可用联网证据；请检查联网配置后重试。";
  }
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
      <p className="mt-1">{webFailureReasonMessage(failure.failureReason)}</p>
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
