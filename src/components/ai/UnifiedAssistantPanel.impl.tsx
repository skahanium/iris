import { useCallback, useEffect, useRef, useState } from "react";

import { AssistantPanelHeader } from "@/components/ai/AssistantPanelHeader";
import {
  AssistantRunCapabilityDegraded,
  AssistantRunWebVerificationFailed,
} from "@/components/ai/AssistantRunCapabilityDegraded";
import { AssistantRunConfirmation } from "@/components/ai/AssistantRunConfirmation";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { Button } from "@/components/ui/button";
import { usePromptProfile } from "@/hooks/usePromptProfile";
import { useAiDomainRuntime } from "@/hooks/useAiDomainRuntime";
import { useAiBubbleSelection } from "@/hooks/useAiBubbleSelection";
import { useAssistantRun } from "@/hooks/useAssistantRun";
import {
  assistantClassifiedContextClear,
  assistantClassifiedContextOpen,
  assistantClassifiedRunTakeResult,
} from "@/lib/ipc";

import type { ImageAttachment } from "./AiMessageList";
import { AssistantComposerDock } from "./AssistantComposerDock";
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
  modelOverride = null,
  onInsertToEditor,
  onOpenWebVerificationSettings,
}: UnifiedAssistantPanelProps) {
  const { profile: promptProfile } = usePromptProfile();
  const assistantRun = useAssistantRun();
  const { reset: resetAssistantRun } = assistantRun;
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
  const [retryingWebVerification, setRetryingWebVerification] = useState(false);
  const [classifiedContextRef, setClassifiedContextRef] = useState<
    string | null
  >(null);
  const [
    includeCurrentClassifiedDocument,
    setIncludeCurrentClassifiedDocument,
  ] = useState(false);
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
  const resetAssistantRunRef = useRef(resetAssistantRun);
  const handleNewChatRef = useRef(handleNewChat);
  useEffect(() => {
    resetAssistantRunRef.current = resetAssistantRun;
    handleNewChatRef.current = handleNewChat;
  }, [handleNewChat, resetAssistantRun]);

  // A classified conversation is strictly tied to one unlocked document view.
  // Domain/path changes invalidate both renderer and backend volatile state.
  useEffect(() => {
    let active = true;
    setIncludeCurrentClassifiedDocument(false);
    setClassifiedContextRef(null);
    resetAssistantRunRef.current();
    handleNewChatRef.current();
    if (aiDomain === "classified" && classifiedPath) {
      void assistantClassifiedContextOpen(classifiedPath)
        .then((context) => {
          if (active) setClassifiedContextRef(context.contextRef);
        })
        .catch(() => {
          if (active)
            setLastError("当前涉密文档不可用于 AI 分析，请确认保险库已解锁。");
        });
    }
    return () => {
      active = false;
      void assistantClassifiedContextClear();
    };
  }, [aiDomain, classifiedPath]);

  const {
    displayMentions,
    handleCompositionEnd,
    handleCompositionStart,
    handleComposerKeyDown,
    handleInputChange,
    mentionCandidates,
    mentionHighlight,
    mentionNavDeltaRef,
    mentionOpen,
    mentionPrefix,
    mentionQuery,
    retrievalScope,
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
    classifiedContextRef,
    takeClassifiedResult: assistantClassifiedRunTakeResult,
  });

  const { isStarting, send } = useUnifiedAssistantSend({
    aiDomain,
    classifiedContextRef,
    includeCurrentClassifiedDocument,
    clearClassifiedDocumentConsent: () =>
      setIncludeCurrentClassifiedDocument(false),
    input,
    images,
    composerDisabled:
      streaming ||
      assistantRun.isBusy ||
      assistantRun.pendingConfirmation !== null,
    session: runSession,
    contextReferences: bubbleSelection.contextReferences,
    displayMentions,
    retrievalScope,
    webSearch,
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
  const refreshClassifiedContext = useCallback(() => {
    if (aiDomain !== "classified" || !classifiedPath) return;
    setClassifiedContextRef(null);
    setIncludeCurrentClassifiedDocument(false);
    void assistantClassifiedContextClear().then(() =>
      assistantClassifiedContextOpen(classifiedPath)
        .then((context) => setClassifiedContextRef(context.contextRef))
        .catch(() =>
          setLastError("当前涉密文档不可用于 AI 分析，请确认保险库已解锁。"),
        ),
    );
  }, [aiDomain, classifiedPath]);
  const resetAssistantSessionState = useCallback(() => {
    resetAssistantRun();
    setLastError(null);
    handleNewChat();
    refreshClassifiedContext();
  }, [handleNewChat, refreshClassifiedContext, resetAssistantRun]);
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
  const handleWebRetry = useCallback(() => {
    setRetryingWebVerification(true);
    setLastError(null);
    setStreaming(true);
    setActivityHint("正在重新联网核实…");
    void assistantRun
      .retryWebVerification()
      .then((accepted) => {
        if (!accepted) setLastError("当前联网失败不可重试，请检查联网配置。");
      })
      .catch(() => {
        setStreaming(false);
        setActivityHint(null);
        setLastError("联网重试未能提交，请稍后再试。");
      })
      .finally(() => setRetryingWebVerification(false));
  }, [assistantRun]);

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
      <p
        className="border-b border-border/60 px-3 py-1 text-[11px] text-muted-foreground"
        data-testid="assistant-security-domain"
      >
        {aiDomain === "classified"
          ? "涉密文档：仅可显式附带当前打开文档，本次对话不会保存。"
          : "普通文档：可通过 @ 显式引用文档。"}
      </p>
      {lastError ? (
        <p className="border-b border-destructive/30 px-3 py-2 text-xs text-destructive">
          {lastError}
        </p>
      ) : null}
      {assistantRun.eventState?.capabilityDegradation ? (
        <AssistantRunCapabilityDegraded
          degradation={assistantRun.eventState.capabilityDegradation}
        />
      ) : null}
      {assistantRun.eventState?.webVerificationFailure ? (
        <AssistantRunWebVerificationFailed
          failure={assistantRun.eventState.webVerificationFailure}
          retrying={retryingWebVerification}
          onRetry={handleWebRetry}
          onCheckConfiguration={onOpenWebVerificationSettings}
        />
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
          streaming={streaming}
          messageListRef={messageListRef}
          onCitationClick={() => undefined}
          onRetract={handleRetract}
          onSelect={bubbleSelection.handleClick}
          onQuoteToInput={handleQuoteToInput}
        />
      </ErrorBoundary>
      <SelectedMessagesActionDock
        count={bubbleSelection.selected.size}
        onClear={bubbleSelection.clear}
        onCopy={handleCopySelected}
        onExport={handleExportSelected}
        onInsert={onInsertToEditor ? handleInsertToEditor : undefined}
      />
      {aiDomain === "classified" ? (
        <div className="border-t border-border/60 px-3 py-2">
          <Button
            type="button"
            size="sm"
            variant={includeCurrentClassifiedDocument ? "secondary" : "outline"}
            disabled={!classifiedContextRef || composerDisabled}
            onClick={() =>
              setIncludeCurrentClassifiedDocument((value) => !value)
            }
            data-testid="classified-current-document-context"
          >
            {includeCurrentClassifiedDocument
              ? "已引用当前涉密文档（仅本次）"
              : "引用当前涉密文档（仅本次）"}
          </Button>
        </div>
      ) : null}
      <AssistantComposerDock
        composerDisabled={composerDisabled}
        images={images}
        input={input}
        displayMentions={displayMentions}
        mentionCandidates={mentionCandidates}
        mentionHighlight={mentionHighlight}
        mentionNavDeltaRef={mentionNavDeltaRef}
        mentionOpen={mentionOpen}
        mentionPrefix={mentionPrefix}
        mentionQuery={mentionQuery}
        streaming={streaming}
        textareaRef={textareaRef}
        onComposerKeyDown={handleComposerKeyDown}
        onCompositionStart={handleCompositionStart}
        onCompositionEnd={handleCompositionEnd}
        onImagesChange={setImages}
        onMentionHighlight={setMentionHighlight}
        onMentionSelect={selectMention}
        onSelect={syncMentionFromInput}
        onStop={stopStreaming}
        onSubmit={() => void send()}
        onValueChange={handleInputChange}
      />
    </div>
  );
}
