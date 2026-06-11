import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";

import { invokeErrorMessage } from "@/lib/credentials";
import {
  listenResearchProgress as defaultListenResearchProgress,
  researchAbort as defaultResearchAbort,
  researchGenerateNote as defaultResearchGenerateNote,
} from "@/lib/ipc";
import type { AssistantActionState, ResearchFocusPayload } from "@/types/ai";

import type { ChatLine } from "../AiMessageList";
import type { ResearchProgressData } from "../AssistantTaskSurfaces";
import { buildActionState } from "../unified-assistant-panel-utils";

interface UseResearchControlParams {
  researchResult: ResearchFocusPayload | null;
  setActionState: Dispatch<SetStateAction<AssistantActionState>>;
  setLastError: Dispatch<SetStateAction<string | null>>;
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
  deps?: {
    listenResearchProgress?: (
      handler: (payload: ResearchProgressData) => void,
    ) => Promise<() => void>;
    researchAbort?: (requestId: string) => Promise<unknown>;
    researchGenerateNote?: (params: {
      topic: string;
      summary: string;
      evidence_count: number;
      coverage_score: number;
    }) => Promise<{ suggested_path: string }>;
  };
}

interface UseResearchControlResult {
  researchProgress: ResearchProgressData | null;
  researchRunning: boolean;
  setResearchRunning: Dispatch<SetStateAction<boolean>>;
  researchPanelExpanded: boolean;
  setResearchPanelExpanded: Dispatch<SetStateAction<boolean>>;
  researchDetailRef: MutableRefObject<HTMLDivElement | null>;
  generatingResearchNote: boolean;
  researchRequestIdRef: MutableRefObject<string | null>;
  clearResearchProgress: () => void;
  abortResearch: () => Promise<void>;
  handleGenerateResearchNote: () => Promise<void>;
  handleExpandResearchDetail: (result: ResearchFocusPayload) => void;
}

export function useResearchControl({
  researchResult,
  setActionState,
  setLastError,
  setMessages,
  deps,
}: UseResearchControlParams): UseResearchControlResult {
  const [researchProgress, setResearchProgress] =
    useState<ResearchProgressData | null>(null);
  const [researchRunning, setResearchRunning] = useState(false);
  const [researchPanelExpanded, setResearchPanelExpanded] = useState(false);
  const researchDetailRef = useRef<HTMLDivElement | null>(null);
  const [generatingResearchNote, setGeneratingResearchNote] = useState(false);
  const researchRequestIdRef = useRef<string | null>(null);

  const listenResearchProgress =
    deps?.listenResearchProgress ?? defaultListenResearchProgress;
  const researchAbort = deps?.researchAbort ?? defaultResearchAbort;
  const researchGenerateNote =
    deps?.researchGenerateNote ?? defaultResearchGenerateNote;

  useEffect(() => {
    const setupResearchListener = async () => {
      return listenResearchProgress((payload) => {
        setResearchProgress(payload);
        if (payload.state === "running") {
          setResearchRunning(true);
        }
        if (
          payload.state === "completed" ||
          payload.state === "failed" ||
          payload.state === "aborted"
        ) {
          setResearchRunning(false);
          setActionState((prev) => ({
            ...prev,
            status:
              payload.state === "completed"
                ? "completed"
                : payload.state === "aborted"
                  ? "completed"
                  : "error",
          }));
        }
      });
    };

    let unlisten: (() => void) | undefined;
    void setupResearchListener().then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, [listenResearchProgress, setActionState]);

  const clearResearchProgress = useCallback(() => {
    setResearchProgress(null);
    setResearchRunning(false);
  }, []);

  const handleExpandResearchDetail = useCallback(
    (_result: ResearchFocusPayload) => {
      setResearchPanelExpanded(true);
      requestAnimationFrame(() => {
        researchDetailRef.current?.scrollIntoView({
          behavior: "smooth",
          block: "nearest",
        });
      });
    },
    [],
  );

  const abortResearch = useCallback(async () => {
    const id = researchRequestIdRef.current;
    if (!id) return;
    try {
      await researchAbort(id);
      setResearchRunning(false);
      setResearchProgress((prev) =>
        prev ? { ...prev, state: "aborted" } : null,
      );
      setActionState(buildActionState("research", "completed"));
    } catch (error) {
      setLastError(invokeErrorMessage(error));
    }
  }, [researchAbort, setActionState, setLastError]);

  const handleGenerateResearchNote = useCallback(async () => {
    if (!researchResult) return;
    setGeneratingResearchNote(true);
    try {
      const note = await researchGenerateNote({
        topic: researchResult.topic,
        summary: researchResult.summary,
        evidence_count: researchResult.evidence_matrix.total_evidence_count,
        coverage_score: researchResult.evidence_matrix.coverage_score,
      });
      setMessages((prev) => [
        ...prev,
        {
          role: "system",
          content: `研究笔记建议路径：${note.suggested_path}`,
        },
      ]);
    } catch (error) {
      setLastError(invokeErrorMessage(error));
    } finally {
      setGeneratingResearchNote(false);
    }
  }, [researchGenerateNote, researchResult, setLastError, setMessages]);

  return {
    researchProgress,
    researchRunning,
    setResearchRunning,
    researchPanelExpanded,
    setResearchPanelExpanded,
    researchDetailRef,
    generatingResearchNote,
    researchRequestIdRef,
    clearResearchProgress,
    abortResearch,
    handleGenerateResearchNote,
    handleExpandResearchDetail,
  };
}
