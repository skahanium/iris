import type { AssistantRunErrorCode } from "@/types/ai";

export type WebCapabilityDegradationDomain =
  | "mcp"
  | "harness"
  | "llm"
  | "unknown";

export interface WebCapabilityDegradationTriage {
  domain: WebCapabilityDegradationDomain;
  meaning: string;
  nextStep: string;
}

const DEFAULT_TRIAGE: WebCapabilityDegradationTriage = {
  domain: "unknown",
  meaning: "未映射的降级码；对照 Run 事件与 tracing 日志继续排查。",
  nextStep:
    "执行 scripts/diagnose-web-capability-degradation.mjs 并查看 docs/ops/web-capability-degradation.md。",
};

const TRIAGE_BY_CODE: Partial<
  Record<AssistantRunErrorCode, WebCapabilityDegradationTriage>
> = {
  agent_run_web_provider_auth_failed: {
    domain: "mcp",
    meaning: "MCP 搜索提供方鉴权失败（API Key 无效或缺失）。",
    nextStep:
      "管理中心 → 联网与证据 → 对应 MCP 提供方 → 实时诊断（credential / searchSmokeLive）。",
  },
  agent_run_web_provider_timeout: {
    domain: "mcp",
    meaning: "在 Run 预算内 MCP 搜索未在时限内返回。",
    nextStep:
      "检查网络与代理；对同一提供方执行实时诊断；检索日志「Run model-decided Web capability outcome」中的 web_duration_bucket。",
  },
  agent_run_web_provider_failed: {
    domain: "mcp",
    meaning: "MCP 传输或提供方瞬时/配额类失败。",
    nextStep:
      "查看 web_evidence_provider_health 与实时诊断；若为限流且 retryable=true 可稍后重试。",
  },
  agent_run_web_evidence_invalid: {
    domain: "mcp",
    meaning: "调用成功但无可用 HTTPS 证据行或摘录为空。",
    nextStep:
      "实时诊断中的 searchResultParseLive；确认 MCP 返回结构含可解析 HTTPS URL。",
  },
  agent_run_web_evidence_required: {
    domain: "harness",
    meaning: "工具循环级错误（通常不会伴随黄条 capability_degraded）。",
    nextStep: "查看 Run 终态 failed 与 safe_error_message，而非仅黄条。",
  },
  agent_run_mcp_unavailable: {
    domain: "mcp",
    meaning: "无可用 MCP 搜索映射或提供方不可用。",
    nextStep:
      "确认已选搜索提供方、映射完整且 enabled；对照 canEnable 与运行时诊断差异。",
  },
};

/**
 * Map a capability_degraded `code` to fault domain and operator next steps.
 */
export function triageWebCapabilityDegradation(
  code: AssistantRunErrorCode,
): WebCapabilityDegradationTriage {
  return TRIAGE_BY_CODE[code] ?? DEFAULT_TRIAGE;
}

export const WEB_CAPABILITY_DEGRADATION_DOMAIN_LABEL: Record<
  WebCapabilityDegradationDomain,
  string
> = {
  mcp: "MCP / 网络传输",
  harness: "Agent harness",
  llm: "LLM 模型",
  unknown: "待确认",
};
