import { useEffect, useState } from "react";

import { llmProviders } from "@/lib/ipc";

const DEFAULT_PROVIDER = "openai";

/**
 * 全应用共享的 LLM 提供商选择（侧栏、内联 AI、`/` 命令一致）。
 */
export function useLlmProvider() {
  const [provider, setProvider] = useState(DEFAULT_PROVIDER);

  useEffect(() => {
    void llmProviders().then((list) => {
      if (list[0]?.id) setProvider(list[0].id);
    });
  }, []);

  return { provider, setProvider };
}
