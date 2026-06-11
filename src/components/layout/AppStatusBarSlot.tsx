import { StatusBar } from "@/components/layout/StatusBar";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import type { ConnectivityStatus } from "@/types/llm";

interface AppStatusBarSlotProps {
  activePath: string | null;
  activeDocumentTitle: string | null;
  unsaved: boolean;
  characterCount: number;
  readingMinutes: number;
  aiStatus: string;
  assistantChrome: AssistantChromeSnapshot;
  keyboardLeaderPending: boolean;
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
  onWebSearchChange: (enabled: boolean) => void;
  theme: "dark" | "light";
  onThemeChange: (theme: "dark" | "light") => void;
  connectivity: ConnectivityStatus | null;
  onOpenConnectivitySettings: () => void;
}

export function AppStatusBarSlot({
  activePath,
  activeDocumentTitle,
  unsaved,
  characterCount,
  readingMinutes,
  aiStatus,
  assistantChrome,
  keyboardLeaderPending,
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
  onWebSearchChange,
  theme,
  onThemeChange,
  connectivity,
  onOpenConnectivitySettings,
}: AppStatusBarSlotProps) {
  return (
    <StatusBar
      path={activePath}
      documentTitle={activeDocumentTitle}
      unsaved={unsaved}
      characterCount={characterCount}
      readingMinutes={readingMinutes}
      aiStatus={aiStatus}
      assistantChrome={assistantChrome}
      keyboardLeaderPending={keyboardLeaderPending}
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
      onWebSearchChange={onWebSearchChange}
      theme={theme}
      onThemeChange={onThemeChange}
      connectivity={connectivity}
      onOpenConnectivitySettings={onOpenConnectivitySettings}
    />
  );
}
