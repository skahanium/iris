import { memo, type RefObject } from "react";

import { cn } from "@/lib/utils";
import {
  AiMessageList,
  type AssistantPendingInputCard,
  type ChatLine,
} from "./AiMessageList";
import { AiMessageSelectionUi } from "./AiMessageSelectionUi";

interface ConversationSurfaceProps {
  messages: ChatLine[];
  streaming: boolean;
  pendingInput?: AssistantPendingInputCard | null;
  selectedIndices?: Set<number>;
  messageListRef: RefObject<HTMLDivElement | null>;
  onCitationClick: (ref: string) => void;
  onQuoteToInput: (text: string) => void;
  onRetract?: (index: number) => void;
  onSelect?: (
    index: number,
    event: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean },
  ) => void;
  /** Agent 主区阅读：消息列进入最大 --ai-focus-measure 内容列（§7.3）。 */
  assistantFocus?: boolean;
}

/**
 * 消息流渲染面 — 会话消息列表 + 选区引用工具。
 *
 * 接收拉平的 messages[] 和 streaming 状态，委托 AiMessageList 渲染。
 * 独立于工件流（ArtifactSurface），可单独测试和替换。
 */
export const ConversationSurface = memo(function ConversationSurface({
  messages,
  streaming,
  pendingInput,
  selectedIndices,
  messageListRef,
  onCitationClick,
  onQuoteToInput,
  onRetract,
  onSelect,
  assistantFocus = false,
}: ConversationSurfaceProps) {
  return (
    <div
      ref={messageListRef}
      data-testid="ai-message-list"
      tabIndex={-1}
      className={cn(
        "ai-sidecar-body relative flex min-h-0 flex-1 flex-col focus:outline-none",
        assistantFocus && "ai-focus-column",
      )}
    >
      <AiMessageList
        messages={messages}
        streaming={streaming}
        pendingInput={pendingInput}
        selectedIndices={selectedIndices}
        onCitationClick={onCitationClick}
        onRetract={onRetract}
        onSelect={onSelect}
      />
      <AiMessageSelectionUi
        messageListRef={messageListRef}
        streaming={streaming}
        onQuoteToInput={onQuoteToInput}
      />
    </div>
  );
});
