import { MessageSquarePlus } from "lucide-react";

import { AgentStatusBadge } from "@/components/ai/AgentStatusBadge";
import { AssistantPersonaDisplay } from "@/components/ai/AssistantPersonaDisplay";
import { Button } from "@/components/ui/button";
import type { PromptProfileDto } from "@/lib/ipc";
import type { AiScene } from "@/types/ai";

import type { ChatLine } from "./AiMessageList";
import { SessionHistoryDropdown } from "./SessionHistoryDropdown";

interface AssistantPanelHeaderProps {
  chromeActionsDisabled: boolean;
  currentSessionId: number | null;
  harnessRequestId: string | null;
  legacySceneHint: AiScene;
  notePath: string | null;
  onDeletedCurrentSession: () => void;
  onNewChat: () => void;
  onOpenAudit: () => void;
  onSelectSession: (id: number, messages: ChatLine[]) => void;
  profile: PromptProfileDto;
  webSearch: boolean;
}

export function AssistantPanelHeader({
  chromeActionsDisabled,
  currentSessionId,
  harnessRequestId,
  legacySceneHint,
  notePath,
  onDeletedCurrentSession,
  onNewChat,
  onOpenAudit,
  onSelectSession,
  profile,
  webSearch,
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
            scene={legacySceneHint}
            disabled={chromeActionsDisabled}
            auditAvailable={Boolean(harnessRequestId)}
            onOpenAudit={onOpenAudit}
          />
          <SessionHistoryDropdown
            scene={legacySceneHint}
            notePath={notePath}
            currentSessionId={currentSessionId}
            disabled={chromeActionsDisabled}
            onSelectSession={onSelectSession}
            onDeleted={(id) => {
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
