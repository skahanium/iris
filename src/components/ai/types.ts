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
  /** Per-composer model override. The backend validates all hard capabilities. */
  modelOverride?: AgentModelOverride | null;
  onInsertToEditor?: (content: string) => void;
  /** Open the selected Web provider's diagnostics in the management center. */
  onOpenWebVerificationSettings?: () => void;
  /** Report Token / tool activity to the global StatusBar. */
  onChromeChange?: (snapshot: AssistantChromeSnapshot) => void;
  /** Agent 主区阅读（assistant_focus）有效状态；面板据此切换内容列与按钮文案。 */
  assistantFocus?: boolean;
  /** 请求进入 Agent 主区阅读（AppShell 布局策略决定最终 presentation）。 */
  onRequestFocusEnter?: () => void;
  /** 请求返回文档主平面。 */
  onRequestFocusExit?: () => void;
}
