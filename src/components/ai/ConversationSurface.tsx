import type { RefObject } from "react";

import { AiMessageList, type ChatLine } from "./AiMessageList";
import { AiMessageSelectionUi } from "./AiMessageSelectionUi";
import type { ResearchFocusPayload } from "@/types/ai";

interface ConversationSurfaceProps {
  messages: ChatLine[];
  streaming: boolean;
  selectedIndices?: Set<number>;
  messageListRef: RefObject<HTMLDivElement | null>;
  onCitationClick: (ref: string) => void;
  onExpandResearch: (result: ResearchFocusPayload) => void;
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
export function ConversationSurface({
  messages,
  streaming,
  selectedIndices,
  messageListRef,
  onCitationClick,
  onExpandResearch,
  onQuoteToInput,
  onRetract,
  onSelect,
}: ConversationSurfaceProps) {
  return (
    <div
      ref={messageListRef}
      data-testid="ai-message-list"
      className="relative flex min-h-0 flex-1 flex-col"
    >
      <AiMessageList
        messages={messages}
        streaming={streaming}
        selectedIndices={selectedIndices}
        onCitationClick={onCitationClick}
        onExpandResearch={onExpandResearch}
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
}
