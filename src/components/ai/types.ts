import type {
  AgentModelOverride,
  AiDomain,
  ContextReference,
} from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import type { FileListItem } from "@/types/ipc";

/** Props accepted by the Run-only assistant presentation surface. */
export interface UnifiedAssistantPanelProps {
  aiDomain?: AiDomain;
  classifiedPath?: string | null;
  oneShotContextReference?: ContextReference | null;
  consumeOneShotContextReference?: () => void;
  runtimeDocumentCandidates?: FileListItem[];
  webSearch?: boolean;
  webSearchProviderName?: string | null;
  /** Per-composer model override. The backend validates all hard capabilities. */
  modelOverride?: AgentModelOverride | null;
  onInsertToEditor?: (content: string) => void;
  /** Open the selected Web provider's diagnostics in the management center. */
  onOpenWebVerificationSettings?: () => void;
  /** Report Token / tool activity to the global StatusBar. */
  onChromeChange?: (snapshot: AssistantChromeSnapshot) => void;
}
