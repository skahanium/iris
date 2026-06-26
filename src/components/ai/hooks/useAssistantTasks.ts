import {
  useCallback,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";

import { mergeContextPackets } from "@/lib/ai/merge-context-packets";
import { shouldStartNewAiSession } from "@/lib/ai/session-thread";
import { resolveAssistantDisplayContent } from "@/lib/assistant-message-content";
import { buildArtifactDraftsFromTaskResult } from "@/lib/assistant-artifact-tabs";
import { validateContextReference } from "@/lib/context-reference";
import { patchSpansPreferSidebar } from "@/lib/assistant-patch";
import { pendingWriteConfirmationAction } from "@/lib/assistant-write-confirmation";
import {
  agentIntentForTaskPlan,
  intentDetectionForTaskPlan,
} from "@/lib/assistant-routing";
import { legacySceneHintForAgentIntent } from "@/lib/assistant-scene";
import { buildAssistantTaskPlan } from "@/lib/assistant-taskplan";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  assistantExecute,
  contextAssemble,
  parseDocumentChapters,
} from "@/lib/ipc";
import { mapChatToolCallsForUi } from "@/lib/map-chat-tool-calls";
import { accumulateTokenUsage } from "@/lib/token-usage";
import type {
  AssistantActionState,
  AgentIntent,
  AgentRunPlanSummary,
  AssistantExecuteResponse,
  AssistantIntent,
  ContextReference,
  ContextPacket,
  ContextScope,
  ContextStatus,
  OrganizeSuggestion,
  PatchProposal,
  ResearchFocusPayload,
  ResearchState,
  TokenUsage,
  WritingState,
  WritingEditorContext,
  IntentDetectionResult,
  PermissionPreflightSummary,
  TaskPlan,
  TaskPlanIntent,
} from "@/types/ai";
import type { AssistantArtifactDraft } from "@/types/assistant-artifact";

import type { ChatLine, ImageAttachment } from "../AiMessageList";
import {
  buildActionState,
  determineDocumentCheckType,
  determineOrganizeTaskType,
} from "../unified-assistant-panel-utils";

interface AssistantRunPort {
  setActivityHint: (hint: string | null) => void;
  setEvidenceRefreshNotice: (notice: string | null) => void;
  setFromTaskStatus: (
    status: AssistantActionState["status"],
    intent: AssistantIntent,
  ) => void;
}

interface AssistantTaskRuntimePorts {
  appendAssistantSummary: (intent: AssistantIntent, count?: number) => void;
  appendUserMessage: (rawMessage: string, imgs?: ImageAttachment[]) => void;
  assistantRun: AssistantRunPort;
  clearCitationMiss: () => void;
  clearContextReferences: () => void;
  clearTaskSurfaces: () => void;
  ensureAssistantStreamSlot: () => void;
  runPlanControls: {
    setIntentDetection: Dispatch<SetStateAction<IntentDetectionResult | null>>;
    setPermissionPreflightSummary: Dispatch<
      SetStateAction<PermissionPreflightSummary | null>
    >;
    setRunPlanSummary: Dispatch<SetStateAction<AgentRunPlanSummary | null>>;
  };
}

interface AssistantTaskContext {
  composerDisabled: boolean;
  contextScope: ContextScope;
  contextReferences: ContextReference[];
  acceptWritingPatch: (patch: PatchProposal) => Promise<boolean>;
  getNoteContent: () => string;
  getParagraphText: () => string | null;
  getWritingContext: () => WritingEditorContext | null;
  input: string;
  messages: ChatLine[];
  notePath: string | null;
  packets: ContextPacket[];
  selectedPacketIds: string[];
  selectionQuoteText?: string | null;
  sessionId: number | null;
  webSearch: boolean;
  writingPatches: PatchProposal[];
}

interface AssistantTaskRefs {
  forceNewSessionRef: MutableRefObject<boolean>;
  panelSendActiveRef: MutableRefObject<boolean>;
  requestIdRef: MutableRefObject<string | null>;
  researchRequestIdRef: MutableRefObject<string | null>;
  streamBufRef: MutableRefObject<string>;
  docStreamActiveRef: MutableRefObject<boolean>;
}

interface AssistantTaskStatePorts {
  setActionState: Dispatch<SetStateAction<AssistantActionState>>;
  setActivityHint: Dispatch<SetStateAction<string | null>>;
  setAssistantArtifacts: Dispatch<SetStateAction<AssistantArtifactDraft[]>>;
  setCitationResult: Dispatch<
    SetStateAction<import("@/types/ai").CitationCheckResult | null>
  >;
  setContextStatusData: Dispatch<SetStateAction<ContextStatus | null>>;
  setCurrentTaskPlanIntent: Dispatch<SetStateAction<TaskPlanIntent | null>>;
  setDocIssues: Dispatch<SetStateAction<string[]>>;
  setDocSummary: Dispatch<SetStateAction<string | null>>;
  setAgentTaskId: Dispatch<SetStateAction<string | null>>;
  setHarnessRequestId: Dispatch<SetStateAction<string | null>>;
  setInput: Dispatch<SetStateAction<string>>;
  setLastError: Dispatch<SetStateAction<string | null>>;
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
  setOrganizeSelection: Dispatch<SetStateAction<Set<string>>>;
  setOrganizeSuggestions: Dispatch<SetStateAction<OrganizeSuggestion[]>>;
  setPackets: Dispatch<SetStateAction<ContextPacket[]>>;
  setPacketsOpen: Dispatch<SetStateAction<boolean>>;
  setPausedTaskId: Dispatch<SetStateAction<string | null>>;
  setResearchPanelExpanded: Dispatch<SetStateAction<boolean>>;
  setResearchResult: Dispatch<SetStateAction<ResearchFocusPayload | null>>;
  setResearchState: Dispatch<SetStateAction<ResearchState | null>>;
  setResearchRunning: Dispatch<SetStateAction<boolean>>;
  setSessionId: Dispatch<SetStateAction<number | null>>;
  setSessionTokenUsage: Dispatch<SetStateAction<TokenUsage | null>>;
  setStreaming: Dispatch<SetStateAction<boolean>>;
  setWritingPatches: Dispatch<SetStateAction<PatchProposal[]>>;
  setWritingState: Dispatch<SetStateAction<WritingState | null>>;
}

interface UseAssistantTasksParams {
  runtime: AssistantTaskRuntimePorts;
  context: AssistantTaskContext;
  refs: AssistantTaskRefs;
  state: AssistantTaskStatePorts;
}

interface UseAssistantTasksResult {
  runWriting: (rawMessage: string, taskPlan?: TaskPlan) => Promise<void>;
  send: () => Promise<void>;
  images: ImageAttachment[];
  setImages: Dispatch<SetStateAction<ImageAttachment[]>>;
}

function assistantIntentForTaskPlanIntent(
  planIntent: TaskPlanIntent,
): AssistantIntent {
  switch (planIntent) {
    case "ask_notes":
      return "knowledge";
    case "creative_write":
    case "rewrite_selection":
      return "writing";
    case "citation_check":
      return "citation";
    case "document_check":
      return "document";
    case "vision_chat":
    case "skill_management":
      return "chat";
    case "chat":
    case "research":
    case "organize":
    case "chapter":
      return planIntent;
  }
}

function agentIntentForAssistantIntent(intent: AssistantIntent): AgentIntent {
  switch (intent) {
    case "knowledge":
      return "ask_notes";
    case "writing":
      return "rewrite_selection";
    case "citation":
      return "citation_check";
    case "document":
      return "document_check";
    case "chat":
    case "research":
    case "organize":
    case "chapter":
      return intent;
  }
}

export function useAssistantTasks({
  runtime,
  context,
  refs,
  state,
}: UseAssistantTasksParams): UseAssistantTasksResult {
  const {
    appendAssistantSummary,
    appendUserMessage,
    assistantRun,
    clearCitationMiss,
    clearContextReferences,
    clearTaskSurfaces,
    ensureAssistantStreamSlot,
    runPlanControls,
  } = runtime;
  const {
    composerDisabled,
    contextScope,
    contextReferences,
    acceptWritingPatch,
    getNoteContent,
    getParagraphText,
    getWritingContext,
    input,
    messages,
    notePath,
    packets,
    selectedPacketIds,
    selectionQuoteText,
    sessionId,
    webSearch,
    writingPatches,
  } = context;
  const {
    forceNewSessionRef,
    panelSendActiveRef,
    requestIdRef,
    researchRequestIdRef,
    streamBufRef,
    docStreamActiveRef,
  } = refs;
  const {
    setActionState,
    setActivityHint,
    setAssistantArtifacts,
    setCitationResult,
    setContextStatusData,
    setCurrentTaskPlanIntent,
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
  } = state;
  const [images, setImages] = useState<ImageAttachment[]>([]);

  const recordRunPlan = useCallback(
    (response: AssistantExecuteResponse) => {
      runPlanControls.setIntentDetection(response.intentDetection ?? null);
      runPlanControls.setRunPlanSummary(response.runPlanSummary ?? null);
      runPlanControls.setPermissionPreflightSummary(
        response.permissionPreflightSummary ?? null,
      );
    },
    [runPlanControls],
  );

  const recordAssistantArtifacts = useCallback(
    (response: AssistantExecuteResponse) => {
      setAssistantArtifacts(buildArtifactDraftsFromTaskResult(response));
    },
    [setAssistantArtifacts],
  );

  const currentContextReferences = useCallback(
    () =>
      contextReferences.map((reference) =>
        validateContextReference(
          reference,
          reference.filePath === notePath ? getNoteContent() : null,
        ),
      ),
    [contextReferences, getNoteContent, notePath],
  );

  const getNoteContentForRequest = useCallback(
    () => (notePath ? getNoteContent() : undefined),
    [getNoteContent, notePath],
  );

  const explicitIntentDetection = useCallback(
    (
      detectedIntent: AgentIntent,
      reason: string,
      extraHints: string[] = [],
      alternatives: AgentIntent[] = ["chat"],
      confidence = 0.95,
    ): IntentDetectionResult => {
      const sourceHints = [...extraHints];
      if (notePath) sourceHints.push("context:note");
      if (
        contextScope.paths.length > 0 ||
        contextScope.pathPrefixes.length > 0
      ) {
        sourceHints.push("context:scope");
      }
      if (webSearch) sourceHints.push("context:web");
      return {
        detectedIntent,
        confidence,
        reason,
        alternatives,
        fallbackBehavior:
          "Use the compatible chat path and suggest safe next actions if this explicit task cannot run.",
        sourceHints,
      };
    },
    [
      contextScope.pathPrefixes.length,
      contextScope.paths.length,
      notePath,
      webSearch,
    ],
  );

  const assembleContextForChat = useCallback(
    async (query: string, intent: AssistantIntent) => {
      const agentIntent = agentIntentForAssistantIntent(intent);
      const scene = legacySceneHintForAgentIntent(agentIntent);
      const result = await contextAssemble({
        scene,
        agent_intent: agentIntent,
        note_path: notePath,
        note_content_hash: null,
        query,
        session_id: sessionId,
        context_scope: contextScope,
        web_search: webSearch,
      });
      const preview = result.provisional !== false;
      setPackets(
        result.packets.map((p) => ({
          ...p,
          provisional: preview,
        })),
      );
      setContextStatusData(result.context_status);
      return result;
    },
    [
      contextScope,
      notePath,
      sessionId,
      setContextStatusData,
      setPackets,
      webSearch,
    ],
  );

  const executeKnowledgeChat = useCallback(
    async (
      rawMessage: string,
      intent: AssistantIntent,
      options?: {
        startNewSession?: boolean;
        agentIntent?: AgentIntent;
        intentDetection?: IntentDetectionResult;
        images?: ImageAttachment[];
        taskPlan?: TaskPlan;
      },
    ) => {
      setStreaming(true);
      streamBufRef.current = "";
      requestIdRef.current = null;
      setAgentTaskId(null);
      setHarnessRequestId(null);
      setPausedTaskId(null);
      panelSendActiveRef.current = true;
      setActionState(buildActionState(intent, "running"));
      assistantRun.setFromTaskStatus("running", intent);
      ensureAssistantStreamSlot();
      setActivityHint("正在连接模型并处理工具调用…");
      assistantRun.setActivityHint("正在连接模型并处理工具调用…");

      let completedOk = false;
      try {
        const agentIntent =
          options?.agentIntent ??
          (intent === "knowledge" ? "ask_notes" : "chat");
        const response = await assistantExecute({
          agentIntent,
          intent,
          intentDetection:
            options?.intentDetection ??
            explicitIntentDetection(
              agentIntent,
              "Conversation entry resolved to the compatible assistant chat route.",
              intent === "knowledge"
                ? ["ui_action:ask_notes"]
                : ["ui_action:chat"],
              intent === "knowledge"
                ? ["chat", "research"]
                : ["ask_notes", "write"],
              intent === "knowledge" ? 0.78 : 0.72,
            ),
          message: rawMessage,
          contextReferences: currentContextReferences(),
          taskPlan: options?.taskPlan,
          images: options?.images,
          notePath,
          noteContent: getNoteContentForRequest(),
          webAuthorized: webSearch,
          contextScope,
          sessionId,
          newSession: options?.startNewSession ?? forceNewSessionRef.current,
          selectedPacketIds:
            selectedPacketIds.length > 0 ? selectedPacketIds : undefined,
        });
        recordRunPlan(response);
        recordAssistantArtifacts(response);
        forceNewSessionRef.current = false;
        if (response.kind !== "chat") {
          throw new Error("助手路由异常：期望对话结果");
        }
        const result = response.payload;
        const taskId = response.taskId ?? result.task_id ?? null;
        setAgentTaskId(taskId);
        const refreshNotice =
          response.evidenceRefreshNotice ?? result.evidence_refresh_notice;
        if (refreshNotice) {
          assistantRun.setEvidenceRefreshNotice(refreshNotice);
          setMessages((prev) => [
            ...prev,
            { role: "system", content: refreshNotice },
          ]);
        }
        requestIdRef.current = result.request_id;
        setHarnessRequestId(result.request_id);
        setSessionId(result.session_id);
        if (result.usage) {
          setSessionTokenUsage((prev) =>
            accumulateTokenUsage(prev, result.usage!),
          );
        }
        const toolCalls = mapChatToolCallsForUi(
          result.tool_calls,
          result.tool_results,
        );
        const serverContent = result.content?.trim() ?? "";
        const finalContent = resolveAssistantDisplayContent(
          serverContent,
          streamBufRef.current,
          toolCalls,
        );

        const evidencePackets = mergeContextPackets(
          packets,
          result.evidence_packets,
        ).map((p) => ({ ...p, provisional: false }));
        setPackets(evidencePackets);

        setMessages((prev) => {
          const next = [...prev];
          const last = next[next.length - 1];
          if (last?.role === "assistant") {
            next[next.length - 1] = {
              ...last,
              content: finalContent,
              toolCalls,
            };
          } else {
            next.push({
              role: "assistant",
              content: finalContent,
              toolCalls,
            });
          }
          return next;
        });
        const pendingTools =
          result.status === "pending_tools" ||
          toolCalls?.some((t) => t.status === "pending") === true;
        const pausedBudget = result.status === "paused_budget";
        setPausedTaskId(pausedBudget ? taskId : null);
        setActionState(
          buildActionState(
            intent,
            pendingTools
              ? "awaiting_confirmation"
              : pausedBudget
                ? "paused_budget"
                : "completed",
          ),
        );
        assistantRun.setFromTaskStatus(
          pendingTools
            ? "awaiting_confirmation"
            : pausedBudget
              ? "paused_budget"
              : "completed",
          intent,
        );
        completedOk = !pendingTools && !pausedBudget;
      } catch (error) {
        const message = invokeErrorMessage(error);
        setLastError(message);
        setMessages((prev) => [
          ...prev,
          { role: "system", content: `错误: ${message}` },
        ]);
        setActionState(buildActionState(intent, "error", message));
        assistantRun.setFromTaskStatus("error", intent);
      } finally {
        panelSendActiveRef.current = false;
        setStreaming(false);
        setActivityHint(null);
        assistantRun.setActivityHint(null);
        if (completedOk) {
          requestIdRef.current = null;
          setHarnessRequestId(null);
        }
        streamBufRef.current = "";
      }
    },
    [
      assistantRun,
      contextScope,
      ensureAssistantStreamSlot,
      forceNewSessionRef,
      explicitIntentDetection,
      getNoteContentForRequest,
      notePath,
      packets,
      panelSendActiveRef,
      recordRunPlan,
      recordAssistantArtifacts,
      currentContextReferences,
      requestIdRef,
      selectedPacketIds,
      sessionId,
      setActionState,
      setActivityHint,
      setAgentTaskId,
      setHarnessRequestId,
      setLastError,
      setMessages,
      setPackets,
      setPausedTaskId,
      setSessionId,
      setSessionTokenUsage,
      setStreaming,
      streamBufRef,
      webSearch,
    ],
  );

  const runKnowledgeChat = useCallback(
    async (
      rawMessage: string,
      intent: AssistantIntent,
      options?: {
        startNewSession?: boolean;
        agentIntent?: AgentIntent;
        intentDetection?: IntentDetectionResult;
        images?: ImageAttachment[];
        taskPlan?: TaskPlan;
      },
    ) => {
      clearTaskSurfaces();
      setLastError(null);
      setActionState(buildActionState(intent, "running"));
      setActivityHint("正在检索知识库与本地笔记…");

      try {
        await assembleContextForChat(rawMessage, intent);
        await executeKnowledgeChat(rawMessage, intent, options);
      } catch (error) {
        const message = invokeErrorMessage(error);
        setLastError(message);
        setMessages((prev) => [
          ...prev,
          { role: "system", content: `错误: ${message}` },
        ]);
        setActionState(buildActionState(intent, "error", message));
        setActivityHint(null);
      }
    },
    [
      assembleContextForChat,
      clearTaskSurfaces,
      executeKnowledgeChat,
      setActionState,
      setActivityHint,
      setLastError,
      setMessages,
    ],
  );

  const runWriting = useCallback(
    async (rawMessage: string, taskPlan?: TaskPlan) => {
      const ctx = getWritingContext();
      if (!notePath || !ctx) {
        throw new Error("请先在编辑器中选中需要处理的内容。");
      }
      setActionState(buildActionState("writing", "running"));
      assistantRun.setFromTaskStatus("running", "writing");
      clearTaskSurfaces();
      const response = await assistantExecute({
        agentIntent: "rewrite_selection",
        intent: "writing",
        intentDetection: explicitIntentDetection(
          "rewrite_selection",
          "Inline writing action explicitly requested a selected-text rewrite.",
          ["ui_action:rewrite", "context:selection"],
          ["write", "chat"],
        ),
        message: rawMessage,
        contextReferences: currentContextReferences(),
        taskPlan,
        notePath,
        noteContent: getNoteContentForRequest(),
        webAuthorized: webSearch,
        selection: ctx.selection,
        cursorContext: ctx.cursorContext,
      });
      recordRunPlan(response);
      recordAssistantArtifacts(response);
      setAgentTaskId(response.taskId ?? null);
      if (response.kind !== "writing") {
        throw new Error("助手路由异常：期望写作结果");
      }
      const result = response.payload;
      const nextPatches = result.patches;
      const nextPackets = result.evidence_used;
      const useSidebarDiff = patchSpansPreferSidebar(nextPatches);
      setWritingPatches(nextPatches);
      setWritingState(result.writing_state ?? null);
      setPackets(nextPackets);
      setPacketsOpen(nextPackets.length > 0);
      setActionState({
        ...buildActionState(
          "writing",
          nextPatches.length > 0 ? "awaiting_confirmation" : "completed",
        ),
        surface: useSidebarDiff ? "diff_review" : "inline_suggestion",
      });
      assistantRun.setFromTaskStatus(
        nextPatches.length > 0 ? "awaiting_confirmation" : "completed",
        "writing",
      );
      appendAssistantSummary("writing", nextPatches.length);
    },
    [
      appendAssistantSummary,
      assistantRun,
      clearTaskSurfaces,
      getNoteContentForRequest,
      getWritingContext,
      notePath,
      explicitIntentDetection,
      recordRunPlan,
      recordAssistantArtifacts,
      currentContextReferences,
      setActionState,
      setAgentTaskId,
      setPackets,
      setPacketsOpen,
      setWritingPatches,
      setWritingState,
      webSearch,
    ],
  );

  const runCitation = useCallback(
    async (rawMessage = "检查引用", taskPlan?: TaskPlan) => {
      if (!notePath) {
        throw new Error("请先打开一篇笔记。");
      }
      const text = getParagraphText();
      if (!text?.trim()) {
        throw new Error("请先在编辑器中选中要检查引用的段落。");
      }
      setActionState(buildActionState("citation", "running"));
      assistantRun.setFromTaskStatus("running", "citation");
      clearTaskSurfaces();
      const response = await assistantExecute({
        agentIntent: "citation_check",
        intent: "citation",
        intentDetection: explicitIntentDetection(
          "citation_check",
          "Citation command explicitly requested claim and evidence checking.",
          ["ui_action:citation_check", "context:selection"],
          ["ask_notes", "research"],
        ),
        message: rawMessage,
        contextReferences: currentContextReferences(),
        taskPlan,
        notePath,
        webAuthorized: webSearch,
        paragraphText: text,
        contextScope,
      });
      recordRunPlan(response);
      recordAssistantArtifacts(response);
      setAgentTaskId(response.taskId ?? null);
      if (response.kind !== "citation") {
        throw new Error("助手路由异常：期望引用检查结果");
      }
      const result = response.payload;
      setCitationResult(result);
      setPackets(result.evidence_used);
      setPacketsOpen(result.evidence_used.length > 0);
      setActionState(buildActionState("citation", "completed"));
      assistantRun.setFromTaskStatus("completed", "citation");
      appendAssistantSummary("citation");
    },
    [
      appendAssistantSummary,
      assistantRun,
      clearTaskSurfaces,
      contextScope,
      explicitIntentDetection,
      getParagraphText,
      notePath,
      recordRunPlan,
      recordAssistantArtifacts,
      currentContextReferences,
      setActionState,
      setAgentTaskId,
      setCitationResult,
      setPackets,
      setPacketsOpen,
      webSearch,
    ],
  );

  const runOrganize = useCallback(
    async (rawMessage: string, taskPlan?: TaskPlan) => {
      setActionState(buildActionState("organize", "running"));
      assistantRun.setFromTaskStatus("running", "organize");
      clearTaskSurfaces();
      const response = await assistantExecute({
        agentIntent: "organize",
        intent: "organize",
        intentDetection: explicitIntentDetection(
          "organize",
          "Organize action explicitly requested note or vault organization.",
          ["ui_action:organize"],
          ["ask_notes", "chat"],
        ),
        message: rawMessage,
        contextReferences: currentContextReferences(),
        taskPlan,
        webAuthorized: webSearch,
        contextScope,
        organizeTaskType: determineOrganizeTaskType(rawMessage),
      });
      recordRunPlan(response);
      recordAssistantArtifacts(response);
      setAgentTaskId(response.taskId ?? null);
      if (response.kind !== "organize") {
        throw new Error("助手路由异常：期望整理结果");
      }
      const suggestions = response.payload.batch.suggestions;
      setOrganizeSuggestions(suggestions);
      setOrganizeSelection(new Set(suggestions.map((item) => item.id)));
      setActionState(
        buildActionState(
          "organize",
          suggestions.length > 0 ? "awaiting_confirmation" : "completed",
        ),
      );
      assistantRun.setFromTaskStatus(
        suggestions.length > 0 ? "awaiting_confirmation" : "completed",
        "organize",
      );
      appendAssistantSummary("organize", suggestions.length);
    },
    [
      appendAssistantSummary,
      assistantRun,
      clearTaskSurfaces,
      contextScope,
      explicitIntentDetection,
      recordRunPlan,
      recordAssistantArtifacts,
      currentContextReferences,
      setActionState,
      setAgentTaskId,
      setOrganizeSelection,
      setOrganizeSuggestions,
      webSearch,
    ],
  );

  const runChapter = useCallback(
    async (rawMessage: string, taskPlan?: TaskPlan) => {
      if (!notePath) {
        throw new Error("请先打开一篇笔记。");
      }
      const chapters = await parseDocumentChapters(getNoteContent());
      const chapter = chapters[0];
      if (!chapter) {
        throw new Error("当前文档没有可识别的章节结构。");
      }
      setActionState(buildActionState("chapter", "running"));
      assistantRun.setFromTaskStatus("running", "chapter");
      clearTaskSurfaces();
      const response = await assistantExecute({
        agentIntent: "chapter",
        intent: "chapter",
        intentDetection: explicitIntentDetection(
          "chapter",
          "Chapter action explicitly requested chapter-level writing.",
          ["ui_action:chapter", "context:note"],
          ["write", "document_check"],
        ),
        message: rawMessage,
        contextReferences: currentContextReferences(),
        taskPlan,
        notePath,
        noteContent: getNoteContentForRequest(),
        webAuthorized: webSearch,
        chapter,
      });
      recordRunPlan(response);
      recordAssistantArtifacts(response);
      setAgentTaskId(response.taskId ?? null);
      if (response.kind !== "chapter") {
        throw new Error("助手路由异常：期望章节写作结果");
      }
      const nextPatches = response.payload.patches;
      setWritingPatches(nextPatches);
      setActionState(
        buildActionState(
          "chapter",
          nextPatches.length > 0 ? "awaiting_confirmation" : "completed",
        ),
      );
      assistantRun.setFromTaskStatus(
        nextPatches.length > 0 ? "awaiting_confirmation" : "completed",
        "chapter",
      );
      appendAssistantSummary("chapter", nextPatches.length);
    },
    [
      appendAssistantSummary,
      assistantRun,
      clearTaskSurfaces,
      explicitIntentDetection,
      getNoteContent,
      getNoteContentForRequest,
      notePath,
      recordRunPlan,
      recordAssistantArtifacts,
      currentContextReferences,
      setActionState,
      setAgentTaskId,
      setWritingPatches,
      webSearch,
    ],
  );

  const runDocumentCheck = useCallback(
    async (rawMessage: string, taskPlan?: TaskPlan) => {
      if (!notePath) {
        throw new Error("请先打开一篇笔记。");
      }
      setActionState(buildActionState("document", "running"));
      assistantRun.setFromTaskStatus("running", "document");
      clearTaskSurfaces();
      // Activate the doc-summary stream so useDocSummaryStream renders the
      // analysis_summary tokens incrementally into the doc panel. Uses a
      // dedicated ref so tokens do not leak into the chat message list.
      docStreamActiveRef.current = true;
      requestIdRef.current = null;
      try {
        const response = await assistantExecute({
          agentIntent: "document_check",
          intent: "document",
          intentDetection: explicitIntentDetection(
            "document_check",
            "Document action explicitly requested whole-note checking.",
            ["ui_action:document_check", "context:note"],
            ["chapter", "write"],
          ),
          message: rawMessage,
          contextReferences: currentContextReferences(),
          taskPlan,
          notePath,
          noteContent: getNoteContentForRequest(),
          webAuthorized: webSearch,
          documentCheckType: determineDocumentCheckType(rawMessage),
        });
        recordRunPlan(response);
        recordAssistantArtifacts(response);
        setAgentTaskId(response.taskId ?? null);
        if (response.kind !== "document") {
          throw new Error("助手路由异常：期望文档检查结果");
        }
        const result = response.payload;
        // The authoritative summary wins over any streamed snapshot.
        setDocSummary(result.analysis_summary ?? null);
        const issues: string[] = [];
        if (result.outline_result) {
          for (const issue of result.outline_result.issues) {
            issues.push(`[大纲] ${issue.description}`);
          }
        }
        if (result.citation_gap_result) {
          for (const claim of result.citation_gap_result.uncited_claims) {
            issues.push(`[引用缺口] ${claim.statement}`);
          }
        }
        if (result.style_result) {
          for (const item of result.style_result.inconsistencies) {
            issues.push(`[风格] ${item.description}`);
          }
        }
        setDocIssues(issues);
        const nextPatches = result.patches ?? [];
        setWritingPatches(nextPatches);
        setActionState(
          buildActionState(
            "document",
            nextPatches.length > 0 ? "awaiting_confirmation" : "completed",
          ),
        );
        assistantRun.setFromTaskStatus(
          nextPatches.length > 0 ? "awaiting_confirmation" : "completed",
          "document",
        );
        appendAssistantSummary("document", nextPatches.length);
      } finally {
        docStreamActiveRef.current = false;
        requestIdRef.current = null;
      }
    },
    [
      appendAssistantSummary,
      assistantRun,
      clearTaskSurfaces,
      docStreamActiveRef,
      explicitIntentDetection,
      getNoteContentForRequest,
      notePath,
      recordRunPlan,
      recordAssistantArtifacts,
      currentContextReferences,
      requestIdRef,
      setActionState,
      setAgentTaskId,
      setDocIssues,
      setDocSummary,
      setWritingPatches,
      webSearch,
    ],
  );

  const runResearch = useCallback(
    async (rawMessage: string, taskPlan?: TaskPlan) => {
      setActionState(buildActionState("research", "running"));
      assistantRun.setFromTaskStatus("running", "research");
      setResearchRunning(true);
      clearTaskSurfaces();
      // Activate the chat stream slot so useAssistantLlmStream renders the
      // synthesize_summary tokens incrementally instead of dumping the whole
      // summary when the IPC call resolves.
      setStreaming(true);
      panelSendActiveRef.current = true;
      streamBufRef.current = "";
      requestIdRef.current = null;
      ensureAssistantStreamSlot();
      let completedOk = false;
      try {
        const response = await assistantExecute({
          agentIntent: "research",
          intent: "research",
          intentDetection: explicitIntentDetection(
            "research",
            "Research action explicitly requested multi-evidence synthesis.",
            ["ui_action:research"],
            ["ask_notes", "chat"],
          ),
          message: rawMessage,
          contextReferences: currentContextReferences(),
          taskPlan,
          webAuthorized: webSearch,
        });
        recordRunPlan(response);
        recordAssistantArtifacts(response);
        setAgentTaskId(response.taskId ?? null);
        if (response.kind !== "research") {
          throw new Error("助手路由异常：期望研究结果");
        }
        const result = response.payload;
        const serverSummary =
          result.summary.trim() ||
          "研究已完成，但没有生成可展示正文。可在来源详情中查看证据状态。";
        researchRequestIdRef.current = result.request_id;
        setResearchResult(result);
        setResearchState(result.research_state ?? null);
        setResearchPanelExpanded(false);
        setResearchRunning(false);
        setActionState(buildActionState("research", "completed"));
        assistantRun.setFromTaskStatus("completed", "research");
        // The streamed tokens already populated the assistant slot; reconcile
        // it with the authoritative server summary rather than appending a
        // duplicate message.
        const finalContent = resolveAssistantDisplayContent(
          serverSummary,
          streamBufRef.current,
          undefined,
        );
        setMessages((prev) => {
          const next = [...prev];
          const last = next[next.length - 1];
          if (last?.role === "assistant") {
            next[next.length - 1] = { ...last, content: finalContent };
          } else {
            next.push({ role: "assistant", content: finalContent });
          }
          return next;
        });
        completedOk = true;
      } finally {
        panelSendActiveRef.current = false;
        setStreaming(false);
        streamBufRef.current = "";
        if (completedOk) {
          requestIdRef.current = null;
        }
      }
    },
    [
      assistantRun,
      clearTaskSurfaces,
      ensureAssistantStreamSlot,
      explicitIntentDetection,
      researchRequestIdRef,
      recordRunPlan,
      recordAssistantArtifacts,
      currentContextReferences,
      panelSendActiveRef,
      requestIdRef,
      setActionState,
      setAgentTaskId,
      setMessages,
      setResearchPanelExpanded,
      setResearchResult,
      setResearchState,
      setResearchRunning,
      setStreaming,
      streamBufRef,
      webSearch,
    ],
  );

  const send = useCallback(async () => {
    if ((!input.trim() && images.length === 0) || composerDisabled) return;
    const rawMessage = input.trim();
    const currentImages = images;
    const pendingWriteAction = pendingWriteConfirmationAction({
      message: rawMessage,
      pendingPatchCount: writingPatches.length,
    });

    if (pendingWriteAction !== "none") {
      setInput("");
      setImages([]);
      setLastError(null);
      clearCitationMiss();
      appendUserMessage(rawMessage, currentImages);
      setCurrentTaskPlanIntent("rewrite_selection");

      if (pendingWriteAction === "clarify_multiple_patches") {
        setActionState(buildActionState("writing", "awaiting_confirmation"));
        assistantRun.setFromTaskStatus("awaiting_confirmation", "writing");
        setMessages((prev) => [
          ...prev,
          {
            role: "assistant",
            content:
              "当前有多条待确认修改。请在写作修改面板中逐条接受或拒绝，以避免多条补丁互相覆盖。",
          },
        ]);
        setActivityHint(null);
        return;
      }

      const [patch] = writingPatches;
      if (!patch) {
        setMessages((prev) => [
          ...prev,
          {
            role: "assistant",
            content: "当前没有待确认的文档修改。",
          },
        ]);
        setActivityHint(null);
        return;
      }

      setActionState(buildActionState("writing", "running"));
      assistantRun.setFromTaskStatus("running", "writing");
      setActivityHint("正在应用已确认的文档修改…");
      const applied = await acceptWritingPatch(patch);
      setMessages((prev) => [
        ...prev,
        {
          role: applied ? "assistant" : "system",
          content: applied
            ? "已应用这条文档修改。"
            : "错误: 文档修改应用失败，请查看上方错误提示后重试。",
        },
      ]);
      setActionState(
        buildActionState("writing", applied ? "completed" : "error"),
      );
      assistantRun.setFromTaskStatus(
        applied ? "completed" : "error",
        "writing",
      );
      setActivityHint(null);
      return;
    }

    const activeContextReferences = currentContextReferences();
    const hasSelection = Boolean(
      getWritingContext()?.selection ||
      selectionQuoteText ||
      activeContextReferences.some(
        (reference) => reference.kind === "selection",
      ),
    );
    const taskPlan = buildAssistantTaskPlan({
      message: rawMessage,
      contextReferences: activeContextReferences,
      hasImage: images.length > 0,
      hasSelection,
      notePath,
      explicitScope:
        contextScope.paths.length > 0 || contextScope.pathPrefixes.length > 0,
      webAuthorized: webSearch,
    });
    const agentIntent = agentIntentForTaskPlan(taskPlan);
    const intentDetection = intentDetectionForTaskPlan(taskPlan);
    const intent = assistantIntentForTaskPlanIntent(taskPlan.intent);
    setCurrentTaskPlanIntent(taskPlan.intent);

    setInput("");
    setImages([]);
    setLastError(null);
    const startNewSession = shouldStartNewAiSession(
      messages,
      forceNewSessionRef.current,
    );
    clearCitationMiss();
    appendUserMessage(rawMessage, currentImages);
    setActivityHint("正在理解你的问题…");

    try {
      if (taskPlan.requiresClarification) {
        clearTaskSurfaces();
        runPlanControls.setIntentDetection(null);
        runPlanControls.setRunPlanSummary(null);
        runPlanControls.setPermissionPreflightSummary(null);
        const question =
          taskPlan.clarificationQuestion ??
          "你希望我先做哪一种处理：普通回答、写作，还是研究？";
        setMessages((prev) => [
          ...prev,
          {
            role: "assistant",
            content: question,
          },
        ]);
        setActionState(buildActionState("chat", "completed"));
        assistantRun.setFromTaskStatus("completed", "chat");
        setActivityHint(null);
        clearContextReferences();
        return;
      }

      switch (taskPlan.intent) {
        case "rewrite_selection":
          await runWriting(rawMessage, taskPlan);
          break;
        case "creative_write":
          await runKnowledgeChat(rawMessage, "chat", {
            startNewSession,
            agentIntent,
            intentDetection,
            images: currentImages.length > 0 ? currentImages : undefined,
            taskPlan,
          });
          break;
        case "citation_check":
          await runCitation(rawMessage, taskPlan);
          break;
        case "organize":
          await runOrganize(rawMessage, taskPlan);
          break;
        case "research":
          await runResearch(rawMessage, taskPlan);
          break;
        case "chapter":
          await runChapter(rawMessage, taskPlan);
          break;
        case "document_check":
          await runDocumentCheck(rawMessage, taskPlan);
          break;
        case "ask_notes":
        case "chat":
        case "vision_chat":
        case "skill_management":
          await runKnowledgeChat(
            rawMessage,
            assistantIntentForTaskPlanIntent(taskPlan.intent),
            {
              startNewSession,
              agentIntent,
              intentDetection,
              images: currentImages.length > 0 ? currentImages : undefined,
              taskPlan,
            },
          );
          break;
      }
      clearContextReferences();
    } catch (error) {
      const message = invokeErrorMessage(error);
      setLastError(message);
      setMessages((prev) => [
        ...prev,
        { role: "system", content: `错误: ${message}` },
      ]);
      setActionState(buildActionState(intent, "error", message));
      assistantRun.setFromTaskStatus("error", intent);
    } finally {
      panelSendActiveRef.current = false;
      setStreaming(false);
      setActivityHint(null);
      assistantRun.setActivityHint(null);
    }
  }, [
    acceptWritingPatch,
    appendUserMessage,
    assistantRun,
    clearCitationMiss,
    clearContextReferences,
    clearTaskSurfaces,
    composerDisabled,
    contextScope.pathPrefixes.length,
    contextScope.paths.length,
    currentContextReferences,
    forceNewSessionRef,
    getWritingContext,
    images,
    input,
    messages,
    notePath,
    panelSendActiveRef,
    runChapter,
    runCitation,
    runDocumentCheck,
    runKnowledgeChat,
    runOrganize,
    runResearch,
    runWriting,
    runPlanControls,
    selectionQuoteText,
    setActionState,
    setActivityHint,
    setCurrentTaskPlanIntent,
    setImages,
    setInput,
    setLastError,
    setMessages,
    setStreaming,
    webSearch,
    writingPatches,
  ]);

  return { runWriting, send, images, setImages };
}
