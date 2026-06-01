import type { TokenUsage } from "@/types/ai";

/** AI 侧栏上报给全局底栏的快照（Token、工具状态、证据计数）。 */
export interface AssistantChromeSnapshot {
  sessionTokenUsage: TokenUsage | null;
  toolActivityLabel: string | null;
  evidenceCount: number;
  webPacketCount: number;
}

export const EMPTY_ASSISTANT_CHROME: AssistantChromeSnapshot = {
  sessionTokenUsage: null,
  toolActivityLabel: null,
  evidenceCount: 0,
  webPacketCount: 0,
};
