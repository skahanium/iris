import { useCallback, useEffect, useState } from "react";

import { connectivityStatus, LLM_CONFIG_CHANGED_EVENT } from "@/lib/llm-ipc";
import type { AiScene } from "@/types/ai";
import type { ConnectivityStatus } from "@/types/llm";

const ACTIVE_SCENE_KEY = "iris_active_ai_scene";

export function setActiveAiScene(scene: AiScene): void {
  try {
    sessionStorage.setItem(ACTIVE_SCENE_KEY, scene);
    window.dispatchEvent(
      new CustomEvent(LLM_CONFIG_CHANGED_EVENT, { detail: { scene } }),
    );
  } catch {
    /* ignore */
  }
}

/** Read the harness scene last synced from assistant intent. */
export function getActiveAiScene(): AiScene {
  try {
    const stored = sessionStorage.getItem(ACTIVE_SCENE_KEY);
    if (
      stored === "knowledge_lookup" ||
      stored === "exemplar_learning" ||
      stored === "drafting_assist" ||
      stored === "research_synthesis"
    ) {
      return stored;
    }
  } catch {
    /* ignore */
  }
  return "knowledge_lookup";
}

export function useConnectivityStatus() {
  const [status, setStatus] = useState<ConnectivityStatus | null>(null);

  const refresh = useCallback(async () => {
    let scene: string | undefined;
    try {
      scene = sessionStorage.getItem(ACTIVE_SCENE_KEY) ?? undefined;
    } catch {
      scene = undefined;
    }
    const next = await connectivityStatus(scene);
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
