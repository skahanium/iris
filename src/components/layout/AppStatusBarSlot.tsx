import { StatusBar } from "@/components/layout/StatusBar";
import { useEffect, useState } from "react";
import { fileLinkSummary } from "@/lib/ipc";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import type { FileLinkSummary } from "@/types/ipc";
import type { WebSearchAvailability } from "@/lib/web-search-provider-state";
import type { ConnectivityStatus } from "@/types/llm";

interface AppStatusBarSlotProps {
  activePath: string | null;
  activeDocumentTitle: string | null;
  unsaved: boolean;
  characterCount: number;
  readingMinutes: number;
  aiStatus: string;
  assistantChrome: AssistantChromeSnapshot;
  editorZoom: number;
  onEditorZoomIn: () => void;
  onEditorZoomOut: () => void;
  onEditorZoomReset: () => void;
  onEditorZoomChange: (zoom: number) => void;
  onUndo: () => void;
  onRedo: () => void;
  canUndo: boolean;
  canRedo: boolean;
  webSearch: boolean;
  webSearchAvailability: WebSearchAvailability | null;
  onWebSearchChange: (enabled: boolean) => void;
  theme: "dark" | "light";
  onThemeChange: (theme: "dark" | "light") => void;
  connectivity: ConnectivityStatus | null;
  onOpenConnectivitySettings: () => void;
  onOpenManagementCenter: () => void;
  onOpenGraph: () => void;
  onOpenKnowledgeRelations: () => void;
}

export function AppStatusBarSlot({
  activePath,
  activeDocumentTitle,
  unsaved,
  characterCount,
  readingMinutes,
  aiStatus,
  assistantChrome,
  editorZoom,
  onEditorZoomIn,
  onEditorZoomOut,
  onEditorZoomReset,
  onEditorZoomChange,
  onUndo,
  onRedo,
  canUndo,
  canRedo,
  webSearch,
  webSearchAvailability,
  onWebSearchChange,
  theme,
  onThemeChange,
  connectivity,
  onOpenConnectivitySettings,
  onOpenManagementCenter,
  onOpenGraph,
  onOpenKnowledgeRelations,
}: AppStatusBarSlotProps) {
  const [linkSummary, setLinkSummary] = useState<FileLinkSummary | null>(null);
  const [linkSummaryUnavailable, setLinkSummaryUnavailable] = useState(false);

  useEffect(() => {
    if (!activePath) {
      setLinkSummary(null);
      setLinkSummaryUnavailable(false);
      return;
    }

    let cancelled = false;
    setLinkSummaryUnavailable(false);

    void fileLinkSummary(activePath)
      .then((summary) => {
        if (cancelled) return;
        setLinkSummary(summary);
      })
      .catch(() => {
        if (cancelled) return;
        setLinkSummary(null);
        setLinkSummaryUnavailable(true);
      });

    return () => {
      cancelled = true;
    };
  }, [activePath]);

  return (
    <StatusBar
      path={activePath}
      documentTitle={activeDocumentTitle}
      unsaved={unsaved}
      characterCount={characterCount}
      readingMinutes={readingMinutes}
      aiStatus={aiStatus}
      assistantChrome={assistantChrome}
      editorZoom={editorZoom}
      onEditorZoomIn={onEditorZoomIn}
      onEditorZoomOut={onEditorZoomOut}
      onEditorZoomReset={onEditorZoomReset}
      onEditorZoomChange={onEditorZoomChange}
      onUndo={onUndo}
      onRedo={onRedo}
      canUndo={canUndo}
      canRedo={canRedo}
      webSearch={webSearch}
      webSearchAvailability={webSearchAvailability}
      onWebSearchChange={onWebSearchChange}
      theme={theme}
      onThemeChange={onThemeChange}
      connectivity={connectivity}
      onOpenConnectivitySettings={onOpenConnectivitySettings}
      onOpenManagementCenter={onOpenManagementCenter}
      onOpenGraph={onOpenGraph}
      linkSummary={linkSummary}
      linkSummaryUnavailable={linkSummaryUnavailable}
      onOpenKnowledgeRelations={onOpenKnowledgeRelations}
    />
  );
}
