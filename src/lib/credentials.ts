/** 系统凭据 ID（与 Rust `credentials` / keyring 一致；勿含 `/` 以利 Windows） */

/** MiniMax Token Plan（联网检索，与 Rust `MINIMAX_CREDENTIAL_SERVICE` 一致） */
export const MINIMAX_CREDENTIAL_SERVICE = "iris.minimax";

export function llmCredentialService(provider: string): string {
  return `iris.llm.${provider}`;
}

function friendlyLlmError(raw: string): string | null {
  const lower = raw.toLowerCase();
  if (
    lower.includes("service_unavailable") ||
    lower.includes("too busy") ||
    lower.includes("overloaded")
  ) {
    return "模型服务繁忙，请稍后重试或在设置中更换模型。";
  }
  if (lower.includes("rate limit") || lower.includes("429")) {
    return "请求过于频繁，请稍后再试。";
  }
  if (lower.includes("401") || lower.includes("invalid_api_key")) {
    return "API Key 无效或未配置，请在设置中检查。";
  }
  if (lower.includes("keyring error") || lower.includes("系统凭据管理器")) {
    return "无法访问系统凭据管理器，请解锁系统钥匙串，或在设置中重新保存对应供应商的 API Key。";
  }
  return null;
}

/** 从 Tauri invoke 错误中提取可读文案 */
export function invokeErrorMessage(err: unknown): string {
  let raw: string;
  if (typeof err === "string") raw = err;
  else if (err instanceof Error) raw = err.message;
  else if (err && typeof err === "object" && "message" in err) {
    const msg = (err as { message: unknown }).message;
    raw = typeof msg === "string" ? msg : "";
  } else {
    try {
      raw = JSON.stringify(err);
    } catch {
      return "未知错误";
    }
  }
  const friendly = friendlyLlmError(raw);
  if (friendly) return friendly;
  if (raw.length > 280) {
    return `${raw.slice(0, 280)}…`;
  }
  return raw || "未知错误";
}
