import { useCallback, useRef, useState } from "react";

import { AssistantPanelHeader } from "@/components/ai/AssistantPanelHeader";
import { AssistantRunConfirmation } from "@/components/ai/AssistantRunConfirmation";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { usePromptProfile } from "@/hooks/usePromptProfile";
import { useAiDomainRuntime } from "@/hooks/useAiDomainRuntime";
import { useAiBubbleSelection } from "@/hooks/useAiBubbleSelection";
import { useAssistantRun } from "@/hooks/useAssistantRun";

import type { ImageAttachment } from "./AiMessageList";
import { AssistantComposerDock } from "./AssistantComposerDock";
import { ContextScopeChips } from "./ContextScopeChips";
import { ConversationSurface } from "./ConversationSurface";
import { SelectedMessagesActionDock } from "./SelectedMessagesActionDock";
import { useAssistantContextScope } from "./hooks/useAssistantContextScope";
import { useAssistantConversation } from "./hooks/useAssistantConversation";
import { useAssistantRunTranscript } from "./hooks/useAssistantRunTranscript";
import { useUnifiedAssistantSend } from "./hooks/useUnifiedAssistantSend";
import type { UnifiedAssistantPanelProps } from "./types";

/** Production assistant panel: one opaque conversation API and one Run lifecycle. */
export function UnifiedAssistantPanel({
  aiDomain = "normal",
  classifiedPath = null,
  runtimeDocumentCandidates = [],
  webSearch = false,
  webSearchProviderName = null,
  routingPolicy,
  modelOverride = null,
  onInsertToEditor,
}: UnifiedAssistantPanelProps) {
  const { profile: promptProfile } = usePromptProfile();
  const assistantRun = useAssistantRun();
  const aiRuntime = useAiDomainRuntime({
    domainState: {
      domain: aiDomain,
      normalActivePath: null,
      classifiedActivePath: aiDomain === "classified" ? classifiedPath : null,
      classifiedUnlocked: aiDomain === "classified",
    },
  });
  const input = aiRuntime.activeDraft;
  const setInput = aiRuntime.setActiveDraft;
  const bubbleSelection = useAiBubbleSelection();
  const [streaming, setStreaming] = useState(false);
  const [, setActivityHint] = useState<string | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);
  const [images, setImages] = useState<ImageAttachment[]>([]);
  const [confirming, setConfirming] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const messageListRef = useRef<HTMLDivElement>(null);

  const clearTaskSurfaces = useCallback(() => undefined, []);

  const {
    appendUserMessage,
    ensureAssistantStreamSlot,
    handleCopySelected,
    handleExportSelected,
    handleInsertToEditor,
    handleLoadSession,
    handleNewChat,
    handleQuoteToInput,
    handleRetract,
    messages,
    runSession,
    setMessages,
    setRunSession,
  } = useAssistantConversation({
    bubbleSelection,
    clearContextReferences: bubbleSelection.clearContextReferences,
    clearTaskSurfaces,
    onInsertToEditor,
    setInput,
    setStreaming,
    textareaRef,
  });

  const {
    handleComposerKeyDown,
    mentionCandidates,
    mentionHighlight,
    mentionNavDeltaRef,
    mentionOpen,
    mentionPrefix,
    mentionQuery,
    mentionTokens,
    removeMentionToken,
    selectMention,
    setMentionHighlight,
    syncMentionFromInput,
  } = useAssistantContextScope({
    input,
    setInput,
    textareaRef,
    runtimeDocumentCandidates,
  });

  useAssistantRunTranscript({
    run: assistantRun.eventState,
    setMessages,
    setStreaming,
    setActivityHint,
    setError: setLastError,
  });

  const { isStarting, send } = useUnifiedAssistantSend({
    aiDomain,
    input,
    images,
    composerDisabled:
      streaming ||
      assistantRun.isBusy ||
      assistantRun.pendingConfirmation !== null,
    session: runSession,
    contextReferences: bubbleSelection.contextReferences,
    webSearch,
    routingPolicy,
    modelOverride,
    start: assistantRun.start,
    appendUserMessage,
    ensureAssistantStreamSlot,
    clearContextReferences: bubbleSelection.clearContextReferences,
    setInput,
    setImages,
    setSession: setRunSession,
    setStreaming,
    setActivityHint,
    setError: setLastError,
  });

  const composerDisabled =
    streaming ||
    assistantRun.isBusy ||
    isStarting ||
    assistantRun.pendingConfirmation !== null;
  const stopStreaming = useCallback(() => {
    void assistantRun.cancel();
  }, [assistantRun]);
  const resetAssistantSessionState = useCallback(() => {
    assistantRun.reset();
    setLastError(null);
    handleNewChat();
  }, [assistantRun, handleNewChat]);
  const handleConfirmation = useCallback(
    (decision: "approve" | "reject") => {
      setConfirming(true);
      const action =
        decision === "approve"
          ? assistantRun.approveChange()
          : assistantRun.rejectChange();
      void action
        .catch(() => {
          setLastError("确认操作未能提交，请稍后重试。");
        })
        .finally(() => {
          setConfirming(false);
        });
    },
    [assistantRun],
  );

  return (
    <div
      className="ai-sidecar flex h-full flex-col bg-ai-workspace"
      data-ai-domain={aiDomain}
      data-testid="unified-assistant-panel"
    >
      <AssistantPanelHeader
        chromeActionsDisabled={composerDisabled}
        currentSession={runSession}
        domain={aiDomain}
        onDeletedCurrentSession={resetAssistantSessionState}
        onNewChat={resetAssistantSessionState}
        onSelectSession={(session, loaded, activeRun) => {
          handleLoadSession(session, loaded);
          if (activeRun) assistantRun.recover(activeRun);
          else assistantRun.reset();
        }}
        profile={promptProfile}
        runState={assistantRun.runState}
        webSearch={webSearch}
        webSearchProviderName={webSearchProviderName}
      />
      {lastError ? (
        <p className="border-b border-destructive/30 px-3 py-2 text-xs text-destructive">
          {lastError}
        </p>
      ) : null}
      {assistantRun.pendingConfirmation ? (
        <AssistantRunConfirmation
          confirmation={assistantRun.pendingConfirmation}
          disabled={confirming}
          onApprove={() => handleConfirmation("approve")}
          onReject={() => handleConfirmation("reject")}
        />
      ) : null}
      {assistantRun.eventState?.provider ? (
        <p
          className="border-b border-border/60 px-3 py-1 text-[11px] text-muted-foreground"
          data-testid="assistant-run-provider-diagnostic"
        >
          当前模型：{assistantRun.eventState.provider.providerId}
          {assistantRun.eventState.provider.modelId
            ? ` / ${assistantRun.eventState.provider.modelId}`
            : ""}
        </p>
      ) : null}
      <ErrorBoundary scope="AI 对话区">
        <ConversationSurface
          messages={messages}
          contextReferences={bubbleSelection.contextReferences}
          streaming={streaming}
          messageListRef={messageListRef}
          onCitationClick={() => undefined}
          onRetract={handleRetract}
          onSelect={bubbleSelection.handleClick}
          onQuoteToInput={handleQuoteToInput}
          onRemoveContextReference={bubbleSelection.removeContextReference}
        />
      </ErrorBoundary>
      <SelectedMessagesActionDock
        count={bubbleSelection.selected.size}
        onClear={bubbleSelection.clear}
        onCopy={handleCopySelected}
        onExport={handleExportSelected}
        onInsert={onInsertToEditor ? handleInsertToEditor : undefined}
      />
      <ContextScopeChips tokens={mentionTokens} onRemove={removeMentionToken} />
      <AssistantComposerDock
        composerDisabled={composerDisabled}
        images={images}
        input={input}
        mentionCandidates={mentionCandidates}
        mentionHighlight={mentionHighlight}
        mentionNavDeltaRef={mentionNavDeltaRef}
        mentionOpen={mentionOpen}
        mentionPrefix={mentionPrefix}
        mentionQuery={mentionQuery}
        streaming={streaming}
        textareaRef={textareaRef}
        onComposerKeyDown={handleComposerKeyDown}
        onImagesChange={setImages}
        onMentionHighlight={setMentionHighlight}
        onMentionSelect={selectMention}
        onSelect={syncMentionFromInput}
        onStop={stopStreaming}
        onSubmit={() => void send()}
        onValueChange={setInput}
      />
    </div>
  );
}
