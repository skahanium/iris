import { useCallback } from "react";

import type { AssistantSelectionQuote } from "@/components/ai/UnifiedAssistantPanel";
import type { MediaTab } from "@/hooks/useMediaTabs";
import type { ArtifactTab } from "@/types/assistant-artifact";
import type { WritingEditorContext } from "@/types/ai";

interface UseWorkspaceAssistantRoutingOptions {
  activeArtifactTab: ArtifactTab | null;
  activeMediaTab: MediaTab | null;
  activeNoteIsClassified: boolean;
  assistantNotePathWithoutMedia: string | null;
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
  assistantNotePathWithoutMedia,
  getLiveMarkdown,
  getParagraphText,
  getWritingContext,
  handleInsertToEditor,
  selectionQuote,
  setAiStatus,
}: UseWorkspaceAssistantRoutingOptions) {
  const nonNoteSurfaceActive = Boolean(
    activeArtifactTab || activeMediaTab || activeNoteIsClassified,
  );

  const getAssistantLiveMarkdown = useCallback(
    () => (nonNoteSurfaceActive ? "" : getLiveMarkdown()),
    [getLiveMarkdown, nonNoteSurfaceActive],
  );
  const getAssistantWritingContext = useCallback(
    () => (nonNoteSurfaceActive ? null : getWritingContext()),
    [getWritingContext, nonNoteSurfaceActive],
  );
  const getAssistantParagraphText = useCallback(
    () => (nonNoteSurfaceActive ? null : getParagraphText()),
    [getParagraphText, nonNoteSurfaceActive],
  );
  const handleAssistantInsertToEditor = useCallback(
    (content: string) => {
      if (activeArtifactTab || activeMediaTab) {
        setAiStatus("请先切回笔记再插入内容");
        return;
      }
      if (activeNoteIsClassified) {
        setAiStatus("涉密笔记不能接收 AI 插入");
        return;
      }
      handleInsertToEditor(content);
    },
    [
      activeArtifactTab,
      activeMediaTab,
      activeNoteIsClassified,
      handleInsertToEditor,
      setAiStatus,
    ],
  );

  return {
    assistantNotePath: activeMediaTab ? null : assistantNotePathWithoutMedia,
    assistantSelectionQuote: nonNoteSurfaceActive ? null : selectionQuote,
    getAssistantLiveMarkdown,
    getAssistantParagraphText,
    getAssistantWritingContext,
    handleAssistantInsertToEditor,
  };
}
