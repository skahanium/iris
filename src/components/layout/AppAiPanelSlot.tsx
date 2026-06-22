import { UnifiedAssistantPanel } from "@/components/ai/UnifiedAssistantPanel";
import type { AssistantSelectionQuote } from "@/components/ai/UnifiedAssistantPanel";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import type { ContextPacket, WritingEditorContext } from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";

interface AppAiPanelSlotProps {
  assistantNotePath: string | null;
  assistantPrefill: string | null;
  bumpVaultIndex: () => void;
  getLiveMarkdown: () => string;
  getParagraphText: () => string | null;
  getWritingContext: () => WritingEditorContext | null;
  handleInsertToEditor: (content: string) => void;
  onOpenArtifact: (draft: AssistantArtifactDraft) => void;
  openNoteLeavingHome: (path: string) => void;
  onSessionDeleted?: (sessionId: number) => void;
  onSessionsCleared?: () => void;
  onPatchApplied: (newContent: string) => void;
  selectionQuote: AssistantSelectionQuote | null;
  setAssistantChrome: (snapshot: AssistantChromeSnapshot) => void;
  webSearch: boolean;
}

export function AppAiPanelSlot({
  assistantNotePath,
  assistantPrefill,
  bumpVaultIndex,
  getLiveMarkdown,
  getParagraphText,
  getWritingContext,
  handleInsertToEditor,
  onOpenArtifact,
  openNoteLeavingHome,
  onSessionDeleted,
  onSessionsCleared,
  onPatchApplied,
  selectionQuote,
  setAssistantChrome,
  webSearch,
}: AppAiPanelSlotProps) {
  const openEvidenceSource = (packet: ContextPacket) => {
    if (packet.source_type === "web") {
      const url = packet.web?.url ?? packet.source_path;
      if (url) window.open(url, "_blank", "noopener,noreferrer");
      return;
    }
    if (packet.source_path) {
      openNoteLeavingHome(packet.source_path);
    }
  };

  return (
    <ErrorBoundary scope="AI面板">
      <UnifiedAssistantPanel
        notePath={assistantNotePath}
        getNoteContent={getLiveMarkdown}
        webSearch={webSearch}
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
    </ErrorBoundary>
  );
}
