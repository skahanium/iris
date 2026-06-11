import { AlertTriangle } from "lucide-react";
import { useCallback, useRef, useState } from "react";

import { AssistantPanelHeader } from "@/components/ai/AssistantPanelHeader";
import { AuditTrailDrawer } from "@/components/ai/AuditTrailDrawer";
import { AiComposer } from "@/components/ui/ai-composer";
import { Button } from "@/components/ui/button";
import { usePromptProfile } from "@/hooks/usePromptProfile";
import { useAssistantLlmStream } from "@/hooks/useAssistantLlmStream";
import { resolveAiSceneForIntent } from "@/lib/assistant-scene";
import type { AiScene } from "@/types/ai";
import { harnessAbort } from "@/lib/ipc";
import type {
  AssistantActionState,
  ContextPacket,
  ContextStatus,
  WritingEditorContext,
} from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";

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
import { useResearchControl } from "./hooks/useResearchControl";
import { ContextScopeChips } from "./ContextScopeChips";
import { AssistantTaskSurfaces } from "./AssistantTaskSurfaces";
import { RuleConfirmDialog } from "./RuleConfirmDialog";
import { ToolConfirmDialog } from "./ToolConfirmDialog";

export interface AssistantSelectionQuote {
  filePath: string;
  text: string;
}

export interface UnifiedAssistantPanelProps {
  notePath: string | null;
  noteDisplayTitle: string | null;
  getNoteContent: () => string;
  webSearch?: boolean;
  getWritingContext: () => WritingEditorContext | null;
  getParagraphText: () => string | null;
  onPatchApplied?: (newContent: string) => void;
  onVaultRefresh?: () => void;
  onInsertToEditor?: (content: string) => void;
  selectionQuote?: AssistantSelectionQuote | null;
  prefillMessage?: string | null;
  onChromeChange?: (snapshot: AssistantChromeSnapshot) => void;
}

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
  const [auditDrawerOpen, setAuditDrawerOpen] = useState(false);
  const assistantRun = useAssistantRun("chat");
  const chromeActionsDisabled = streaming;
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
    setCitationResult,
    setDocIssues,
    setDocSummary,
    setLastError,
    setOrganizeSelection,
    setOrganizeSuggestions,
    setResearchResult,
    setWritingPatches,
    writingPatches,
  } = useAssistantArtifacts({
    getNoteContent,
    onPatchApplied,
    onVaultRefresh,
  });

  const clearTaskSurfaces = useCallback(() => {
    clearArtifactSurfaces();
    clearResearchProgressRef.current?.();
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
    handleExpandResearchDetail,
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
    setActivityHint,
    setLastError,
    setMessages,
    setPackets,
    setSessionTokenUsage,
    setStreaming,
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

  const { runWriting, send } = useAssistantTasks({
    appendAssistantSummary,
    appendUserMessage,
    assistantRun,
    clearCitationMiss,
    clearTaskSurfaces,
    composerDisabled,
    contextScope,
    ensureAssistantStreamSlot,
    forceNewSessionRef,
    getNoteContent,
    getParagraphText,
    getWritingContext,
    input,
    messages,
    notePath,
    packets,
    panelSendActiveRef,
    requestIdRef,
    researchRequestIdRef,
    selectedPacketIds,
    selectionQuoteText: selectionQuote?.text,
    sessionId,
    setActionState,
    setActivityHint,
    setCitationResult,
    setContextStatusData,
    setDocIssues,
    setDocSummary,
    setHarnessRequestId,
    setInput,
    setLastError,
    setMessages,
    setOrganizeSelection,
    setOrganizeSuggestions,
    setPackets,
    setPacketsOpen,
    setResearchPanelExpanded,
    setResearchResult,
    setResearchRunning,
    setSessionId,
    setSessionTokenUsage,
    setStreaming,
    setWritingPatches,
    streamBufRef: streamBuf,
    webSearch,
  });

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

  const activeScene: AiScene = resolveAiSceneForIntent(actionState.intent);

  return (
    <div
      className="ai-sidecar flex h-full flex-col bg-ai-workspace"
      data-testid="unified-assistant-panel"
    >
      <AssistantPanelHeader
        activeScene={activeScene}
        chromeActionsDisabled={chromeActionsDisabled}
        currentSessionId={sessionId}
        harnessRequestId={harnessRequestId}
        notePath={notePath}
        onDeletedCurrentSession={handleNewChat}
        onNewChat={handleNewChat}
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

      {lastError ? (
        <div className="space-y-2 px-3 pt-3">
          <div className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
            <span>{lastError}</span>
          </div>
          {harnessRequestId ? (
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 text-xs"
              disabled={chromeActionsDisabled}
              onClick={() => void handleHarnessResume()}
            >
              从 checkpoint 恢复 Agent
            </Button>
          ) : null}
        </div>
      ) : null}

      <AssistantTaskSurfaces
        researchProgress={researchProgress}
        researchRunning={researchRunning}
        onAbortResearch={() => void abortResearch()}
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
        onExpandResearch={handleExpandResearchDetail}
        onRetract={handleRetract}
        onSelect={bubbleSelection.handleClick}
        onQuoteToInput={handleQuoteToInput}
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
