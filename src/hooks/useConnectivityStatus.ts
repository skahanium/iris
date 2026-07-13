import { useCallback, useEffect, useState } from "react";

import { connectivityStatus } from "@/lib/ipc";
import { LLM_CONFIG_CHANGED_EVENT } from "@/lib/llm-events";
import type { ConnectivityStatus } from "@/types/llm";

/** Reads provider readiness independently of any assistant scenario or intent. */
export function useConnectivityStatus() {
  const [status, setStatus] = useState<ConnectivityStatus | null>(null);

  const refresh = useCallback(async () => {
    const next = await connectivityStatus();
    setStatus(next);
  }, []);

  useEffect(() => {
    void refresh();
    const onFocus = () => void refresh();
    const onConfig = () => void refresh();
    window.addEventListener("focus", onFocus);
    window.addEventListener(LLM_CONFIG_CHANGED_EVENT, onConfig);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener(LLM_CONFIG_CHANGED_EVENT, onConfig);
    };
  }, [refresh]);

  return { status, refresh };
}
