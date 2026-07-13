import type { AiDomain } from "@/types/ai";
import type { FileListItem } from "@/types/ipc";

/** Props accepted by the Run-only assistant presentation surface. */
export interface UnifiedAssistantPanelProps {
  aiDomain?: AiDomain;
  classifiedPath?: string | null;
  runtimeDocumentCandidates?: FileListItem[];
  webSearch?: boolean;
  webSearchProviderName?: string | null;
  onInsertToEditor?: (content: string) => void;
}
