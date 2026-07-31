import { Maximize2, MessageSquarePlus, Minimize2 } from "lucide-react";

import { AgentStatusBadge } from "@/components/ai/AgentStatusBadge";
import { AssistantPersonaDisplay } from "@/components/ai/AssistantPersonaDisplay";
import { Button } from "@/components/ui/button";
import type { AssistantRunState } from "@/hooks/useAssistantRun";
import type { PromptProfileDto } from "@/lib/ipc";
import type { AiDomain, AssistantSessionRef } from "@/types/ai";

import type { ChatLine } from "./AiMessageList";
import { SessionHistoryDropdown } from "./SessionHistoryDropdown";

interface AssistantPanelHeaderProps {
  chromeActionsDisabled: boolean;
  currentSession: AssistantSessionRef | null;
  domain?: AiDomain;
  onDeletedCurrentSession: () => void;
  onDeletedSession?: (session: AssistantSessionRef) => void;
  onNewChat: () => void;
  onSelectSession: (
    session: AssistantSessionRef,
    messages: ChatLine[],
    activeRun: import("@/types/ai").AssistantRunGetResponse | null,
  ) => void;
  profile: PromptProfileDto;
  runState: AssistantRunState;
  webSearch: boolean;
  webSearchProviderName?: string | null;
  /** Agent 主区阅读有效状态：控制“展开阅读/返回文档”按钮文案与图标。 */
  assistantFocus?: boolean;
  /** 请求切换主区阅读（进入/返回由布局策略决定）。 */
  onRequestFocusToggle?: () => void;
}

/** Header actions use opaque conversation references and the unified Run state only. */
export function AssistantPanelHeader({
  chromeActionsDisabled,
  currentSession,
  domain = "normal",
  onDeletedCurrentSession,
  onDeletedSession,
  onNewChat,
  onSelectSession,
  profile,
  runState,
  webSearch,
  webSearchProviderName,
  assistantFocus = false,
  onRequestFocusToggle,
}: AssistantPanelHeaderProps) {
  return (
    <header className="ai-sidecar-header shrink-0 border-b border-border-subtle px-3 py-1">
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 flex-1 items-center">
          <AssistantPersonaDisplay profile={profile} />
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 w-7 px-0"
            title={assistantFocus ? "返回文档" : "展开阅读"}
            aria-label={assistantFocus ? "返回文档" : "展开阅读"}
            onClick={onRequestFocusToggle}
          >
            {assistantFocus ? (
              <Minimize2 className="h-3.5 w-3.5" />
            ) : (
              <Maximize2 className="h-3.5 w-3.5" />
            )}
          </Button>
          <AgentStatusBadge
            webSearchEnabled={webSearch}
            webSearchProviderName={webSearchProviderName}
            runState={runState}
            disabled={chromeActionsDisabled}
          />
          {domain === "normal" ? (
            <SessionHistoryDropdown
              currentSession={currentSession}
              disabled={chromeActionsDisabled}
              domain={domain}
              onSelectSession={onSelectSession}
              onDeleted={(session) => {
                onDeletedSession?.(session);
                if (
                  currentSession?.domain === session.domain &&
                  currentSession.sessionKey === session.sessionKey
                ) {
                  onDeletedCurrentSession();
                }
              }}
            />
          ) : null}
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-7 gap-1 px-2 text-caption"
            title="新建对话"
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
