import { useCallback, useEffect } from "react";

import { AssistantPanelHeader } from "@/components/ai/AssistantPanelHeader";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { usePromptProfile } from "@/hooks/usePromptProfile";
import { useAssistantLlmStream } from "@/hooks/useAssistantLlmStream";
import { useAiDomainRuntime } from "@/hooks/useAiDomainRuntime";
import { useDocSummaryStream } from "@/hooks/useDocSummaryStream";
import { agentTaskAbort, harnessAbort } from "@/lib/ipc";
import { legacySceneHintForTaskPlanIntent } from "@/lib/assistant-scene";
import { buildActionState } from "./unified-assistant-panel-utils";
import { AssistantComposerDock } from "./AssistantComposerDock";
import { ConversationSurface } from "./ConversationSurface";
import { SelectedMessagesActionDock } from "./SelectedMessagesActionDock";
import { useCitationClick } from "./hooks/useCitationClick";
import { ContextPacketDrawer } from "./ContextPacketDrawer";
import { useAiBubbleSelection } from "@/hooks/useAiBubbleSelection";
import { useAssistantRun } from "@/hooks/useAssistantRun";
import { useAssistantConversation } from "./hooks/useAssistantConversation";
import { useAssistantContextScope } from "./hooks/useAssistantContextScope";
import { useAssistantConfirmations } from "./hooks/useAssistantConfirmations";
import { useAssistantArtifacts } from "./hooks/useAssistantArtifacts";
import { useAssistantHarnessResume } from "./hooks/useAssistantHarnessResume";
import { useAssistantPanelEffects } from "./hooks/useAssistantPanelEffects";
import { useAssistantTasks } from "./hooks/useAssistantTasks";
import { useAgentTaskStatus } from "./hooks/useAgentTaskStatus";
import { useResearchControl } from "./hooks/useResearchControl";
import { ContextScopeChips } from "./ContextScopeChips";
import { AssistantTaskSurfaces } from "./AssistantTaskSurfaces";
import { AgentTaskStatusPanel } from "./AgentTaskStatusPanel";
import { AssistantConfirmDialogs } from "./AssistantConfirmDialogs";
import { useAssistantRunPlan } from "./hooks/useAssistantRunPlan";
import { useSelectionQuoteReference } from "./hooks/useSelectionQuoteReference";
import { useAssistantPanelRuntimeState } from "./hooks/useAssistantPanelRuntimeState";
import { AssistantErrorRecovery } from "./AssistantErrorRecovery";
import type { UnifiedAssistantPanelProps } from "./types";

export function UnifiedAssistantPanel({
  aiDomain = "normal",
  classifiedPath = null,
  notePath,
  getNoteContent,
  runtimeDocumentCandidates = [],
  runtimeDocumentSnapshots = [],
  webSearch = false,
  webSearchProviderName = null,
  getWritingContext,
  getParagraphText,
  onPatchApplied,
  onVaultRefresh,
  onInsertToEditor,
  onOpenArtifact,
  onOpenEvidenceSource,
  onSessionDeleted,
  selectionQuote,
  prefillMessage,
  onChromeChange,
}: UnifiedAssistantPanelProps) {
  const panelRuntime = useAssistantPanelRuntimeState();
  const {
    actionState,
    activityHint,
    agentTaskId,
    currentTaskPlanIntent,
    packets,
    requestIdRef,
    setActionState,
    setActivityHint,
    setAgentTaskId,
    setContextStatusData,
    setCurrentTaskPlanIntent,
    setHarnessRequestId,
    setPackets,
    setPacketsOpen,
    setPausedTaskId,
    setProcessEvents,
    setSelectedPacketIds,
    setStreaming,
    setWebSearchUsage,
    streamBuf,
    streaming,
    textareaRef,
  } = panelRuntime;
  const bubbleSelection = useAiBubbleSelection();
  const { pruneSelected } = bubbleSelection;
  const runPlan = useAssistantRunPlan();
  const assistantRun = useAssistantRun("chat");
  const { profile: promptProfile } = usePromptProfile();
  const aiRuntime = useAiDomainRuntime({
    domainState: {
      domain: aiDomain,
      normalActivePath: aiDomain === "normal" ? notePath : null,
      classifiedActivePath: aiDomain === "classified" ? classifiedPath : null,
      classifiedUnlocked: aiDomain === "classified",
    },
  });
  const input = aiRuntime.activeDraft;
  const setInput = aiRuntime.setActiveDraft;
  const { handleCitationClick, citationMiss, clearCitationMiss } =
    useCitationClick(packets, () => setPacketsOpen(true), setSelectedPacketIds);
  const {
    assistantArtifacts,
    citationResult,
    clearTaskSurfaces: clearArtifactSurfaces,
    handleAcceptPatch,
    docIssues,
    docSummary,
    lastError,
    organizeSelection,
    organizeSuggestions,
    researchResult,
    researchState,
    setAssistantArtifacts,
    setCitationResult,
    setDocIssues,
    setDocSummary,
    setLastError,
    setOrganizeSelection,
    setOrganizeSuggestions,
    setResearchResult,
    setResearchState,
    setWritingPatches,
    setWritingState,
    writingState,
    writingPatches,
  } = useAssistantArtifacts({
    getNoteContent,
    onPatchApplied,
    onVaultRefresh,
  });

  function clearTaskSurfaces() {
    clearArtifactSurfaces();
    panelRuntime.clearResearchProgressRef.current?.();
    setProcessEvents([]);
    setAgentTaskId(null);
  }

  const {
    appendAssistantSummary,
    appendUserMessage,
    classifiedThreadId,
    ensureAssistantStreamSlot,
    handleCopySelected,
    handleExportSelected,
    handleInsertToEditor,
    handleLoadSession,
    handleNewChat,
    handleQuoteToInput,
    handleRetract,
    messages,
    saveClassifiedThread,
    sessionId,
    sessionTokenUsage,
    setMessages,
    setSessionId,
    setSessionTokenUsage,
  } = useAssistantConversation({
    actionIntent: actionState.intent,
    aiDomain,
    bubbleSelection,
    clearCitationMiss,
    clearContextReferences: bubbleSelection.clearContextReferences,
    clearTaskSurfaces,
    documentPath: classifiedPath ?? undefined,
    forceNewSessionRef: panelRuntime.forceNewSessionRef,
    onInsertToEditor,
    requestIdRef,
    setActionState,
    setActivityHint,
    setHarnessRequestId,
    setInput,
    setPackets,
    setSelectedPacketIds,
    setStreaming,
    streamBufRef: streamBuf,
    textareaRef,
    streaming,
  });
  useEffect(() => {
    pruneSelected(messages.length);
  }, [pruneSelected, messages.length]);
  const {
    contextScope,
    handleComposerKeyDown,
    mentionCandidates,
    mentionHighlight,
    mentionNavDeltaRef,
    mentionOpen,
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

  const {
    abortResearch,
    clearResearchProgress,
    researchRequestIdRef,
    researchRunning,
    setResearchPanelExpanded,
    setResearchRunning,
  } = useResearchControl({
    researchResult,
    setActionState,
    setLastError,
    setMessages,
  });
  panelRuntime.clearResearchProgressRef.current = clearResearchProgress;

  useAssistantPanelEffects({
    activityHint,
    harnessRequestId: panelRuntime.harnessRequestId,
    messages,
    onChromeChange,
    packets,
    prefillMessage,
    requestIdRef,
    selectionQuote,
    sessionTokenUsage,
    setActionState,
    setAgentTaskId,
    setHarnessRequestId,
    setInput,
    setSessionId,
    streaming,
  });

  useAssistantLlmStream({
    domain: aiDomain,
    panelSendActiveRef: panelRuntime.panelSendActiveRef,
    requestIdRef,
    streamBufRef: streamBuf,
    setActivityHint,
    setMessages,
    setProcessEvents,
    setStreaming,
  });

  useDocSummaryStream({
    docStreamActiveRef: panelRuntime.docStreamActiveRef,
    requestIdRef,
    setDocSummary,
  });

  useSelectionQuoteReference({
    quoteSelectionAsReference: bubbleSelection.quoteSelectionAsReference,
    selectionQuote,
  });

  const handleHarnessResume = useAssistantHarnessResume({
    ensureAssistantStreamSlot,
    harnessRequestId: panelRuntime.harnessRequestId,
    pausedTaskId: panelRuntime.pausedTaskId,
    setActivityHint,
    setLastError,
    setMessages,
    setPackets,
    setAgentTaskId,
    setPausedTaskId,
    setSessionTokenUsage,
    setStreaming,
  });

  const agentTaskStatus = useAgentTaskStatus({
    taskId: agentTaskId,
    setLastError,
    setPausedTaskId,
  });

  const {
    closeRuleConfirm,
    dismissToolConfirm,
    handleRuleConfirm,
    handleToolConfirm,
    ruleConfirmRequest,
    toolConfirmRequest,
  } = useAssistantConfirmations({
    actionIntent: actionState.intent,
    activeSessionId: sessionId,
    assistantRun,
    buildActionState,
    ensureAssistantStreamSlot,
    requestIdRef,
    setActionState,
    setActivityHint,
    setHarnessRequestId,
    setMessages,
    setPackets,
    setSessionTokenUsage,
    setStreaming,
  });

  const composerDisabled =
    streaming || assistantRun.isBusy || toolConfirmRequest !== null;

  const { send, images, setImages } = useAssistantTasks({
    runtime: {
      appendAssistantSummary,
      appendUserMessage,
      assistantRun,
      clearCitationMiss,
      clearTaskSurfaces,
      clearContextReferences: bubbleSelection.clearContextReferences,
      ensureAssistantStreamSlot,
      runPlanControls: runPlan,
      saveConversationSnapshot:
        aiDomain === "classified" ? saveClassifiedThread : undefined,
    },
    context: {
      aiDomain,
      composerDisabled,
      contextScope,
      getNoteContent,
      getParagraphText,
      getWritingContext,
      input,
      messages,
      notePath,
      packets,
      selectedPacketIds: panelRuntime.selectedPacketIds,
      contextReferences: bubbleSelection.contextReferences,
      runtimeDocumentSnapshots,
      acceptWritingPatch: handleAcceptPatch,
      selectionQuoteText: selectionQuote?.text,
      sessionId,
      webSearch,
      writingPatches,
    },
    refs: {
      forceNewSessionRef: panelRuntime.forceNewSessionRef,
      panelSendActiveRef: panelRuntime.panelSendActiveRef,
      requestIdRef,
      researchRequestIdRef,
      streamBufRef: streamBuf,
      docStreamActiveRef: panelRuntime.docStreamActiveRef,
    },
    state: {
      setActionState,
      setActivityHint,
      setAssistantArtifacts,
      setCurrentTaskPlanIntent,
      setCitationResult,
      setContextStatusData,
      setDocIssues,
      setDocSummary,
      setAgentTaskId,
      setHarnessRequestId,
      setInput,
      setLastError,
      setMessages,
      setOrganizeSelection,
      setOrganizeSuggestions,
      setPackets,
      setPacketsOpen,
      setPausedTaskId,
      setResearchPanelExpanded,
      setResearchResult,
      setResearchState,
      setResearchRunning,
      setSessionId,
      setSessionTokenUsage,
      setStreaming,
      setWritingPatches,
      setWritingState,
      setWebSearchUsage,
    },
  });

  function resetAssistantSessionState() {
    setAgentTaskId(null);
    setPausedTaskId(null);
    setCurrentTaskPlanIntent(null);
    setWebSearchUsage(null);
    handleNewChat();
  }
  function loadSessionAndResetTaskPlan(
    ...args: Parameters<typeof handleLoadSession>
  ) {
    setCurrentTaskPlanIntent(null);
    setWebSearchUsage(null);
    handleLoadSession(...args);
  }
  const stopStreaming = useCallback(() => {
    const taskId = agentTaskId;
    const id = requestIdRef.current;
    if (taskId) {
      void agentTaskAbort(taskId);
    } else if (id) {
      void harnessAbort(id);
    }
    panelRuntime.panelSendActiveRef.current = false;
    setStreaming(false);
    setActivityHint(null);
  }, [
    agentTaskId,
    panelRuntime.panelSendActiveRef,
    requestIdRef,
    setActivityHint,
    setStreaming,
  ]);
  const togglePacketSelection = (id: string) => {
    setSelectedPacketIds((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id],
    );
  };
  const currentScene = legacySceneHintForTaskPlanIntent(currentTaskPlanIntent);
  const currentConversationId =
    aiDomain === "classified" ? classifiedThreadId : sessionId;
  return (
    <div
      className="ai-sidecar flex h-full flex-col bg-ai-workspace"
      data-ai-domain={aiDomain}
      data-testid="unified-assistant-panel"
    >
      <AssistantPanelHeader
        chromeActionsDisabled={streaming}
        currentSessionId={currentConversationId}
        domain={aiDomain}
        scene={currentScene}
        onDeletedSession={onSessionDeleted}
        onDeletedCurrentSession={resetAssistantSessionState}
        onNewChat={resetAssistantSessionState}
        onSelectSession={loadSessionAndResetTaskPlan}
        profile={promptProfile}
        taskPlanIntent={currentTaskPlanIntent}
        taskStatus={actionState.status}
        webSearch={webSearch}
        webSearchProviderName={webSearchProviderName}
        webSearchUsage={panelRuntime.webSearchUsage}
      />
      <ContextPacketDrawer
        open={panelRuntime.packetsOpen}
        onOpenChange={setPacketsOpen}
        packets={packets}
        selectedIds={panelRuntime.selectedPacketIds}
        onSelect={togglePacketSelection}
        onOpenSource={onOpenEvidenceSource}
        contextStatus={panelRuntime.contextStatusData}
        citationMiss={citationMiss}
        sessionId={sessionId}
        onOpenArtifact={(draft) => onOpenArtifact?.(draft)}
      />
      <AssistantErrorRecovery
        disabled={streaming}
        harnessRequestId={panelRuntime.harnessRequestId}
        lastError={lastError}
        pausedTaskId={panelRuntime.pausedTaskId}
        onResume={() => void handleHarnessResume()}
      />
      <ErrorBoundary scope="AI任务区">
        <AssistantTaskSurfaces
          assistantArtifacts={assistantArtifacts}
          docSummary={docSummary}
          docIssues={docIssues}
          citationResult={citationResult}
          organizeSuggestions={organizeSuggestions}
          organizeSelection={organizeSelection}
          evidenceRefreshNotice={assistantRun.evidenceRefreshNotice}
          writingPatches={writingPatches}
          writingState={writingState}
          onOpenArtifact={(draft) => onOpenArtifact?.(draft)}
        />
      </ErrorBoundary>
      <ErrorBoundary scope="AI对话区">
        <ConversationSurface
          messages={messages}
          contextReferences={bubbleSelection.contextReferences}
          streaming={streaming}
          processEvents={panelRuntime.processEvents}
          selectedIndices={bubbleSelection.selected}
          messageListRef={panelRuntime.messageListRef}
          onCitationClick={handleCitationClick}
          onRetract={handleRetract}
          onSelect={bubbleSelection.handleClick}
          onQuoteToInput={handleQuoteToInput}
          onRemoveContextReference={bubbleSelection.removeContextReference}
        />
      </ErrorBoundary>
      <ErrorBoundary scope="AI任务状态">
        <AgentTaskStatusPanel
          task={agentTaskStatus.agentTask}
          steps={agentTaskStatus.agentTaskSteps}
          events={agentTaskStatus.agentTaskEvents}
          intentDetection={runPlan.intentDetection}
          onAbort={() => void agentTaskStatus.abortAgentTask()}
          onOpenArtifact={(draft) => onOpenArtifact?.(draft)}
          onResume={() => void handleHarnessResume()}
          permissionPreflightSummary={runPlan.permissionPreflightSummary}
          researchState={researchState}
          runPlanSummary={runPlan.runPlanSummary}
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
        mentionQuery={mentionQuery}
        streaming={streaming}
        textareaRef={textareaRef}
        onComposerKeyDown={handleComposerKeyDown}
        onImagesChange={setImages}
        onMentionHighlight={setMentionHighlight}
        onMentionSelect={selectMention}
        onSelect={syncMentionFromInput}
        onStop={() => {
          if (researchRunning) void abortResearch();
          else if (streaming) stopStreaming();
          else void agentTaskStatus.abortAgentTask();
        }}
        onSubmit={() => void send()}
        onValueChange={setInput}
      />
      <AssistantConfirmDialogs
        ruleConfirmRequest={ruleConfirmRequest}
        toolConfirmRequest={toolConfirmRequest}
        onRuleClose={closeRuleConfirm}
        onRuleConfirm={handleRuleConfirm}
        onRuleReject={closeRuleConfirm}
        onToolClose={dismissToolConfirm}
        onToolConfirm={handleToolConfirm}
      />
    </div>
  );
}
