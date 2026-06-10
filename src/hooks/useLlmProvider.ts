import { useCallback, useEffect, useState } from "react";

import { llmConfigGet } from "@/lib/ipc";
import { LLM_CONFIG_CHANGED_EVENT } from "@/lib/llm-events";

const DEFAULT_PROVIDER = "deepseek";

/**
 * 全应用共享的 LLM 默认厂商（内联 AI、`/` 命令；场景路由见设置页）。
 */
export function useLlmProvider() {
  const [provider, setProvider] = useState(DEFAULT_PROVIDER);

  const refresh = useCallback(async () => {
    try {
      const config = await llmConfigGet();
      const firstScene = config.routing.scenes.knowledge_lookup;
      if (firstScene?.providerId) {
        setProvider(firstScene.providerId);
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
