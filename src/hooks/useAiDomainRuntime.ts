import { useCallback, useEffect, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";

import type { AiDomain, AiDomainState } from "@/lib/ai-domain";
import { classifiedAiCacheClear, classifiedAiRetrievalClear } from "@/lib/ipc";

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

  // A classified document may never lend its draft or selected context to
  // another document. Changing the active classified path is therefore a
  // destructive in-memory boundary, not a conversation switch.
  useEffect(() => {
    const prevPath = prevClassifiedPathRef.current;
    const currPath = domainState.classifiedActivePath;

    if (
      domainState.domain === "classified" &&
      currPath !== null &&
      prevPath !== currPath
    ) {
      clearClassifiedVolatileState("classified_document_switch");
    }

    prevClassifiedPathRef.current = currPath;
  }, [
    domainState.domain,
    domainState.classifiedActivePath,
    clearClassifiedVolatileState,
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
    abortClassifiedRequest,
    clearClassifiedVolatileState,
  };
}
