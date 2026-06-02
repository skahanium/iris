import { useCallback, useState } from "react";

import type { UnifiedArtifact } from "@/lib/map-harness-result-to-artifacts";

export function useAssistantArtifacts() {
  const [artifacts, setArtifacts] = useState<UnifiedArtifact[]>([]);

  const replaceArtifacts = useCallback((next: UnifiedArtifact[]) => {
    setArtifacts(next);
  }, []);

  const appendArtifacts = useCallback((next: UnifiedArtifact[]) => {
    setArtifacts((prev) => {
      const byId = new Map(prev.map((a) => [a.id, a]));
      for (const a of next) {
        byId.set(a.id, a);
      }
      return Array.from(byId.values());
    });
  }, []);

  const clearArtifacts = useCallback(() => {
    setArtifacts([]);
  }, []);

  return {
    artifacts,
    replaceArtifacts,
    appendArtifacts,
    clearArtifacts,
  };
}
