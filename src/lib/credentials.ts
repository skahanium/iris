/** 系统凭据 ID（与 Rust `credentials` / keyring 一致；勿含 `/` 以利 Windows） */
export const BING_SEARCH_CREDENTIAL_SERVICE = "iris.bing.search";

export function llmCredentialService(provider: string): string {
  return `iris.llm.${provider}`;
}

/** 从 Tauri invoke 错误中提取可读文案 */
export function invokeErrorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  if (err && typeof err === "object" && "message" in err) {
    const msg = (err as { message: unknown }).message;
    if (typeof msg === "string") return msg;
  }
  try {
    return JSON.stringify(err);
  } catch {
    return "未知错误";
  }
}
