import type {
  AiDomain,
  ContextPacket,
  RuntimeDocumentSnapshot,
  WritingEditorContext,
} from "@/types/ai";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import type { FileListItem } from "@/types/ipc";

export interface AssistantSelectionQuote {
  filePath: string;
  text: string;
  content?: string;
  editorRange?: { from: number; to: number } | null;
}

export interface UnifiedAssistantPanelProps {
  aiDomain?: AiDomain;
  classifiedPath?: string | null;
  notePath: string | null;
  getNoteContent: () => string;
  runtimeDocumentCandidates?: FileListItem[];
  runtimeDocumentSnapshots?: RuntimeDocumentSnapshot[];
  webSearch?: boolean;
  webSearchProviderName?: string | null;
  getWritingContext: () => WritingEditorContext | null;
  getParagraphText: () => string | null;
  onPatchApplied?: (newContent: string) => void;
  onVaultRefresh?: () => void;
  onInsertToEditor?: (content: string) => void;
  onOpenArtifact?: (draft: AssistantArtifactDraft) => void;
  onOpenEvidenceSource?: (packet: ContextPacket) => void;
  onSessionDeleted?: (sessionId: number | string) => void;
  onSessionsCleared?: () => void;
  selectionQuote?: AssistantSelectionQuote | null;
  prefillMessage?: string | null;
  onChromeChange?: (snapshot: AssistantChromeSnapshot) => void;
}
