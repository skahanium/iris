import { useCallback, useRef, useState } from "react";

import { AssistantPanelHeader } from "@/components/ai/AssistantPanelHeader";
import { AuditTrailDrawer } from "@/components/ai/AuditTrailDrawer";
import { AiComposer } from "@/components/ui/ai-composer";
import { usePromptProfile } from "@/hooks/usePromptProfile";
import { useAssistantLlmStream } from "@/hooks/useAssistantLlmStream";
import { legacySceneHintForAssistantIntent } from "@/lib/assistant-scene";
import { harnessAbort } from "@/lib/ipc";
import type {
  AssistantActionState,
  ContextPacket,
  ContextStatus,
} from "@/types/ai";

import { buildActionState } from "./unified-assistant-panel-utils";
import { AiMentionPopover } from "./AiMentionPopover";
import { AiComposerContextMenu } from "./AiComposerContextMenu";
import { ConversationSurface } from "./ConversationSurface";
import { AiSelectionActionBar } from "./AiSelectionActionBar";
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
import { RuleConfirmDialog } from "./RuleConfirmDialog";
import { ToolConfirmDialog } from "./ToolConfirmDialog";
import { useAssistantRunPlan } from "./hooks/useAssistantRunPlan";
import { AssistantErrorRecovery } from "./AssistantErrorRecovery";
import type { UnifiedAssistantPanelProps } from "./types";

export function UnifiedAssistantPanel({
  notePath,
  getNoteContent,
  webSearch = false,
  getWritingContext,
  getParagraphText,
  onPatchApplied,
  onVaultRefresh,
  onInsertToEditor,
  selectionQuote,
  prefillMessage,
  onChromeChange,
}: UnifiedAssistantPanelProps) {
  const [actionState, setActionState] = useState<AssistantActionState>(
    buildActionState("chat", "idle"),
  );
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const bubbleSelection = useAiBubbleSelection();
  const [packets, setPackets] = useState<ContextPacket[]>([]);
  const [selectedPacketIds, setSelectedPacketIds] = useState<string[]>([]);
  const [packetsOpen, setPacketsOpen] = useState(false);

  const [contextStatusData, setContextStatusData] =
    useState<ContextStatus | null>(null);
  const [activityHint, setActivityHint] = useState<string | null>(null);
  const streamBuf = useRef("");
  const requestIdRef = useRef<string | null>(null);
  const [harnessRequestId, setHarnessRequestId] = useState<string | null>(null);
  const [agentTaskId, setAgentTaskId] = useState<string | null>(null);
  const [pausedTaskId, setPausedTaskId] = useState<string | null>(null);
  const [auditDrawerOpen, setAuditDrawerOpen] = useState(false);
  const runPlan = useAssistantRunPlan();
  const assistantRun = useAssistantRun("chat");
  const clearResearchProgressRef = useRef<(() => void) | null>(null);
  const panelSendActiveRef = useRef(false);
  const forceNewSessionRef = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const messageListRef = useRef<HTMLDivElement>(null);
  const { profile: promptProfile } = usePromptProfile();

  const { handleCitationClick, citationMiss, clearCitationMiss } =
    useCitationClick(packets, () => setPacketsOpen(true), setSelectedPacketIds);

  const {
    citationResult,
    clearTaskSurfaces: clearArtifactSurfaces,
    docIssues,
    docSummary,
    handleAcceptOrganize,
    handleAcceptPatch,
    handleClearOrganizeSelection,
    handleCopyPatch,
    handleRejectPatch,
    handleToggleOrganizeSuggestion,
    lastError,
    organizeSelection,
    organizeSuggestions,
    researchResult,
    researchState,
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

  const clearTaskSurfaces = useCallback(() => {
    clearArtifactSurfaces();
    clearResearchProgressRef.current?.();
    setAgentTaskId(null);
  }, [clearArtifactSurfaces]);

  const {
    appendAssistantSummary,
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
    sessionId,
    sessionTokenUsage,
    setMessages,
    setSessionId,
    setSessionTokenUsage,
  } = useAssistantConversation({
    actionIntent: actionState.intent,
    bubbleSelection,
    clearCitationMiss,
    clearTaskSurfaces,
    forceNewSessionRef,
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
  });

  const {
    abortResearch,
    clearResearchProgress,
    generatingResearchNote,
    handleGenerateResearchNote,
    researchDetailRef,
    researchPanelExpanded,
    researchProgress,
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
  clearResearchProgressRef.current = clearResearchProgress;

  useAssistantPanelEffects({
    actionState,
    activityHint,
    harnessRequestId,
    messages,
    onChromeChange,
    packets,
    prefillMessage,
    requestIdRef,
    selectionQuote,
    sessionTokenUsage,
    setActionState,
    setAuditDrawerOpen,
    setHarnessRequestId,
    setInput,
    streaming,
  });

  useAssistantLlmStream({
    panelSendActiveRef,
    requestIdRef,
    streamBufRef: streamBuf,
    setMessages,
    setStreaming,
  });

  const handleHarnessResume = useAssistantHarnessResume({
    ensureAssistantStreamSlot,
    harnessRequestId,
    pausedTaskId,
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

  const { runWriting, send, images, setImages } = useAssistantTasks({
    runtime: {
      appendAssistantSummary,
      appendUserMessage,
      assistantRun,
      clearCitationMiss,
      clearTaskSurfaces,
      ensureAssistantStreamSlot,
      runPlanControls: runPlan,
    },
    context: {
      composerDisabled,
      contextScope,
      getNoteContent,
      getParagraphText,
      getWritingContext,
      input,
      messages,
      notePath,
      packets,
      selectedPacketIds,
      selectionQuoteText: selectionQuote?.text,
      sessionId,
      webSearch,
    },
    refs: {
      forceNewSessionRef,
      panelSendActiveRef,
      requestIdRef,
      researchRequestIdRef,
      streamBufRef: streamBuf,
    },
    state: {
      setActionState,
      setActivityHint,
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
    },
  });

  const resetAssistantSessionState = useCallback(() => {
    setAgentTaskId(null);
    setPausedTaskId(null);
    handleNewChat();
  }, [handleNewChat]);

  const stopStreaming = useCallback(() => {
    const id = requestIdRef.current;
    if (id) {
      void harnessAbort(id);
    }
    panelSendActiveRef.current = false;
    setStreaming(false);
    setActivityHint(null);
  }, []);

  const togglePacketSelection = useCallback((id: string) => {
    setSelectedPacketIds((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id],
    );
  }, []);

  return (
    <div
      className="ai-sidecar flex h-full flex-col bg-ai-workspace"
      data-testid="unified-assistant-panel"
    >
      <AssistantPanelHeader
        chromeActionsDisabled={streaming}
        currentSessionId={sessionId}
        harnessRequestId={harnessRequestId}
        legacySceneHint={legacySceneHintForAssistantIntent(actionState.intent)}
        notePath={notePath}
        onClearedAllSessions={resetAssistantSessionState}
        onDeletedCurrentSession={resetAssistantSessionState}
        onNewChat={resetAssistantSessionState}
        onOpenAudit={() => setAuditDrawerOpen(true)}
        onSelectSession={handleLoadSession}
        profile={promptProfile}
        webSearch={webSearch}
      />

      <ContextPacketDrawer
        open={packetsOpen}
        onOpenChange={setPacketsOpen}
        packets={packets}
        selectedIds={selectedPacketIds}
        onSelect={togglePacketSelection}
        contextStatus={contextStatusData}
        citationMiss={citationMiss}
      />

      <AssistantErrorRecovery
        disabled={streaming}
        harnessRequestId={harnessRequestId}
        lastError={lastError}
        pausedTaskId={pausedTaskId}
        onResume={() => void handleHarnessResume()}
      />

      <AssistantTaskSurfaces
        activityHint={activityHint}
        agentTask={agentTaskStatus.agentTask}
        researchProgress={researchProgress}
        researchRunning={researchRunning}
        onAbortProcess={() => {
          if (researchRunning) void abortResearch();
          else void agentTaskStatus.abortAgentTask();
        }}
        researchResult={researchResult}
        researchPanelExpanded={researchPanelExpanded}
        researchDetailRef={researchDetailRef}
        generatingResearchNote={generatingResearchNote}
        onGenerateResearchNote={() => void handleGenerateResearchNote()}
        docSummary={docSummary}
        docIssues={docIssues}
        citationResult={citationResult}
        organizeSuggestions={organizeSuggestions}
        organizeSelection={organizeSelection}
        onClearOrganizeSelection={handleClearOrganizeSelection}
        onToggleOrganizeSuggestion={handleToggleOrganizeSuggestion}
        onAcceptOrganize={() => void handleAcceptOrganize()}
        evidenceRefreshNotice={assistantRun.evidenceRefreshNotice}
        writingPatches={writingPatches}
        writingState={writingState}
        onAcceptPatch={(item) => void handleAcceptPatch(item)}
        onRejectPatch={handleRejectPatch}
        onCopyPatch={(item) => void handleCopyPatch(item)}
        onRegenerateWriting={() => {
          if (!input.trim()) return;
          void runWriting(input.trim());
        }}
      />

      <ConversationSurface
        messages={messages}
        streaming={streaming}
        selectedIndices={bubbleSelection.selected}
        messageListRef={messageListRef}
        onCitationClick={handleCitationClick}
        onRetract={handleRetract}
        onSelect={bubbleSelection.handleClick}
        onQuoteToInput={handleQuoteToInput}
      />

      <AgentTaskStatusPanel
        task={agentTaskStatus.agentTask}
        steps={agentTaskStatus.agentTaskSteps}
        events={agentTaskStatus.agentTaskEvents}
        intentDetection={runPlan.intentDetection}
        onAbort={() => void agentTaskStatus.abortAgentTask()}
        onOpenAudit={() => setAuditDrawerOpen(true)}
        onResume={() => void handleHarnessResume()}
        permissionPreflightSummary={runPlan.permissionPreflightSummary}
        researchState={researchState}
        runPlanSummary={runPlan.runPlanSummary}
      />

      {bubbleSelection.selected.size > 0 ? (
        <div className="flex justify-center px-3 py-1.5">
          <AiSelectionActionBar
            count={bubbleSelection.selected.size}
            onInsert={onInsertToEditor ? handleInsertToEditor : undefined}
            onCopy={handleCopySelected}
            onExport={handleExportSelected}
            onClear={bubbleSelection.clear}
          />
        </div>
      ) : null}

      <ContextScopeChips tokens={mentionTokens} onRemove={removeMentionToken} />

      <div data-testid="ai-input">
        <AiComposerContextMenu
          textareaRef={textareaRef}
          value={input}
          onValueChange={setInput}
        >
          <AiComposer
            value={input}
            streaming={streaming}
            disabled={composerDisabled}
            placeholder="输入问题，或直接说明你想查、想改、想检、想整理什么"
            textareaRef={textareaRef}
            onComposerKeyDown={handleComposerKeyDown}
            onSelect={syncMentionFromInput}
            onChange={setInput}
            onSubmit={() => void send()}
            onStop={stopStreaming}
            images={images}
            onImagesChange={setImages}
            mentionPopover={
              <AiMentionPopover
                open={mentionOpen}
                query={mentionQuery}
                candidates={mentionCandidates}
                highlight={mentionHighlight}
                onHighlight={setMentionHighlight}
                navDeltaRef={mentionNavDeltaRef}
                onSelect={selectMention}
              />
            }
          />
        </AiComposerContextMenu>
      </div>

      <ToolConfirmDialog
        request={toolConfirmRequest}
        onConfirm={handleToolConfirm}
        onClose={dismissToolConfirm}
      />
      <RuleConfirmDialog
        request={ruleConfirmRequest}
        onConfirm={handleRuleConfirm}
        onReject={closeRuleConfirm}
        onClose={closeRuleConfirm}
      />
      <AuditTrailDrawer
        open={auditDrawerOpen}
        onOpenChange={setAuditDrawerOpen}
        requestId={harnessRequestId}
      />
    </div>
  );
}
