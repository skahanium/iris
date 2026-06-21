import { useCallback, useMemo, useState } from "react";

import {
  buildArtifactTab,
  loadArtifactTabsSnapshot,
  saveArtifactTabsSnapshot,
} from "@/lib/assistant-artifact-tabs";
import type {
  ArtifactTab,
  AssistantArtifactDraft,
} from "@/types/assistant-artifact";

function browserStorage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function useArtifactTabs() {
  const storage = browserStorage();
  const [artifactTabs, setArtifactTabs] = useState<ArtifactTab[]>(() =>
    storage ? loadArtifactTabsSnapshot(storage) : [],
  );
  const [activeArtifactId, setActiveArtifactId] = useState<string | null>(null);

  const persist = useCallback(
    (tabs: ArtifactTab[]) => {
      if (storage) {
        saveArtifactTabsSnapshot(storage, tabs);
      }
    },
    [storage],
  );

  const openArtifact = useCallback(
    (draft: AssistantArtifactDraft) => {
      const tab = buildArtifactTab(draft);
      setArtifactTabs((prev) => {
        const next = [...prev.filter((item) => item.id !== tab.id), tab].slice(
          -10,
        );
        persist(next);
        return next;
      });
      setActiveArtifactId(tab.id);
    },
    [persist],
  );

  const activateArtifact = useCallback((id: string) => {
    setActiveArtifactId(id);
  }, []);

  const closeArtifact = useCallback(
    (id: string) => {
      setArtifactTabs((prev) => {
        const next = prev.filter((item) => item.id !== id);
        persist(next);
        return next;
      });
      setActiveArtifactId((current) => (current === id ? null : current));
    },
    [persist],
  );

  const activeArtifactTab = useMemo(
    () => artifactTabs.find((item) => item.id === activeArtifactId) ?? null,
    [activeArtifactId, artifactTabs],
  );

  return {
    activateArtifact,
    activeArtifactId,
    activeArtifactTab,
    artifactTabs,
    closeArtifact,
    openArtifact,
    setActiveArtifactId,
  };
}
