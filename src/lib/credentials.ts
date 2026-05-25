/** 系统凭据服务名（与 Rust `credentials` / keyring 一致） */
export const BING_SEARCH_CREDENTIAL_SERVICE = "iris/bing-search";

export function llmCredentialService(provider: string): string {
  return `iris/llm/${provider}`;
}
