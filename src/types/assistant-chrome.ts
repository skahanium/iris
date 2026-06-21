import type { TokenUsage } from "@/types/ai";

/** AI 侧栏上报给全局底栏的快照（Token、工具状态、证据计数）。 */
export interface AssistantChromeSnapshot {
  sessionTokenUsage: TokenUsage | null;
  toolActivityLabel: string | null;
  evidenceCount: number;
  webPacketCount: number;
  /** 活跃 harness 请求 ID，仅用于暂停/错误恢复，不在普通界面展示。 */
  harnessRequestId: string | null;
}

export const EMPTY_ASSISTANT_CHROME: AssistantChromeSnapshot = {
  sessionTokenUsage: null,
  toolActivityLabel: null,
  evidenceCount: 0,
  webPacketCount: 0,
  harnessRequestId: null,
};
