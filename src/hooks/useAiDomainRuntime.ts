import { useCallback, useEffect, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";

import type { AiDomain, AiDomainState } from "@/lib/ai-domain";
import { classifiedAiCacheClear, classifiedAiRetrievalClear } from "@/lib/ipc";

interface ClassifiedThreadSnapshot {
  path: string;
  draft: string;
  selectedMessageIds: Set<number>;
}

export interface UseAiDomainRuntimeOptions {
  domainState: AiDomainState;
}

export interface UseAiDomainRuntimeReturn {
  activeDraft: string;
  setActiveDraft: Dispatch<SetStateAction<string>>;
  normalDraft: string;
  setNormalDraft: (value: string) => void;
  classifiedDraft: string;
  setClassifiedDraft: (value: string) => void;
  normalSelectedMessageIds: Set<number>;
  classifiedSelectedMessageIds: Set<number>;
  toggleNormalMessageSelection: (id: number) => void;
  toggleClassifiedMessageSelection: (id: number) => void;
  clearClassifiedSelection: () => void;
  classifiedThreadByPath: Map<string, ClassifiedThreadSnapshot>;
  abortClassifiedRequest: () => void;
  clearClassifiedVolatileState: (reason: string) => void;
}

export function useAiDomainRuntime({
  domainState,
}: UseAiDomainRuntimeOptions): UseAiDomainRuntimeReturn {
  const [normalDraft, setNormalDraft] = useState("");
  const [classifiedDraft, setClassifiedDraft] = useState("");
  const [normalSelectedMessageIds, setNormalSelectedMessageIds] = useState(
    () => new Set<number>(),
  );
  const [classifiedSelectedMessageIds, setClassifiedSelectedMessageIds] =
    useState(() => new Set<number>());
  const [classifiedThreadByPath, setClassifiedThreadByPath] = useState(
    () => new Map<string, ClassifiedThreadSnapshot>(),
  );

  const abortControllerRef = useRef<AbortController | null>(null);
  const prevDomainRef = useRef<AiDomain>(domainState.domain);
  const prevClassifiedPathRef = useRef<string | null>(
    domainState.classifiedActivePath,
  );

  const classifiedStreamBufRef = useRef("");
  const classifiedPendingPatchesRef = useRef<unknown[]>([]);
  const classifiedWritingArtifactsRef = useRef<unknown[]>([]);

  const abortClassifiedRequest = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
  }, []);

  const clearClassifiedVolatileState = useCallback(
    (reason: string) => {
      console.debug("[classified-ai] volatile state cleared:", reason);
      abortClassifiedRequest();
      setClassifiedDraft("");
      setClassifiedSelectedMessageIds(new Set());
      setClassifiedThreadByPath(new Map());
      classifiedStreamBufRef.current = "";
      classifiedPendingPatchesRef.current = [];
      classifiedWritingArtifactsRef.current = [];
      void classifiedAiCacheClear();
      void classifiedAiRetrievalClear();
    },
    [abortClassifiedRequest],
  );

  const toggleNormalMessageSelection = useCallback((id: number) => {
    setNormalSelectedMessageIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const toggleClassifiedMessageSelection = useCallback((id: number) => {
    setClassifiedSelectedMessageIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const clearClassifiedSelection = useCallback(() => {
    setClassifiedSelectedMessageIds(new Set());
  }, []);

  const activeDraft =
    domainState.domain === "classified" ? classifiedDraft : normalDraft;
  const setActiveDraft = useCallback(
    (next: SetStateAction<string>) => {
      const setDraft =
        domainState.domain === "classified"
          ? setClassifiedDraft
          : setNormalDraft;
      setDraft(next);
    },
    [domainState.domain],
  );

  // Handle domain switch: classified → normal
  useEffect(() => {
    const prevDomain = prevDomainRef.current;
    const currDomain = domainState.domain;

    if (prevDomain === "classified" && currDomain === "normal") {
      clearClassifiedVolatileState("domain_switch_classified_to_normal");
    }

    prevDomainRef.current = currDomain;
  }, [domainState.domain, clearClassifiedVolatileState]);

  // Handle classified path switch: save current, load target
  useEffect(() => {
    const prevPath = prevClassifiedPathRef.current;
    const currPath = domainState.classifiedActivePath;

    if (
      domainState.domain === "classified" &&
      prevPath !== null &&
      prevPath !== currPath
    ) {
      setClassifiedThreadByPath((prev) => {
        const next = new Map(prev);
        next.set(prevPath, {
          path: prevPath,
          draft: classifiedDraft,
          selectedMessageIds: new Set(classifiedSelectedMessageIds),
        });
        return next;
      });
    }

    if (
      domainState.domain === "classified" &&
      currPath !== null &&
      prevPath !== currPath
    ) {
      const existing = classifiedThreadByPath.get(currPath);
      if (existing) {
        setClassifiedDraft(existing.draft);
        setClassifiedSelectedMessageIds(new Set(existing.selectedMessageIds));
      } else {
        setClassifiedDraft("");
        setClassifiedSelectedMessageIds(new Set());
      }
    }

    prevClassifiedPathRef.current = currPath;
  }, [
    domainState.domain,
    domainState.classifiedActivePath,
    classifiedDraft,
    classifiedSelectedMessageIds,
    classifiedThreadByPath,
  ]);

  return {
    activeDraft,
    setActiveDraft,
    normalDraft,
    setNormalDraft,
    classifiedDraft,
    setClassifiedDraft,
    normalSelectedMessageIds,
    classifiedSelectedMessageIds,
    toggleNormalMessageSelection,
    toggleClassifiedMessageSelection,
    clearClassifiedSelection,
    classifiedThreadByPath,
    abortClassifiedRequest,
    clearClassifiedVolatileState,
  };
}
