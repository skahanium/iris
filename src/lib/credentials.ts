/** 系统凭据 ID，与 Rust credentials/keyring 保持一致。 */

export function llmCredentialService(provider: string): string {
  return `iris.llm.${provider}`;
}

const CREDENTIAL_ACCESS_MESSAGE =
  "无法访问系统凭据管理器，请解锁系统钥匙串，或在设置中重新保存对应供应商的 API Key。";

interface InvokeErrorPayload {
  code?: string;
  message?: string;
}

function asInvokeErrorPayload(err: unknown): InvokeErrorPayload | null {
  if (!err || typeof err !== "object" || Array.isArray(err)) return null;
  const record = err as Record<string, unknown>;
  return {
    code: typeof record.code === "string" ? record.code : undefined,
    message: typeof record.message === "string" ? record.message : undefined,
  };
}

function friendlyLlmError(raw: string, code?: string): string | null {
  const normalizedCode = code?.toLowerCase();
  if (normalizedCode === "credential" || normalizedCode === "keyring") {
    return CREDENTIAL_ACCESS_MESSAGE;
  }

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
    return CREDENTIAL_ACCESS_MESSAGE;
  }
  return null;
}

/** 从 Tauri invoke 错误中提取可读文案。 */
export function invokeErrorMessage(err: unknown): string {
  const payload = asInvokeErrorPayload(err);
  const code = payload?.code;
  let raw: string;

  if (typeof err === "string") raw = err;
  else if (err instanceof Error) raw = err.message;
  else if (payload?.message) raw = payload.message;
  else if (code) raw = code;
  else {
    try {
      raw = JSON.stringify(err);
    } catch {
      return "未知错误";
    }
  }

  const friendly = friendlyLlmError(raw, code);
  if (friendly) return friendly;
  if (raw.length > 280) {
    return `${raw.slice(0, 280)}…`;
  }
  return raw || "未知错误";
}
