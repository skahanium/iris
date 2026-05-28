import { invoke } from "@tauri-apps/api/core";

import type {
  ConnectivityStatus,
  LlmConfigGetResponse,
  LlmConfigTestResult,
  LlmRoutingConfig,
} from "@/types/llm";

export function llmConfigGet(): Promise<LlmConfigGetResponse> {
  return invoke<LlmConfigGetResponse>("llm_config_get");
}

export function llmConfigSet(routing: LlmRoutingConfig): Promise<void> {
  return invoke("llm_config_set", { routing });
}

export function llmConfigApplyDeepseekDefaults(): Promise<LlmRoutingConfig> {
  return invoke<LlmRoutingConfig>("llm_config_apply_deepseek_defaults");
}

export function connectivityStatus(
  scene?: string,
): Promise<ConnectivityStatus> {
  return invoke<ConnectivityStatus>("connectivity_status", { scene });
}

export function llmConfigTest(providerId: string): Promise<LlmConfigTestResult> {
  return invoke<LlmConfigTestResult>("llm_config_test", { providerId });
}

export interface MinimaxConfigGetResponse {
  minimaxConfigured: boolean;
  minimaxApiHost: string;
  webSearchBackend: string;
}

export interface MinimaxConfigSetRequest {
  minimaxApiHost?: string;
  webSearchBackend?: string;
}

export function minimaxConfigGet(): Promise<MinimaxConfigGetResponse> {
  return invoke<MinimaxConfigGetResponse>("minimax_config_get");
}

export function minimaxConfigSet(
  request: MinimaxConfigSetRequest,
): Promise<MinimaxConfigGetResponse> {
  return invoke<MinimaxConfigGetResponse>("minimax_config_set", { request });
}

export function minimaxConfigTest(): Promise<LlmConfigTestResult> {
  return invoke<LlmConfigTestResult>("minimax_config_test");
}

export const LLM_CONFIG_CHANGED_EVENT = "iris:llm-config-changed";

export function notifyLlmConfigChanged(): void {
  window.dispatchEvent(new CustomEvent(LLM_CONFIG_CHANGED_EVENT));
}
