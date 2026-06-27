import { useCallback, useMemo } from "react";

import type { AssistantSelectionQuote } from "@/components/ai/UnifiedAssistantPanel";
import type { MediaTab } from "@/hooks/useMediaTabs";
import { deriveAiDomainState } from "@/lib/ai-domain";
import type { ArtifactTab } from "@/types/assistant-artifact";
import type { WritingEditorContext } from "@/types/ai";

interface UseWorkspaceAssistantRoutingOptions {
  activeArtifactTab: ArtifactTab | null;
  activeMediaTab: MediaTab | null;
  activeNoteIsClassified: boolean;
  activePath: string | null;
  assistantNotePathWithoutMedia: string | null;
  classifiedUnlocked: boolean;
  getLiveMarkdown: () => string;
  getParagraphText: () => string | null;
  getWritingContext: () => WritingEditorContext | null;
  handleInsertToEditor: (content: string) => void;
  selectionQuote: AssistantSelectionQuote | null;
  setAiStatus: (status: string) => void;
}

export function useWorkspaceAssistantRouting({
  activeArtifactTab,
  activeMediaTab,
  activeNoteIsClassified,
  activePath,
  assistantNotePathWithoutMedia,
  classifiedUnlocked,
  getLiveMarkdown,
  getParagraphText,
  getWritingContext,
  handleInsertToEditor,
  selectionQuote,
  setAiStatus,
}: UseWorkspaceAssistantRoutingOptions) {
  const domainState = useMemo(
    () =>
      deriveAiDomainState({
        activePath,
        activeNoteIsClassified,
        classifiedUnlocked,
        activeArtifactTab,
        activeMediaTab,
      }),
    [
      activePath,
      activeNoteIsClassified,
      classifiedUnlocked,
      activeArtifactTab,
      activeMediaTab,
    ],
  );

  const nonNoteSurfaceActive = Boolean(
    activeArtifactTab || activeMediaTab || activeNoteIsClassified,
  );

  const isNormalDomain = domainState.domain === "normal";

  const getAssistantLiveMarkdown = useCallback(
    () => (isNormalDomain && !nonNoteSurfaceActive ? getLiveMarkdown() : ""),
    [getLiveMarkdown, isNormalDomain, nonNoteSurfaceActive],
  );
  const getAssistantWritingContext = useCallback(
    () =>
      isNormalDomain && !nonNoteSurfaceActive ? getWritingContext() : null,
    [getWritingContext, isNormalDomain, nonNoteSurfaceActive],
  );
  const getAssistantParagraphText = useCallback(
    () =>
      isNormalDomain && !nonNoteSurfaceActive ? getParagraphText() : null,
    [getParagraphText, isNormalDomain, nonNoteSurfaceActive],
  );
  const handleAssistantInsertToEditor = useCallback(
    (content: string) => {
      if (activeArtifactTab || activeMediaTab) {
        setAiStatus("请先切回笔记再插入内容");
        return;
      }
      if (domainState.domain === "classified") {
        return;
      }
      handleInsertToEditor(content);
    },
    [
      activeArtifactTab,
      activeMediaTab,
      domainState.domain,
      handleInsertToEditor,
      setAiStatus,
    ],
  );

  return {
    aiDomain: domainState.domain,
    assistantNotePath: activeMediaTab ? null : assistantNotePathWithoutMedia,
    assistantSelectionQuote: nonNoteSurfaceActive ? null : selectionQuote,
    classifiedPath: domainState.classifiedActivePath,
    getAssistantLiveMarkdown,
    getAssistantParagraphText,
    getAssistantWritingContext,
    handleAssistantInsertToEditor,
  };
}
