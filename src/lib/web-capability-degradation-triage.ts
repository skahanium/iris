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
  meaning: "未映射的降级码；对照 Run 事件继续排查。",
  nextStep:
    "在管理中心打开对应联网提供方的实时诊断，并对照本次回答的过程记录。",
};

const TRIAGE_BY_CODE: Partial<
  Record<AssistantRunErrorCode, WebCapabilityDegradationTriage>
> = {
  agent_run_web_provider_auth_failed: {
    domain: "mcp",
    meaning: "MCP 搜索提供方鉴权失败（API Key 无效或缺失）。",
    nextStep: "在管理中心重新配置对应联网提供方的凭据后重试。",
  },
  agent_run_web_provider_timeout: {
    domain: "mcp",
    meaning: "在 Run 预算内 MCP 搜索未在时限内返回。",
    nextStep: "检查网络后稍后重试；若持续超时，到管理中心查看该联网提供方。",
  },
  agent_run_web_provider_failed: {
    domain: "mcp",
    meaning: "MCP 传输或提供方瞬时/配额类失败。",
    nextStep: "稍后重试；若持续失败，到管理中心查看联网提供方状态。",
  },
  agent_run_web_evidence_invalid: {
    domain: "mcp",
    meaning: "调用成功但无可用 HTTPS 证据行或摘录为空。",
    nextStep: "换一个可公开访问的来源后重试，或到管理中心检查联网提供方。",
  },
  agent_run_web_evidence_required: {
    domain: "harness",
    meaning: "工具循环级错误（通常不会伴随黄条 capability_degraded）。",
    nextStep:
      "本轮需要可核验网页正文才能给出适用结论；可稍后重试或贴上官方原文。",
  },
  agent_run_mcp_unavailable: {
    domain: "mcp",
    meaning: "无可用 MCP 搜索映射或提供方不可用。",
    nextStep: "在管理中心启用并完成联网提供方配置后再试。",
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
  harness: "运行环境",
  llm: "LLM 模型",
  unknown: "待确认",
};
