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

  const nonNoteSurfaceActive = Boolean(activeArtifactTab || activeMediaTab);

  const isNormalDomain = domainState.domain === "normal";
  const hasExplicitNoteContext = Boolean(selectionQuote);

  const getAssistantLiveMarkdown = useCallback(
    () =>
      isNormalDomain && !nonNoteSurfaceActive && hasExplicitNoteContext
        ? getLiveMarkdown()
        : "",
    [
      getLiveMarkdown,
      hasExplicitNoteContext,
      isNormalDomain,
      nonNoteSurfaceActive,
    ],
  );
  const getAssistantWritingContext = useCallback(
    () =>
      isNormalDomain && !nonNoteSurfaceActive && hasExplicitNoteContext
        ? getWritingContext()
        : null,
    [
      getWritingContext,
      hasExplicitNoteContext,
      isNormalDomain,
      nonNoteSurfaceActive,
    ],
  );
  const getAssistantParagraphText = useCallback(
    () =>
      isNormalDomain && !nonNoteSurfaceActive && hasExplicitNoteContext
        ? getParagraphText()
        : null,
    [
      getParagraphText,
      hasExplicitNoteContext,
      isNormalDomain,
      nonNoteSurfaceActive,
    ],
  );
  const handleAssistantInsertToEditor = useCallback(
    (content: string) => {
      if (activeArtifactTab || activeMediaTab) {
        setAiStatus("请先切回笔记再插入内容");
        return;
      }
      handleInsertToEditor(content);
    },
    [activeArtifactTab, activeMediaTab, handleInsertToEditor, setAiStatus],
  );

  return {
    aiDomain: domainState.domain,
    assistantNotePath:
      activeMediaTab || activeArtifactTab
        ? null
        : domainState.domain === "classified"
          ? domainState.classifiedActivePath
          : hasExplicitNoteContext
            ? assistantNotePathWithoutMedia
            : null,
    assistantSelectionQuote: nonNoteSurfaceActive ? null : selectionQuote,
    classifiedPath: domainState.classifiedActivePath,
    getAssistantLiveMarkdown,
    getAssistantParagraphText,
    getAssistantWritingContext,
    handleAssistantInsertToEditor,
  };
}
