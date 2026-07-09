import { memo, type RefObject } from "react";

import { contextReferenceDisplayText } from "@/lib/context-reference";
import type { ContextReference } from "@/types/ai";

import {
  AiMessageList,
  type AssistantProcessEvent,
  type ChatLine,
} from "./AiMessageList";
import { AiMessageSelectionUi } from "./AiMessageSelectionUi";

interface ConversationSurfaceProps {
  messages: ChatLine[];
  contextReferences?: ContextReference[];
  streaming: boolean;
  processEvents?: AssistantProcessEvent[];
  selectedIndices?: Set<number>;
  messageListRef: RefObject<HTMLDivElement | null>;
  onCitationClick: (ref: string) => void;
  onQuoteToInput: (text: string) => void;
  onRemoveContextReference?: (id: string) => void;
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
  contextReferences = [],
  streaming,
  processEvents,
  selectedIndices,
  messageListRef,
  onCitationClick,
  onQuoteToInput,
  onRemoveContextReference,
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
      {contextReferences.length > 0 ? (
        <div className="border-t border-border/60 px-3 py-2">
          <div className="flex flex-wrap gap-1.5">
            {contextReferences.map((reference) => (
              <span
                key={reference.id}
                className="inline-flex max-w-full items-center gap-1 rounded-md border border-border/70 bg-surface-inset px-2 py-1 text-xs text-muted-foreground"
                title={contextReferenceDisplayText(reference)}
              >
                <span className="min-w-0 truncate">
                  {contextReferenceDisplayText(reference)}
                </span>
                {onRemoveContextReference ? (
                  <button
                    type="button"
                    className="shrink-0 text-muted-foreground hover:text-foreground"
                    onClick={() => onRemoveContextReference(reference.id)}
                    aria-label="移除引用"
                  >
                    ×
                  </button>
                ) : null}
              </span>
            ))}
          </div>
        </div>
      ) : null}
      <AiMessageSelectionUi
        messageListRef={messageListRef}
        streaming={streaming}
        onQuoteToInput={onQuoteToInput}
      />
    </div>
  );
});
