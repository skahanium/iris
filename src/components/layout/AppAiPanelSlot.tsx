import { lazy, Suspense, useMemo } from "react";

import type { AssistantSelectionQuote } from "@/components/ai/UnifiedAssistantPanel";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import type {
  AiDomain,
  ContextPacket,
  RuntimeDocumentSnapshot,
  WritingEditorContext,
} from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";
import type { FileListItem } from "@/types/ipc";
import type {
  DocumentOpenPriority,
  NoteOpenSource,
} from "@/lib/document-open-runtime";

const UnifiedAssistantPanel = lazy(() =>
  import("@/components/ai/UnifiedAssistantPanel").then((m) => ({
    default: m.UnifiedAssistantPanel,
  })),
);

function AssistantPanelLoading() {
  return (
    <div
      className="ai-sidecar flex min-h-0 flex-1 items-center justify-center px-4 text-xs text-muted-foreground"
      aria-live="polite"
      role="status"
    >
      AI 面板加载中…
    </div>
  );
}

interface AppAiPanelSlotProps {
  aiDomain: AiDomain;
  assistantNotePath: string | null;
  assistantPrefill: string | null;
  bumpVaultIndex: () => void;
  classifiedPath: string | null;
  getLiveMarkdown: () => string;
  runtimeDocumentCandidates?: FileListItem[];
  runtimeDocumentSnapshots?: RuntimeDocumentSnapshot[];
  getParagraphText: () => string | null;
  getWritingContext: () => WritingEditorContext | null;
  handleInsertToEditor: (content: string) => void;
  onOpenArtifact: (draft: AssistantArtifactDraft) => void;
  openNoteLeavingHome: (
    path: string,
    titleHint?: string,
    options?: { priority?: DocumentOpenPriority; source?: NoteOpenSource },
  ) => void | Promise<void>;
  onPrepareNotePath?: (
    path: string,
    titleHint?: string,
    source?: NoteOpenSource,
  ) => void;
  onSessionDeleted?: (sessionId: number | string) => void;
  onSessionsCleared?: () => void;
  onPatchApplied: (newContent: string) => void;
  selectionQuote: AssistantSelectionQuote | null;
  setAssistantChrome: (snapshot: AssistantChromeSnapshot) => void;
  webSearch: boolean;
  webSearchProviderName?: string | null;
}

export function AppAiPanelSlot({
  aiDomain,
  assistantNotePath,
  assistantPrefill,
  bumpVaultIndex,
  classifiedPath,
  getLiveMarkdown,
  runtimeDocumentCandidates = [],
  runtimeDocumentSnapshots = [],
  getParagraphText,
  getWritingContext,
  handleInsertToEditor,
  onOpenArtifact,
  openNoteLeavingHome,
  onPrepareNotePath,
  onSessionDeleted,
  onSessionsCleared,
  onPatchApplied,
  selectionQuote,
  setAssistantChrome,
  webSearch,
  webSearchProviderName = null,
}: AppAiPanelSlotProps) {
  const mentionRuntimeCandidates = useMemo(
    () =>
      runtimeDocumentCandidates.filter((candidate) =>
        candidate.path.trim().endsWith(".md"),
      ),
    [runtimeDocumentCandidates],
  );

  const openEvidenceSource = (packet: ContextPacket) => {
    if (packet.source_type === "web") {
      const url = packet.web?.url ?? packet.source_path;
      if (url) window.open(url, "_blank", "noopener,noreferrer");
      return;
    }
    if (packet.source_path) {
      onPrepareNotePath?.(packet.source_path, packet.title, "ai");
      openNoteLeavingHome(packet.source_path, packet.title, {
        priority: "foreground",
        source: "ai",
      });
    }
  };

  return (
    <ErrorBoundary scope="AI面板">
      <Suspense fallback={<AssistantPanelLoading />}>
        <UnifiedAssistantPanel
          aiDomain={aiDomain}
          classifiedPath={classifiedPath}
          notePath={assistantNotePath}
          getNoteContent={getLiveMarkdown}
          runtimeDocumentCandidates={mentionRuntimeCandidates}
          runtimeDocumentSnapshots={runtimeDocumentSnapshots}
          webSearch={webSearch}
          webSearchProviderName={webSearchProviderName}
          getWritingContext={getWritingContext}
          getParagraphText={getParagraphText}
          selectionQuote={selectionQuote}
          prefillMessage={assistantPrefill}
          onChromeChange={setAssistantChrome}
          onVaultRefresh={bumpVaultIndex}
          onInsertToEditor={handleInsertToEditor}
          onOpenArtifact={onOpenArtifact}
          onOpenEvidenceSource={openEvidenceSource}
          onSessionDeleted={onSessionDeleted}
          onSessionsCleared={onSessionsCleared}
          onPatchApplied={onPatchApplied}
        />
      </Suspense>
    </ErrorBoundary>
  );
}
