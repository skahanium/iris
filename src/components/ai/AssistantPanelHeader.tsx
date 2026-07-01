import { MessageSquarePlus } from "lucide-react";

import { AgentStatusBadge } from "@/components/ai/AgentStatusBadge";
import { AssistantPersonaDisplay } from "@/components/ai/AssistantPersonaDisplay";
import { Button } from "@/components/ui/button";
import type { PromptProfileDto } from "@/lib/ipc";
import type {
  AiDomain,
  AiScene,
  AssistantTaskStatus,
  TaskPlanIntent,
  WebSearchUsage,
} from "@/types/ai";

import type { ChatLine } from "./AiMessageList";
import { SessionHistoryDropdown } from "./SessionHistoryDropdown";

interface AssistantPanelHeaderProps {
  chromeActionsDisabled: boolean;
  currentSessionId: number | string | null;
  domain?: AiDomain;
  scene: AiScene;
  onDeletedCurrentSession: () => void;
  onDeletedSession?: (sessionId: number | string) => void;
  onNewChat: () => void;
  onSelectSession: (
    id: number | string,
    messages: ChatLine[],
    ledgerPackets?: ChatLine["evidencePackets"],
  ) => void;
  profile: PromptProfileDto;
  taskPlanIntent?: TaskPlanIntent | null;
  taskStatus: AssistantTaskStatus;
  webSearch: boolean;
  webSearchUsage?: WebSearchUsage | null;
}

export function AssistantPanelHeader({
  chromeActionsDisabled,
  currentSessionId,
  domain = "normal",
  scene,
  onDeletedCurrentSession,
  onDeletedSession,
  onNewChat,
  onSelectSession,
  profile,
  taskPlanIntent,
  taskStatus,
  webSearch,
  webSearchUsage,
}: AssistantPanelHeaderProps) {
  return (
    <header className="ai-sidecar-header shrink-0 border-b border-border/60 px-3 py-1.5">
      <div className="flex items-center justify-between gap-3">
        <div className="flex min-w-0 flex-1 items-center">
          <AssistantPersonaDisplay profile={profile} />
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <AgentStatusBadge
            webSearchEnabled={webSearch}
            webSearchUsage={webSearchUsage}
            scene={scene}
            taskPlanIntent={taskPlanIntent}
            taskStatus={taskStatus}
            disabled={chromeActionsDisabled}
          />
          <SessionHistoryDropdown
            currentSessionId={currentSessionId}
            disabled={chromeActionsDisabled}
            domain={domain}
            onSelectSession={onSelectSession}
            onDeleted={(id) => {
              onDeletedSession?.(id);
              if (currentSessionId === id) {
                onDeletedCurrentSession();
              }
            }}
          />
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-8 gap-1 px-2 text-xs"
            title="新对话（不加载本笔记下的历史会话）"
            onClick={onNewChat}
            disabled={chromeActionsDisabled}
          >
            <MessageSquarePlus className="h-3.5 w-3.5" />
            新对话
          </Button>
        </div>
      </div>
    </header>
  );
}
