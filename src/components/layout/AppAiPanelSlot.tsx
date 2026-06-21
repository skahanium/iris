import { UnifiedAssistantPanel } from "@/components/ai/UnifiedAssistantPanel";
import type { AssistantSelectionQuote } from "@/components/ai/UnifiedAssistantPanel";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import type { WritingEditorContext } from "@/types/ai";
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
  onPatchApplied,
  selectionQuote,
  setAssistantChrome,
  webSearch,
}: AppAiPanelSlotProps) {
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
        onPatchApplied={onPatchApplied}
      />
    </ErrorBoundary>
  );
}
