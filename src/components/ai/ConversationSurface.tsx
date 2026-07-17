import { memo, type RefObject } from "react";

import {
  AiMessageList,
  type AssistantProcessEvent,
  type ChatLine,
} from "./AiMessageList";
import { AiMessageSelectionUi } from "./AiMessageSelectionUi";

interface ConversationSurfaceProps {
  messages: ChatLine[];
  streaming: boolean;
  processEvents?: AssistantProcessEvent[];
  selectedIndices?: Set<number>;
  messageListRef: RefObject<HTMLDivElement | null>;
  onCitationClick: (ref: string) => void;
  onQuoteToInput: (text: string) => void;
  onRetract?: (index: number) => void;
  onSelect?: (
    index: number,
    event: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean },
  ) => void;
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
  processEvents,
  selectedIndices,
  messageListRef,
  onCitationClick,
  onQuoteToInput,
  onRetract,
  onSelect,
}: ConversationSurfaceProps) {
  return (
    <div
      ref={messageListRef}
      data-testid="ai-message-list"
      className="ai-sidecar-body relative flex min-h-0 flex-1 flex-col"
    >
      <AiMessageList
        messages={messages}
        streaming={streaming}
        processEvents={processEvents}
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
