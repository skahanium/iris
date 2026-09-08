import { observeElementOffset, useVirtualizer } from "@tanstack/react-virtual";
import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { ArrowDown, Check, Copy, RotateCcw } from "lucide-react";

import { ScrollArea } from "@/components/ui/scroll-area";
import { AiMessage } from "@/components/ui/ai-message";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { AiMessageBubble } from "@/components/ai/AiMessageBubble";
import { AssistantCitationFooter } from "@/components/ai/AssistantCitationFooter";
import {
  conversationFollowStreamKey,
  useConversationReadingAnchor,
} from "@/components/ai/hooks/useConversationReadingAnchor";
import { assistantMessageIdentity } from "@/lib/ai-message-identity";

import { useToast } from "@/components/ui/use-toast";
import {
  restoreChatLineContent,
  type AiPayloadRef,
} from "@/lib/ai-payload-store";
import type {
  ContentPart,
  CitationBinding,
  DisplayMention,
  RunState,
  SelectionReferenceDisplay,
  ToolCallInfo,
  WebCitationEntry,
} from "@/types/ai";
import type { AssistantProcessItem } from "@/lib/assistant-process";

export interface ImageAttachment {
  id: string;
  dataBase64: string;
  mimeType: string;
  fileName?: string;
  sizeBytes: number;
}

export interface ChatLine {
  role: "user" | "assistant" | "system";
  content: string;
  /** Runtime identity for idempotent Run-to-transcript projection. */
  clientRequestId?: string;
  runId?: string;
  turnId?: string;
  turnState?: RunState;
  retryable?: boolean;
  /** Reference to full content when React state only keeps a bounded projection. */
  contentRef?: AiPayloadRef;
  /** 多模态原始数据（传给后端）；纯文本时为 undefined */
  contentParts?: ContentPart[];
  /** 前端渲染用图片列表 */
  images?: ImageAttachment[];
  /** Inline presentation metadata, separate from retrieval and model input. */
  displayMentions?: DisplayMention[];
  /** Runtime/history-safe marker for a committed editor selection reference. */
  selectionReference?: SelectionReferenceDisplay;
  seq?: number;
  created_at?: string;
  toolCalls?: ToolCallInfo[];
  /** Safe Run progress rendered separately from answer content. */
  processItems?: AssistantProcessItem[];
  webCitations?: WebCitationEntry[];
  citationBinding?: CitationBinding;
  sourceSummary?: import("@/types/ai").SourceSummaryEntry[];
  /** Local-only playback state; durable Run completion must not cancel it. */
  presentationStreaming?: boolean;
  /** Local playback consumes the authoritative body in the bubble, never per-frame transcript writes. */
  answerPresentation?: import("./hooks/useAssistantAnswerReveal").AssistantAnswerPresentation;
}

export interface AssistantPendingInputCard {
  runId: string;
  prompt: string;
  fields: string[];
  values: Record<string, string>;
  submitting: boolean;
  onValueChange: (field: string, value: string) => void;
  onSubmit: () => void;
  onCancel: () => void;
}

interface AiMessageListProps {
  messages: ChatLine[];
  streaming: boolean;
  pendingInput?: AssistantPendingInputCard | null;
  selectedIndices?: Set<number>;
  onCitationClick?: (ref: string) => void;
  onRetract?: (index: number) => void;
  onSelect?: (
    index: number,
    event: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean },
  ) => void;
}

type MessageRow =
  | { type: "empty" }
  | { type: "thinking" }
  | { type: "message"; messageIndex: number }
  | { type: "citations"; messageIndex: number }
  | { type: "input_required"; messageIndex: number };

// Keep copy feedback ASCII-only in source: a legacy WebView code-page decode must not turn the
// user-facing UTF-8 literals into mojibake before React receives them.
const COPY_SUCCESS_TOAST = "\u5df2\u590d\u5236\u56de\u7b54";
const COPY_FAILURE_TOAST = "\u590d\u5236\u5931\u8d25";

function isRenderableMessageRow(
  message: ChatLine,
  messageIndex: number,
  messages: ChatLine[],
  streaming: boolean,
): boolean {
  if (message.role !== "assistant") return true;
  if (message.content.trim()) return true;
  return streaming && messageIndex === messages.length - 1;
}

function isAssistantStreaming(
  message: ChatLine,
  messageIndex: number,
  messages: readonly ChatLine[],
  streaming: boolean,
): boolean {
  return (
    message.presentationStreaming ??
    (streaming &&
      message.role === "assistant" &&
      messageIndex === messages.length - 1)
  );
}

function hasCitationFooter(message: ChatLine): boolean {
  return Boolean(message.webCitations?.length || message.sourceSummary?.length);
}

function isLiveConversationRow(
  row: MessageRow,
  messages: readonly ChatLine[],
  streaming: boolean,
): boolean {
  if (row.type === "thinking") return true;
  if (row.type === "empty" || row.type === "citations") return false;
  if (row.type === "input_required") return true;
  const message = messages[row.messageIndex];
  if (!message) return false;
  return isAssistantStreaming(message, row.messageIndex, messages, streaming);
}

function MessageSelectControl({
  selected,
  onSelect,
}: {
  selected: boolean;
  onSelect?: (event: {
    shiftKey: boolean;
    metaKey: boolean;
    ctrlKey: boolean;
  }) => void;
}) {
  if (!onSelect) return <span className="h-6 w-6" aria-hidden="true" />;

  return (
    <button
      type="button"
      aria-label={selected ? "取消选择此消息" : "选择此消息"}
      aria-pressed={selected}
      title={selected ? "取消选择此消息" : "选择此消息"}
      className={[
        "flex h-6 w-6 items-center justify-center rounded-md border text-[10px] transition",
        selected
          ? "border-primary bg-primary text-primary-foreground opacity-100"
          : "border-border/60 bg-panel/85 text-muted-foreground opacity-0 hover:border-primary/50 hover:text-foreground group-focus-within/ai-message-row:opacity-100 group-hover/ai-message-row:opacity-100",
      ].join(" ")}
      onClick={(event) => {
        event.preventDefault();
        event.stopPropagation();
        onSelect({
          shiftKey: event.shiftKey,
          metaKey: event.metaKey,
          ctrlKey: event.ctrlKey,
        });
      }}
    >
      <Check className="h-3.5 w-3.5" />
    </button>
  );
}

function AssistantMessageActions({
  onCopy,
  onRetract,
  copyDisabled,
}: {
  onCopy?: () => void;
  onRetract?: () => void;
  copyDisabled?: boolean;
}) {
  if (!onCopy && !onRetract) {
    return <span className="h-6 w-6" aria-hidden="true" />;
  }

  return (
    <div className="flex flex-col items-center gap-0.5 opacity-0 transition-opacity group-focus-within/ai-message-row:opacity-100 group-hover/ai-message-row:opacity-100">
      {onCopy ? (
        <button
          type="button"
          title="复制此消息"
          disabled={copyDisabled}
          className="flex h-6 w-6 items-center justify-center rounded text-muted-foreground/45 hover:bg-muted hover:text-muted-foreground"
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            if (copyDisabled) return;
            onCopy();
          }}
        >
          <Copy className="h-3.5 w-3.5" />
        </button>
      ) : null}

      {onRetract ? (
        <button
          type="button"
          title="撤回此消息及后续所有消息"
          className="flex h-6 w-6 items-center justify-center rounded text-muted-foreground/45 hover:bg-muted hover:text-muted-foreground"
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onRetract();
          }}
        >
          <RotateCcw className="h-3.5 w-3.5" />
        </button>
      ) : null}
    </div>
  );
}

function AssistantRunInputCard({
  input,
}: {
  input: AssistantPendingInputCard;
}) {
  const cardRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    cardRef.current
      ?.querySelector<HTMLInputElement>("input")
      ?.focus({ preventScroll: true });
  }, [input.runId]);

  const complete = input.fields.every((field) =>
    Boolean(input.values[field]?.trim()),
  );

  return (
    <section
      ref={cardRef}
      className="rounded-xl border border-border-subtle bg-muted/30 px-3 py-3"
      data-testid="assistant-run-input-required"
      aria-live="polite"
    >
      <p className="text-xs font-medium">{input.prompt}</p>
      <div className="mt-2 flex items-end gap-2">
        <div className="min-w-0 flex-1 space-y-2">
          {input.fields.map((field) => (
            <Input
              key={field}
              value={input.values[field] ?? ""}
              onChange={(event) =>
                input.onValueChange(field, event.target.value)
              }
              placeholder={fieldPlaceholder(field)}
              aria-label={fieldLabel(field)}
              disabled={input.submitting}
              onKeyDown={(event) => {
                if (event.key === "Enter" && complete) input.onSubmit();
              }}
            />
          ))}
        </div>
        <Button
          type="button"
          size="sm"
          variant="brand"
          disabled={!complete || input.submitting}
          onClick={input.onSubmit}
        >
          {input.submitting ? "提交中…" : "继续"}
        </Button>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={input.submitting}
          onClick={input.onCancel}
        >
          取消本轮
        </Button>
      </div>
    </section>
  );
}

function fieldLabel(field: string): string {
  return field === "city" ? "查询城市" : `补充信息：${field}`;
}

function fieldPlaceholder(field: string): string {
  return field === "city" ? "例如：上海" : "请输入补充信息";
}

export const AiMessageList = memo(function AiMessageList({
  messages,
  streaming,
  pendingInput,
  selectedIndices,
  onCitationClick,
  onRetract,
  onSelect,
}: AiMessageListProps) {
  const last = messages[messages.length - 1];
  const lastUserIndex = (() => {
    for (let index = messages.length - 1; index >= 0; index -= 1) {
      if (messages[index]?.role === "user") return index;
    }
    return -1;
  })();
  const lastUser = lastUserIndex >= 0 ? messages[lastUserIndex] : undefined;
  const showStandaloneThinking =
    streaming &&
    (messages.length === 0 ||
      last?.role === "user" ||
      (last?.role === "system" &&
        !messages.some((m) => m.role === "assistant")));
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const geometryCallbackRef = useRef<() => void>(() => undefined);
  const [presentedOffsets, setPresentedOffsets] = useState(
    () => new Map<string, number>(),
  );
  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    const onPresented = (event: Event) => {
      if (!(event instanceof CustomEvent)) return;
      const detail: unknown = event.detail;
      if (!detail || typeof detail !== "object") return;
      const { runId, resetEpoch, length } = detail as Record<string, unknown>;
      if (
        typeof runId !== "string" ||
        typeof resetEpoch !== "number" ||
        typeof length !== "number" ||
        !Number.isFinite(length) ||
        length < 0
      )
        return;
      const key = JSON.stringify([runId, resetEpoch]);
      setPresentedOffsets((offsets) => {
        const previous = offsets.get(key);
        if (previous !== undefined && previous >= length) return offsets;
        const next = new Map(offsets);
        next.set(key, Math.floor(length));
        return next;
      });
    };
    viewport.addEventListener("iris-answer-presented", onPresented);
    return () =>
      viewport.removeEventListener("iris-answer-presented", onPresented);
  }, []);
  const presentations = useMemo(
    () =>
      messages.map((message) => {
        const metadata = message.answerPresentation;
        if (!metadata) return undefined;
        const offset = presentedOffsets.get(
          JSON.stringify([metadata.runId, metadata.resetEpoch]),
        );
        return offset === undefined
          ? metadata
          : { ...metadata, initialVisibleLength: offset };
      }),
    [messages, presentedOffsets],
  );
  const pendingPlayback = useMemo(
    () =>
      messages.map((message, index) => {
        const metadata = presentations[index];
        if (!metadata || metadata.settled) return false;
        const offset = metadata.initialVisibleLength;
        // Pin through the one terminal notification, including a partially shown stop.
        return (
          offset === undefined ||
          (!metadata.stopped &&
            (!metadata.complete ||
              offset < restoreChatLineContent(message).length))
        );
      }),
    [messages, presentations],
  );
  const toast = useToast();
  const conversationRows = useMemo(() => {
    const pendingAssistantExists = pendingInput
      ? messages.some(
          (message) =>
            message.role === "assistant" &&
            message.runId === pendingInput.runId,
        )
      : false;
    const conversationRows: MessageRow[] =
      messages.length === 0
        ? [
            { type: "empty" },
            ...(showStandaloneThinking ? [{ type: "thinking" as const }] : []),
          ]
        : [
            ...(showStandaloneThinking ? [{ type: "thinking" as const }] : []),
            ...messages.flatMap((message, messageIndex) => {
              const ownsPendingInput =
                pendingInput?.runId === message.runId &&
                (message.role === "assistant" ||
                  (!pendingAssistantExists && message.role === "user"));
              const inputRow: MessageRow[] = ownsPendingInput
                ? [{ type: "input_required", messageIndex }]
                : [];
              if (
                !isRenderableMessageRow(
                  message,
                  messageIndex,
                  messages,
                  streaming,
                )
              ) {
                return inputRow;
              }
              const messageRow: MessageRow = { type: "message", messageIndex };
              const citationRow: MessageRow | null =
                message.role === "assistant" &&
                !isAssistantStreaming(
                  message,
                  messageIndex,
                  messages,
                  streaming,
                ) &&
                hasCitationFooter(message)
                  ? { type: "citations", messageIndex }
                  : null;
              return citationRow
                ? [messageRow, citationRow, ...inputRow]
                : [messageRow, ...inputRow];
            }),
          ];
    return [
      ...conversationRows.filter((row) => row.type !== "thinking"),
      ...conversationRows.filter((row) => row.type === "thinking"),
    ];
  }, [messages, pendingInput, showStandaloneThinking, streaming]);

  const messagesForIdentityRef = useRef(messages);
  messagesForIdentityRef.current = messages;
  const getItemKey = useCallback(
    (index: number): string => {
      const row = conversationRows[index];
      if (!row || row.type === "empty" || row.type === "thinking") {
        return row?.type ?? `row:${index}`;
      }
      const message = messagesForIdentityRef.current[row.messageIndex];
      const messageIdentity = message
        ? assistantMessageIdentity(message, row.messageIndex)
        : `row:${index}`;
      if (row.type === "citations") return `${messageIdentity}:citations`;
      if (row.type === "input_required") {
        return `${messageIdentity}:input-required`;
      }
      return messageIdentity;
    },
    [conversationRows],
  );

  // Actual row height comes exclusively from the batched ResizeObserver. These
  // conservative first-render estimates deliberately do not change per token,
  // so streaming text cannot churn virtual total size before measurement.
  const estimateRowSize = useCallback(
    (index: number): number => {
      const row = conversationRows[index];
      if (!row || row.type === "empty" || row.type === "thinking") return 80;
      if (row.type === "citations") return 72;
      if (row.type === "input_required") return 144;
      const message = messages[row.messageIndex];
      if (!message) return 112;
      return message.role === "assistant" ? 168 : 96;
    },
    [conversationRows, messages],
  );

  const rowHeightsRef = useRef(new Map<string, number>());
  const rowVirtualizer = useVirtualizer({
    count: conversationRows.length,
    getScrollElement: () => viewportRef.current,
    initialOffset: () => viewportRef.current?.scrollTop ?? 0,
    // Initialization and reconciliation must also leave scrolling to the anchor.
    scrollToFn: () => undefined,
    observeElementOffset: (instance, callback) => {
      // Refs were empty during render; seed the mounted viewport before events.
      callback(instance.scrollElement?.scrollTop ?? 0, false);
      return observeElementOffset(instance, callback);
    },
    getItemKey,
    estimateSize: estimateRowSize,
    measureElement: (element, entry) => {
      const index = Number(element.getAttribute("data-index"));
      const height =
        (entry?.borderBoxSize?.[0]?.blockSize ??
          element.getBoundingClientRect().height) ||
        estimateRowSize(index);
      rowHeightsRef.current.set(getItemKey(index), height);
      return height;
    },
    overscan: 8,
    paddingStart: 12,
    paddingEnd: 12,
    initialRect: { width: 420, height: 640 },
    onChange: () => geometryCallbackRef.current(),
  });
  // The reading anchor is the sole scroll writer, including delayed row sizes.
  rowVirtualizer.shouldAdjustScrollPositionOnItemSizeChange = () => false;
  const measureRowElement = rowVirtualizer.measureElement;
  const virtualItems = rowVirtualizer.getVirtualItems();
  const visibleRows = new Set(virtualItems.map((item) => item.index));
  const measuredRows = new Map(
    virtualItems.map((item) => [item.index, item.size]),
  );
  for (const [index, height] of measuredRows) {
    if (height > 0) rowHeightsRef.current.set(getItemKey(index), height);
  }
  const activeStreamingMessage =
    last?.role === "assistant" &&
    (isAssistantStreaming(last, messages.length - 1, messages, streaming) ||
      pendingPlayback[messages.length - 1])
      ? last
      : null;
  const activeStreamKey = activeStreamingMessage
    ? assistantMessageIdentity(activeStreamingMessage, messages.length - 1)
    : null;
  const { following, returnToLatest, scheduleGeometry } =
    useConversationReadingAnchor({
      viewportRef,
      active:
        streaming || activeStreamingMessage != null || pendingInput != null,
      revision: conversationRows.length,
      streamKey: conversationFollowStreamKey({
        live:
          streaming || activeStreamingMessage != null || pendingInput != null,
        userClientRequestId: lastUser?.clientRequestId,
        userRunId: lastUser?.runId,
        activeStreamKey,
        pendingInputRunId: pendingInput?.runId,
      }),
    });
  geometryCallbackRef.current = scheduleGeometry;

  // Stable per-index callback cache. Inline arrows like `() => onRetract(i)`
  // create new function refs every render, breaking AiMessageBubble's memo
  // during animation-frame streaming updates. This Map persists
  // across renders so each index always gets the same function ref.
  const retractCallbackRef = useRef<Map<number, () => void>>(new Map());
  const copyCallbackRef = useRef<Map<number, () => void>>(new Map());
  const messagesRef = useRef(messages);

  useEffect(() => {
    messagesRef.current = messages;
  }, [messages]);

  useEffect(() => {
    for (const key of retractCallbackRef.current.keys()) {
      if (key >= messages.length) retractCallbackRef.current.delete(key);
    }
    for (const key of copyCallbackRef.current.keys()) {
      if (key >= messages.length) copyCallbackRef.current.delete(key);
    }
  }, [messages.length]);

  const handleMessageSelect = useCallback(
    (
      index: number,
      event: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean },
    ) => {
      onSelect?.(index, event);
    },
    [onSelect],
  );

  const handleCopyMessage = useCallback(
    async (message: ChatLine) => {
      const content = restoreChatLineContent(message);
      try {
        if (!navigator.clipboard?.writeText) {
          throw new Error("Clipboard API is unavailable");
        }
        await navigator.clipboard.writeText(content);
        toast(COPY_SUCCESS_TOAST, { tone: "success" });
      } catch {
        toast(COPY_FAILURE_TOAST, { tone: "error" });
      }
    },
    [toast],
  );

  const renderRow = (row: MessageRow) => {
    if (row.type === "empty") {
      return (
        <p className="py-8 text-center text-xs text-muted-foreground">
          输入问题开始对话。处理过程会显示在回答气泡内。
        </p>
      );
    }

    if (row.type === "thinking") {
      return <AiMessageBubble role="assistant" streaming />;
    }

    if (row.type === "citations") {
      const message = messages[row.messageIndex];
      if (!message || message.role !== "assistant") return null;
      return (
        <div className="assistant-citation-row pl-7" data-row-kind="citations">
          <AssistantCitationFooter
            content={message.content}
            entries={message.webCitations ?? []}
            binding={message.citationBinding}
            sourceSummary={message.sourceSummary}
            onOpenUrl={onCitationClick}
          />
        </div>
      );
    }

    if (row.type === "input_required") {
      if (!pendingInput) return null;
      return (
        <div className="pl-7" data-row-kind="input_required">
          <AssistantRunInputCard input={pendingInput} />
        </div>
      );
    }

    const i = row.messageIndex;
    const m = messages[i];
    if (!m) return null;
    const isLast = i === messages.length - 1;
    const assistantStreaming = isAssistantStreaming(m, i, messages, streaming);
    const isSelected = selectedIndices?.has(i) ?? false;

    if (m.role === "assistant") {
      const msgContent = restoreChatLineContent(m);
      // Fetch or create stable callbacks for this index. The Map persists
      // across renders, so the same index always gets the same function ref,
      // preserving AiMessageBubble's memo during streaming re-renders.
      let retractCb = retractCallbackRef.current.get(i);
      if (!retractCb && onRetract) {
        retractCb = () => onRetract(i);
        retractCallbackRef.current.set(i, retractCb);
      }
      let copyCb = copyCallbackRef.current.get(i);
      if (!copyCb) {
        copyCb = () => {
          const latestMessage = messagesRef.current[i];
          if (latestMessage) void handleCopyMessage(latestMessage);
        };
        copyCallbackRef.current.set(i, copyCb);
      }
      return (
        <div className="group/ai-message-row grid w-full grid-cols-[1.75rem_minmax(0,1fr)] items-start gap-1">
          <div className="flex flex-col items-center gap-1 pt-1">
            <MessageSelectControl
              selected={isSelected}
              onSelect={
                onSelect ? (event) => handleMessageSelect(i, event) : undefined
              }
            />
            <AssistantMessageActions
              onCopy={copyCb}
              onRetract={retractCb}
              copyDisabled={!msgContent}
            />
          </div>
          <div className="min-w-0 max-w-full flex-1">
            <AiMessageBubble
              key={assistantMessageIdentity(m, i)}
              role="assistant"
              content={msgContent || undefined}
              answerPresentation={presentations[i]}
              messageIdentity={assistantMessageIdentity(m, i)}
              streaming={assistantStreaming}
              processItems={m.processItems}
              selected={isSelected}
              isLastMessage={isLast}
              createdAt={m.created_at}
              onCitationClick={onCitationClick}
              webCitations={m.webCitations}
            />
          </div>
        </div>
      );
    }

    if (m.role === "user") {
      const userContent = restoreChatLineContent(m);
      return (
        <div className="group/ai-message-row flex w-full items-start justify-end gap-1">
          <div className="pt-1">
            <MessageSelectControl
              selected={isSelected}
              onSelect={
                onSelect ? (event) => handleMessageSelect(i, event) : undefined
              }
            />
          </div>
          <div className="flex min-w-0 flex-1 flex-col items-end gap-1">
            <AiMessageBubble
              role="user"
              content={userContent}
              selected={isSelected}
              images={m.images}
              displayMentions={m.displayMentions}
              selectionReference={m.selectionReference}
            />
            {m.turnState === "failed" ? (
              <p className="text-[10px] text-muted-foreground">
                本次请求未完成，未纳入后续对话上下文。
                {m.retryable ? " 可重试。" : ""}
              </p>
            ) : m.turnState === "cancelled" ? (
              <p className="text-[10px] text-muted-foreground">
                本次回答已取消，未纳入后续对话上下文。
              </p>
            ) : null}
          </div>
        </div>
      );
    }

    return <AiMessage role={m.role} content={m.content} />;
  };

  const mountedRows = new Map(
    Array.from(
      viewportRef.current?.querySelectorAll<HTMLElement>(
        "[data-conversation-row]",
      ) ?? [],
    ).map((node) => [Number(node.dataset.conversationRow), node]),
  );
  const nativeSelection = window.getSelection();
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <ScrollArea
        className="ai-conversation-scroll min-h-0 flex-1"
        viewportRef={viewportRef}
      >
        <div className="relative py-3" data-conversation-content="">
          {conversationRows.map((row, index) => {
            const key = getItemKey(index);
            const existing = mountedRows.get(index);
            const live =
              isLiveConversationRow(row, messages, streaming) ||
              (row.type === "message" && pendingPlayback[row.messageIndex]);
            const selection = nativeSelection;
            const preserved =
              existing &&
              (existing.contains(document.activeElement) ||
                (selection &&
                  !selection.isCollapsed &&
                  selection.containsNode(existing, true)));
            const mounted =
              conversationRows.length <= 40 ||
              visibleRows.has(index) ||
              live ||
              preserved ||
              (row.type === "message" &&
                (row.messageIndex === lastUserIndex ||
                  selectedIndices?.has(row.messageIndex)));
            return (
              <div
                key={key}
                ref={mounted ? measureRowElement : undefined}
                data-index={index}
                data-conversation-row={index}
                data-row-identity={key}
                data-row-kind={row.type}
                data-live-stream={live ? "" : undefined}
                data-conversation-park={
                  row.type === "message" && row.messageIndex === lastUserIndex
                    ? ""
                    : undefined
                }
                className="w-full px-3"
                style={
                  mounted
                    ? undefined
                    : {
                        height:
                          rowHeightsRef.current.get(key) ??
                          estimateRowSize(index),
                      }
                }
              >
                {mounted ? <div className="pb-4">{renderRow(row)}</div> : null}
              </div>
            );
          })}
        </div>
        <div
          className="h-24 shrink-0"
          aria-hidden="true"
          data-conversation-spacer=""
        />
      </ScrollArea>
      {messages.length > 0 && !following ? (
        <button
          type="button"
          aria-label="回到最新"
          title="回到最新"
          className="absolute bottom-4 left-1/2 flex h-10 w-10 -translate-x-1/2 items-center justify-center rounded-full border border-border-subtle bg-panel text-foreground shadow-sm transition hover:bg-muted"
          onClick={returnToLatest}
        >
          <ArrowDown className="h-4 w-4" />
        </button>
      ) : null}
    </div>
  );
});
