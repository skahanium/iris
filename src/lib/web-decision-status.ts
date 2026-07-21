import {
  normalizeFreshness,
  type Freshness,
  type WebDecisionReason,
} from "@/types/ai";

export interface WebDecisionStatusInput {
  freshness?: string | null;
  webReason?: string | null;
  /** True once this Run completed a `web_search` / `web.search` tool call. */
  searched?: boolean;
}

const OFFLINE_REASON_LABEL: Partial<Record<WebDecisionReason, string>> = {
  user_disabled: "用户关闭",
  security_domain_offline: "安全域限制",
  explicit_local_only: "显式本地",
  trusted_runtime_fact: "本机事实",
  conversation_meta: "对话元问题",
  local_transformation: "本地变换",
  creative_generation: "创意生成",
};

/** Build a short, user-visible Web decision label for one Run. */
export function formatWebDecisionStatus(
  input: WebDecisionStatusInput,
): string | null {
  if (input.freshness == null && input.webReason == null && !input.searched) {
    return null;
  }
  const freshness: Freshness = normalizeFreshness(input.freshness);
  if (freshness === "offline") {
    const reason = (input.webReason ?? "") as WebDecisionReason;
    const reasonLabel = OFFLINE_REASON_LABEL[reason];
    return reasonLabel ? `离线（${reasonLabel}）` : "离线";
  }
  if (input.searched) {
    return "在线（已搜索）";
  }
  return "在线（搜索可用）";
}
