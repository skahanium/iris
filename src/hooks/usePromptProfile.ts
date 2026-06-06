import { useCallback, useEffect, useState } from "react";

import { invokeErrorMessage } from "@/lib/credentials";
import { promptProfileGet, promptProfileSet } from "@/lib/ipc";
import type { PromptProfileDto } from "@/lib/ipc";
import {
  DEFAULT_PROMPT_PROFILE,
  dispatchPromptProfileChanged,
  mergeLegacyAssistantIdentity,
  normalizePromptProfile,
  PROMPT_PROFILE_CHANGED,
} from "@/lib/prompt-profile";

export function usePromptProfile() {
  const [profile, setProfileState] = useState<PromptProfileDto>(
    DEFAULT_PROMPT_PROFILE,
  );
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setError(null);
    try {
      const loaded = normalizePromptProfile(await promptProfileGet());
      const { profile: merged, migrated } =
        mergeLegacyAssistantIdentity(loaded);
      if (migrated) {
        await promptProfileSet(merged);
      }
      setProfileState(merged);
    } catch (e) {
      setError(invokeErrorMessage(e));
      setProfileState(DEFAULT_PROMPT_PROFILE);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const sync = () => {
      void refresh();
    };
    window.addEventListener(PROMPT_PROFILE_CHANGED, sync);
    return () => window.removeEventListener(PROMPT_PROFILE_CHANGED, sync);
  }, [refresh]);

  const saveProfile = useCallback(async (next: PromptProfileDto) => {
    setError(null);
    const normalized = normalizePromptProfile(next);
    await promptProfileSet(normalized);
    setProfileState(normalized);
    dispatchPromptProfileChanged();
  }, []);

  return { profile, loading, error, refresh, saveProfile, setProfileState };
}
