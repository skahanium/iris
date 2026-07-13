import { useCallback, useMemo } from "react";

import type { TabItem } from "@/components/layout/TabBar";
import type { MediaTab } from "@/hooks/useMediaTabs";
import { deriveAiDomainState } from "@/lib/ai-domain";
import { isClassifiedVaultPath } from "@/lib/classified-path";
import type { FileListItem } from "@/types/ipc";

interface UseWorkspaceAssistantRoutingOptions {
  activeMediaTab: MediaTab | null;
  activeNoteIsClassified: boolean;
  activePath: string | null;
  classifiedUnlocked: boolean;
  handleInsertToEditor: (content: string) => void;
  setAiStatus: (status: string) => void;
  tabs: TabItem[];
}

/** Resolves only domain and explicit UI capabilities; it never carries editor bodies. */
export function useWorkspaceAssistantRouting({
  activeMediaTab,
  activeNoteIsClassified,
  activePath,
  classifiedUnlocked,
  handleInsertToEditor,
  setAiStatus,
  tabs,
}: UseWorkspaceAssistantRoutingOptions) {
  const domainState = useMemo(
    () =>
      deriveAiDomainState({
        activePath,
        activeNoteIsClassified,
        classifiedUnlocked,
        activeMediaTab,
      }),
    [activePath, activeNoteIsClassified, classifiedUnlocked, activeMediaTab],
  );

  const handleAssistantInsertToEditor = useCallback(
    (content: string) => {
      if (activeMediaTab) {
        setAiStatus("请先切回笔记再插入内容");
        return;
      }
      handleInsertToEditor(content);
    },
    [activeMediaTab, handleInsertToEditor, setAiStatus],
  );

  const assistantRuntimeDocumentCandidates = useMemo<FileListItem[]>(
    () =>
      domainState.domain === "normal"
        ? tabs
            .filter((tab) => !isClassifiedVaultPath(tab.path))
            .map((tab) => ({
              path: tab.path,
              title: tab.title,
              updatedAt: "",
              isLocked: Boolean(tab.locked),
            }))
        : [],
    [domainState.domain, tabs],
  );

  return {
    aiDomain: domainState.domain,
    assistantRuntimeDocumentCandidates,
    classifiedPath: domainState.classifiedActivePath,
    handleAssistantInsertToEditor,
  };
}
