import { useCallback, useMemo } from "react";

import type { AssistantSelectionQuote } from "@/components/ai/UnifiedAssistantPanel";
import type { TabItem } from "@/components/layout/TabBar";
import type { MediaTab } from "@/hooks/useMediaTabs";
import { deriveAiDomainState } from "@/lib/ai-domain";
import { isClassifiedVaultPath } from "@/lib/classified-path";
import type { ArtifactTab } from "@/types/assistant-artifact";
import type { RuntimeDocumentSnapshot, WritingEditorContext } from "@/types/ai";
import type { FileListItem } from "@/types/ipc";

interface UseWorkspaceAssistantRoutingOptions {
  activeArtifactTab: ArtifactTab | null;
  activeMediaTab: MediaTab | null;
  activeNoteIsClassified: boolean;
  activePath: string | null;
  classifiedUnlocked: boolean;
  getLiveMarkdown: () => string;
  getParagraphText: () => string | null;
  getTabMarkdownCached: (path: string) => string | undefined;
  getWritingContext: () => WritingEditorContext | null;
  handleInsertToEditor: (content: string) => void;
  selectionQuote: AssistantSelectionQuote | null;
  setAiStatus: (status: string) => void;
  tabs: TabItem[];
}

export function useWorkspaceAssistantRouting({
  activeArtifactTab,
  activeMediaTab,
  activeNoteIsClassified,
  activePath,
  classifiedUnlocked,
  getLiveMarkdown,
  getParagraphText,
  getTabMarkdownCached,
  getWritingContext,
  handleInsertToEditor,
  selectionQuote,
  setAiStatus,
  tabs,
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
  const assistantNotePathWithoutMedia = activePath;

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

  const assistantRuntimeDocumentCandidates = useMemo<FileListItem[]>(
    () =>
      isNormalDomain
        ? tabs
            .filter((tab) => !isClassifiedVaultPath(tab.path))
            .map((tab) => ({
              path: tab.path,
              title: tab.title,
              updatedAt: "",
              isLocked: Boolean(tab.locked),
            }))
        : [],
    [isNormalDomain, tabs],
  );
  const assistantRuntimeDocumentSnapshots = useMemo<RuntimeDocumentSnapshot[]>(
    () =>
      isNormalDomain
        ? tabs
            .filter((tab) => !isClassifiedVaultPath(tab.path))
            .map((tab) => ({
              path: tab.path,
              title: tab.title,
              content:
                tab.path === activePath
                  ? getLiveMarkdown()
                  : (getTabMarkdownCached(tab.path) ?? ""),
              isLocked: Boolean(tab.locked),
            }))
            .filter((doc) => doc.path.trim() && doc.content.trim())
        : [],
    [activePath, getLiveMarkdown, getTabMarkdownCached, isNormalDomain, tabs],
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
    assistantRuntimeDocumentCandidates,
    assistantRuntimeDocumentSnapshots,
    assistantSelectionQuote: nonNoteSurfaceActive ? null : selectionQuote,
    classifiedPath: domainState.classifiedActivePath,
    getAssistantLiveMarkdown,
    getAssistantParagraphText,
    getAssistantWritingContext,
    handleAssistantInsertToEditor,
  };
}
