import { useCallback, useEffect, useState } from "react";

import { llmConfigGet } from "@/lib/ipc";
import { LLM_CONFIG_CHANGED_EVENT } from "@/lib/llm-events";

const DEFAULT_PROVIDER = "deepseek";

/**
 * 全应用共享的 LLM 默认供应商（内联 AI 与 `/` 命令）。
 */
export function useLlmProvider() {
  const [provider, setProvider] = useState(DEFAULT_PROVIDER);

  const refresh = useCallback(async () => {
    try {
      const config = await llmConfigGet();
      const defaultModel = config.routing.defaultModel;
      if (defaultModel?.providerId) {
        setProvider(defaultModel.providerId);
      }
    } catch {
      setProvider(DEFAULT_PROVIDER);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const onChange = () => void refresh();
    window.addEventListener(LLM_CONFIG_CHANGED_EVENT, onChange);
    return () => window.removeEventListener(LLM_CONFIG_CHANGED_EVENT, onChange);
  }, [refresh]);

  return { provider, setProvider };
}
