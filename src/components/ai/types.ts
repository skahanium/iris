import type { WritingEditorContext } from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";

export interface AssistantSelectionQuote {
  filePath: string;
  text: string;
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
  selectionQuote?: AssistantSelectionQuote | null;
  prefillMessage?: string | null;
  onChromeChange?: (snapshot: AssistantChromeSnapshot) => void;
}
