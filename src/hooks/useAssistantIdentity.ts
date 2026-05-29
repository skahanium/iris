import { useCallback, useEffect, useState } from "react";

import {
  ASSISTANT_IDENTITY_CHANGED,
  loadAssistantIdentity,
  saveAssistantIdentity,
  type AssistantIdentity,
} from "@/lib/assistant-identity";

export function useAssistantIdentity() {
  const [identity, setIdentityState] = useState(loadAssistantIdentity);

  useEffect(() => {
    const sync = () => setIdentityState(loadAssistantIdentity());
    window.addEventListener(ASSISTANT_IDENTITY_CHANGED, sync);
    return () => window.removeEventListener(ASSISTANT_IDENTITY_CHANGED, sync);
  }, []);

  const setIdentity = useCallback((next: AssistantIdentity) => {
    saveAssistantIdentity(next);
    setIdentityState(loadAssistantIdentity());
    window.dispatchEvent(new CustomEvent(ASSISTANT_IDENTITY_CHANGED));
  }, []);

  return { identity, setIdentity };
}
