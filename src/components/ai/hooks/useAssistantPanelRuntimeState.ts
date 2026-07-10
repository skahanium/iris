import { useRef, useState } from "react";

import type {
  ContextPacket,
  ContextStatus,
  TaskPlanIntent,
  WebSearchUsage,
} from "@/types/ai";
import { buildActionState } from "../unified-assistant-panel-utils";
import type { AssistantProcessEvent } from "../AiMessageList";

export function useAssistantPanelRuntimeState() {
  const [actionState, setActionState] = useState(() =>
    buildActionState("chat", "idle"),
  );
  const [currentTaskPlanIntent, setCurrentTaskPlanIntent] =
    useState<TaskPlanIntent | null>(null);
  const [streaming, setStreaming] = useState(false);
  const [packets, setPackets] = useState<ContextPacket[]>([]);
  const [webSearchUsage, setWebSearchUsage] = useState<WebSearchUsage | null>(
    null,
  );
  const [selectedPacketIds, setSelectedPacketIds] = useState<string[]>([]);
  const [packetsOpen, setPacketsOpen] = useState(false);
  const [contextStatusData, setContextStatusData] =
    useState<ContextStatus | null>(null);
  const [activityHint, setActivityHint] = useState<string | null>(null);
  const [processEvents, setProcessEvents] = useState<AssistantProcessEvent[]>(
    [],
  );
  const streamBuf = useRef("");
  const requestIdRef = useRef<string | null>(null);
  const [harnessRequestId, setHarnessRequestId] = useState<string | null>(null);
  const [agentTaskId, setAgentTaskId] = useState<string | null>(null);
  const [pausedTaskId, setPausedTaskId] = useState<string | null>(null);
  const clearResearchProgressRef = useRef<(() => void) | null>(null);
  const panelSendActiveRef = useRef(false);
  const docStreamActiveRef = useRef(false);
  const forceNewSessionRef = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const messageListRef = useRef<HTMLDivElement>(null);

  return {
    actionState,
    activityHint,
    agentTaskId,
    clearResearchProgressRef,
    contextStatusData,
    currentTaskPlanIntent,
    docStreamActiveRef,
    forceNewSessionRef,
    harnessRequestId,
    messageListRef,
    packets,
    packetsOpen,
    panelSendActiveRef,
    pausedTaskId,
    processEvents,
    requestIdRef,
    selectedPacketIds,
    setActionState,
    setActivityHint,
    setAgentTaskId,
    setContextStatusData,
    setCurrentTaskPlanIntent,
    setHarnessRequestId,
    setPackets,
    setPacketsOpen,
    setPausedTaskId,
    setProcessEvents,
    setSelectedPacketIds,
    setStreaming,
    setWebSearchUsage,
    streamBuf,
    streaming,
    textareaRef,
    webSearchUsage,
  };
}
