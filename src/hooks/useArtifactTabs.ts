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

function evidenceArtifactSessionId(tab: ArtifactTab): number | string | null {
  if (tab.kind !== "session_evidence_detail") return null;
  const payload = tab.payload;
  if (typeof payload !== "object" || payload === null) return null;
  const sessionId = (payload as { sessionId?: unknown }).sessionId;
  if (typeof sessionId === "number" || typeof sessionId === "string") {
    return sessionId;
  }
  return null;
}

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

  const closeEvidenceArtifactsForSession = useCallback(
    (sessionId: number | string) => {
      setArtifactTabs((prev) => {
        const removedIds = new Set(
          prev
            .filter(
              (item) =>
                String(evidenceArtifactSessionId(item)) === String(sessionId),
            )
            .map((item) => item.id),
        );
        if (removedIds.size === 0) return prev;
        const next = prev.filter((item) => !removedIds.has(item.id));
        persist(next);
        setActiveArtifactId((current) =>
          current && removedIds.has(current) ? null : current,
        );
        return next;
      });
    },
    [persist],
  );

  const closeAllEvidenceArtifacts = useCallback(() => {
    setArtifactTabs((prev) => {
      const removedIds = new Set(
        prev
          .filter((item) => item.kind === "session_evidence_detail")
          .map((item) => item.id),
      );
      if (removedIds.size === 0) return prev;
      const next = prev.filter((item) => !removedIds.has(item.id));
      persist(next);
      setActiveArtifactId((current) =>
        current && removedIds.has(current) ? null : current,
      );
      return next;
    });
  }, [persist]);

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
    closeAllEvidenceArtifacts,
    closeEvidenceArtifactsForSession,
    openArtifact,
    setActiveArtifactId,
  };
}
