import {
  useCallback,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";

import { mergeContextPackets } from "@/lib/ai/merge-context-packets";
import { shouldStartNewAiSession } from "@/lib/ai/session-thread";
import { resolveAssistantDisplayContent } from "@/lib/assistant-message-content";
import { patchSpansPreferSidebar } from "@/lib/assistant-patch";
import { resolveAssistantIntent } from "@/lib/assistant-routing";
import { resolveAiSceneForIntent } from "@/lib/assistant-scene";
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
  AssistantIntent,
  ContextPacket,
  ContextScope,
  ContextStatus,
  OrganizeSuggestion,
  PatchProposal,
  ResearchFocusPayload,
  TokenUsage,
  WritingEditorContext,
} from "@/types/ai";

import type { ChatLine } from "../AiMessageList";
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

interface UseAssistantTasksParams {
  appendAssistantSummary: (intent: AssistantIntent, count?: number) => void;
  appendUserMessage: (rawMessage: string) => void;
  assistantRun: AssistantRunPort;
  clearCitationMiss: () => void;
  clearTaskSurfaces: () => void;
  composerDisabled: boolean;
  contextScope: ContextScope;
  ensureAssistantStreamSlot: () => void;
  forceNewSessionRef: MutableRefObject<boolean>;
  getNoteContent: () => string;
  getParagraphText: () => string | null;
  getWritingContext: () => WritingEditorContext | null;
  input: string;
  messages: ChatLine[];
  notePath: string | null;
  panelSendActiveRef: MutableRefObject<boolean>;
  packets: ContextPacket[];
  requestIdRef: MutableRefObject<string | null>;
  researchRequestIdRef: MutableRefObject<string | null>;
  selectedPacketIds: string[];
  selectionQuoteText?: string | null;
  sessionId: number | null;
  setActionState: Dispatch<SetStateAction<AssistantActionState>>;
  setActivityHint: Dispatch<SetStateAction<string | null>>;
  setCitationResult: Dispatch<
    SetStateAction<import("@/types/ai").CitationCheckResult | null>
  >;
  setContextStatusData: Dispatch<SetStateAction<ContextStatus | null>>;
  setDocIssues: Dispatch<SetStateAction<string[]>>;
  setDocSummary: Dispatch<SetStateAction<string | null>>;
  setHarnessRequestId: Dispatch<SetStateAction<string | null>>;
  setInput: Dispatch<SetStateAction<string>>;
  setLastError: Dispatch<SetStateAction<string | null>>;
  setMessages: Dispatch<SetStateAction<ChatLine[]>>;
  setOrganizeSelection: Dispatch<SetStateAction<Set<string>>>;
  setOrganizeSuggestions: Dispatch<SetStateAction<OrganizeSuggestion[]>>;
  setPackets: Dispatch<SetStateAction<ContextPacket[]>>;
  setPacketsOpen: Dispatch<SetStateAction<boolean>>;
  setResearchPanelExpanded: Dispatch<SetStateAction<boolean>>;
  setResearchResult: Dispatch<SetStateAction<ResearchFocusPayload | null>>;
  setResearchRunning: Dispatch<SetStateAction<boolean>>;
  setSessionId: Dispatch<SetStateAction<number | null>>;
  setSessionTokenUsage: Dispatch<SetStateAction<TokenUsage | null>>;
  setStreaming: Dispatch<SetStateAction<boolean>>;
  setWritingPatches: Dispatch<SetStateAction<PatchProposal[]>>;
  streamBufRef: MutableRefObject<string>;
  webSearch: boolean;
}

interface UseAssistantTasksResult {
  runWriting: (rawMessage: string) => Promise<void>;
  send: () => Promise<void>;
}

export function useAssistantTasks({
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
  selectionQuoteText,
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
  streamBufRef,
  webSearch,
}: UseAssistantTasksParams): UseAssistantTasksResult {
  const assembleContextForChat = useCallback(
    async (query: string, intent: AssistantIntent) => {
      const scene = resolveAiSceneForIntent(intent);
      const result = await contextAssemble({
        scene,
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
      options?: { startNewSession?: boolean },
    ) => {
      setStreaming(true);
      streamBufRef.current = "";
      requestIdRef.current = null;
      setHarnessRequestId(null);
      panelSendActiveRef.current = true;
      setActionState(buildActionState(intent, "running"));
      assistantRun.setFromTaskStatus("running", intent);
      ensureAssistantStreamSlot();
      setActivityHint("正在连接模型并处理工具调用…");
      assistantRun.setActivityHint("正在连接模型并处理工具调用…");

      let completedOk = false;
      try {
        const response = await assistantExecute({
          intent,
          message: rawMessage,
          notePath,
          noteContent: getNoteContent(),
          webAuthorized: webSearch,
          contextScope,
          sessionId,
          newSession: options?.startNewSession ?? forceNewSessionRef.current,
          selectedPacketIds:
            selectedPacketIds.length > 0 ? selectedPacketIds : undefined,
        });
        forceNewSessionRef.current = false;
        if (response.kind !== "chat") {
          throw new Error("助手路由异常：期望对话结果");
        }
        const result = response.payload;
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
        setActionState(
          buildActionState(
            intent,
            pendingTools ? "awaiting_confirmation" : "completed",
          ),
        );
        assistantRun.setFromTaskStatus(
          pendingTools ? "awaiting_confirmation" : "completed",
          intent,
        );
        completedOk = !pendingTools;
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
      getNoteContent,
      notePath,
      packets,
      panelSendActiveRef,
      requestIdRef,
      selectedPacketIds,
      sessionId,
      setActionState,
      setActivityHint,
      setHarnessRequestId,
      setLastError,
      setMessages,
      setPackets,
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
      options?: { startNewSession?: boolean },
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
    async (rawMessage: string) => {
      const ctx = getWritingContext();
      if (!notePath || !ctx) {
        throw new Error("请先在编辑器中选中需要处理的内容。");
      }
      setActionState(buildActionState("writing", "running"));
      assistantRun.setFromTaskStatus("running", "writing");
      clearTaskSurfaces();
      const response = await assistantExecute({
        intent: "writing",
        message: rawMessage,
        notePath,
        noteContent: getNoteContent(),
        webAuthorized: webSearch,
        selection: ctx.selection,
        cursorContext: ctx.cursorContext,
      });
      if (response.kind !== "writing") {
        throw new Error("助手路由异常：期望写作结果");
      }
      const result = response.payload;
      const nextPatches = result.patches;
      const nextPackets = result.evidence_used;
      const useSidebarDiff = patchSpansPreferSidebar(nextPatches);
      setWritingPatches(nextPatches);
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
      getNoteContent,
      getWritingContext,
      notePath,
      setActionState,
      setPackets,
      setPacketsOpen,
      setWritingPatches,
      webSearch,
    ],
  );

  const runCitation = useCallback(async () => {
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
      intent: "citation",
      message: "检查引用",
      notePath,
      webAuthorized: webSearch,
      paragraphText: text,
      contextScope,
    });
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
  }, [
    appendAssistantSummary,
    assistantRun,
    clearTaskSurfaces,
    contextScope,
    getParagraphText,
    notePath,
    setActionState,
    setCitationResult,
    setPackets,
    setPacketsOpen,
    webSearch,
  ]);

  const runOrganize = useCallback(
    async (rawMessage: string) => {
      setActionState(buildActionState("organize", "running"));
      assistantRun.setFromTaskStatus("running", "organize");
      clearTaskSurfaces();
      const response = await assistantExecute({
        intent: "organize",
        message: rawMessage,
        webAuthorized: webSearch,
        contextScope,
        organizeTaskType: determineOrganizeTaskType(rawMessage),
      });
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
      setActionState,
      setOrganizeSelection,
      setOrganizeSuggestions,
      webSearch,
    ],
  );

  const runChapter = useCallback(
    async (rawMessage: string) => {
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
        intent: "chapter",
        message: rawMessage,
        notePath,
        noteContent: getNoteContent(),
        webAuthorized: webSearch,
        chapter,
      });
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
      getNoteContent,
      notePath,
      setActionState,
      setWritingPatches,
      webSearch,
    ],
  );

  const runDocumentCheck = useCallback(
    async (rawMessage: string) => {
      if (!notePath) {
        throw new Error("请先打开一篇笔记。");
      }
      setActionState(buildActionState("document", "running"));
      assistantRun.setFromTaskStatus("running", "document");
      clearTaskSurfaces();
      const response = await assistantExecute({
        intent: "document",
        message: rawMessage,
        notePath,
        noteContent: getNoteContent(),
        webAuthorized: webSearch,
        documentCheckType: determineDocumentCheckType(rawMessage),
      });
      if (response.kind !== "document") {
        throw new Error("助手路由异常：期望文档检查结果");
      }
      const result = response.payload;
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
    },
    [
      appendAssistantSummary,
      assistantRun,
      clearTaskSurfaces,
      getNoteContent,
      notePath,
      setActionState,
      setDocIssues,
      setDocSummary,
      setWritingPatches,
      webSearch,
    ],
  );

  const runResearch = useCallback(
    async (rawMessage: string) => {
      setActionState(buildActionState("research", "running"));
      assistantRun.setFromTaskStatus("running", "research");
      setResearchRunning(true);
      clearTaskSurfaces();
      const response = await assistantExecute({
        intent: "research",
        message: rawMessage,
        webAuthorized: webSearch,
      });
      if (response.kind !== "research") {
        throw new Error("助手路由异常：期望研究结果");
      }
      const result = response.payload;
      researchRequestIdRef.current = result.request_id;
      setResearchResult(result);
      setResearchPanelExpanded(false);
      setResearchRunning(false);
      setActionState(buildActionState("research", "completed"));
      assistantRun.setFromTaskStatus("completed", "research");
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          content: "",
          kind: "research",
          research: result,
        },
      ]);
    },
    [
      assistantRun,
      clearTaskSurfaces,
      researchRequestIdRef,
      setActionState,
      setMessages,
      setResearchPanelExpanded,
      setResearchResult,
      setResearchRunning,
      webSearch,
    ],
  );

  const send = useCallback(async () => {
    if (!input.trim() || composerDisabled) return;
    const rawMessage = input.trim();
    const intent = resolveAssistantIntent({
      message: rawMessage,
      hasSelection: Boolean(
        getWritingContext()?.selection || selectionQuoteText,
      ),
      notePath,
      explicitScope:
        contextScope.paths.length > 0 || contextScope.pathPrefixes.length > 0,
    });

    setInput("");
    setLastError(null);
    const startNewSession = shouldStartNewAiSession(
      messages,
      forceNewSessionRef.current,
    );
    clearCitationMiss();
    appendUserMessage(rawMessage);
    setActivityHint("正在理解你的问题…");

    try {
      switch (intent) {
        case "writing":
          await runWriting(rawMessage);
          break;
        case "citation":
          await runCitation();
          break;
        case "organize":
          await runOrganize(rawMessage);
          break;
        case "research":
          await runResearch(rawMessage);
          break;
        case "chapter":
          await runChapter(rawMessage);
          break;
        case "document":
          await runDocumentCheck(rawMessage);
          break;
        case "knowledge":
        case "chat":
          await runKnowledgeChat(rawMessage, intent, { startNewSession });
          break;
      }
    } catch (error) {
      const message = invokeErrorMessage(error);
      setLastError(message);
      setMessages((prev) => [
        ...prev,
        { role: "system", content: `错误: ${message}` },
      ]);
      setActionState(buildActionState(intent, "error", message));
      assistantRun.setFromTaskStatus("error", intent);
      setActivityHint(null);
    }
  }, [
    appendUserMessage,
    assistantRun,
    clearCitationMiss,
    composerDisabled,
    contextScope.pathPrefixes.length,
    contextScope.paths.length,
    forceNewSessionRef,
    getWritingContext,
    input,
    messages,
    notePath,
    runChapter,
    runCitation,
    runDocumentCheck,
    runKnowledgeChat,
    runOrganize,
    runResearch,
    runWriting,
    selectionQuoteText,
    setActionState,
    setActivityHint,
    setInput,
    setLastError,
    setMessages,
  ]);

  return { runWriting, send };
}
