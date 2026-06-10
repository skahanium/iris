export const LLM_CONFIG_CHANGED_EVENT = "iris:llm-config-changed";

export function notifyLlmConfigChanged(): void {
  window.dispatchEvent(new CustomEvent(LLM_CONFIG_CHANGED_EVENT));
}
