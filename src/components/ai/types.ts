import type { WritingEditorContext } from "@/types/ai";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";

export interface AssistantSelectionQuote {
  filePath: string;
  text: string;
  content?: string;
  editorRange?: { from: number; to: number } | null;
}

export interface UnifiedAssistantPanelProps {
  notePath: string | null;
  getNoteContent: () => string;
  webSearch?: boolean;
  getWritingContext: () => WritingEditorContext | null;
  getParagraphText: () => string | null;
  onPatchApplied?: (newContent: string) => void;
  onVaultRefresh?: () => void;
  onInsertToEditor?: (content: string) => void;
  onOpenArtifact?: (draft: AssistantArtifactDraft) => void;
  selectionQuote?: AssistantSelectionQuote | null;
  prefillMessage?: string | null;
  onChromeChange?: (snapshot: AssistantChromeSnapshot) => void;
}
