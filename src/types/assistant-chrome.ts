import type { TokenUsage } from "@/types/ai";

/** AI 渚ф爮涓婃姤缁欏叏灞€搴曟爮鐨勫揩鐓э紙Token銆佸伐鍏风姸鎬併€佽瘉鎹鏁帮級銆?*/
export interface AssistantChromeSnapshot {
  sessionTokenUsage: TokenUsage | null;
  toolActivityLabel: string | null;
  evidenceCount: number;
  webEvidenceCount: number;
}

export const EMPTY_ASSISTANT_CHROME: AssistantChromeSnapshot = {
  sessionTokenUsage: null,
  toolActivityLabel: null,
  evidenceCount: 0,
  webEvidenceCount: 0,
};
