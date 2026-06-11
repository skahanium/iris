import { AlertTriangle, MessageSquarePlus } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { AgentStatusBadge } from "@/components/ai/AgentStatusBadge";
import { AssistantPersonaDisplay } from "@/components/ai/AssistantPersonaDisplay";
import { AuditTrailDrawer } from "@/components/ai/AuditTrailDrawer";
import { AiComposer } from "@/components/ui/ai-composer";
import { Button } from "@/components/ui/button";
import { usePromptProfile } from "@/hooks/usePromptProfile";
import { useAssistantLlmStream } from "@/hooks/useAssistantLlmStream";
import { patchSpansPreferSidebar } from "@/lib/assistant-patch";
import { resolveAssistantIntent } from "@/lib/assistant-routing";
import {
  resolveAiSceneForIntent,
  syncActiveAiScene,
} from "@/lib/assistant-scene";
import type { AiScene } from "@/types/ai";
import { stripMentionTokensForDisplay } from "@/lib/ai-context-scope";
import { mergeContextPackets } from "@/lib/ai/merge-context-packets";
import { shouldStartNewAiSession } from "@/lib/ai/session-thread";
import { resolveAssistantDisplayContent } from "@/lib/assistant-message-content";
import { OPEN_AUDIT_TRAIL_EVENT } from "@/lib/audit-trail-events";
import { buildAssistantChromeSnapshot } from "@/lib/assistant-chrome";
import { mapChatToolCallsForUi } from "@/lib/map-chat-tool-calls";
import { skillInstallSuccessNotice } from "@/lib/skill-install-notice";
import { accumulateTokenUsage } from "@/lib/token-usage";
import { invokeErrorMessage } from "@/lib/credentials";
import {
  assistantExecute,
  contextAssemble,
  harnessAbort,
  organizeApply,
  parseDocumentChapters,
  patchApply,
  profileSetRule,
  harnessResume,
  researchAbort,
  researchGenerateNote,
  listenToolConfirmRequest,
  listenResearchProgress,
  toolConfirm as toolConfirmIpc,
} from "@/lib/ipc";
import type {
  AssistantActionState,
  AssistantIntent,
  AssistantTaskStatus,
  CitationCheckResult,
  ContextPacket,
  ContextStatus,
  OrganizeSuggestion,
  PatchProposal,
  ResearchFocusPayload,
  TokenUsage,
  WritingEditorContext,
} from "@/types/ai";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";

import {
  buildActionState,
  buildTaskSummary,
  determineDocumentCheckType,
  determineOrganizeTaskType,
} from "./unified-assistant-panel-utils";
import { AiMentionPopover } from "./AiMentionPopover";
import { AiComposerContextMenu } from "./AiComposerContextMenu";
import { type ChatLine } from "./AiMessageList";
import { ConversationSurface } from "./ConversationSurface";
import { AiSelectionActionBar } from "./AiSelectionActionBar";
import { useCitationClick } from "./hooks/useCitationClick";
import { ContextPacketDrawer } from "./ContextPacketDrawer";
import { SessionHistoryDropdown } from "./SessionHistoryDropdown";
import { useAiBubbleSelection } from "@/hooks/useAiBubbleSelection";
import { useAssistantRun } from "@/hooks/useAssistantRun";
import { listenAiRequestStarted, sessionRetract, llmAbort } from "@/lib/ipc";
import { useAssistantContextScope } from "./hooks/useAssistantContextScope";
import { ContextScopeChips } from "./ContextScopeChips";
import {
  AssistantTaskSurfaces,
  type ResearchProgressData,
} from "./AssistantTaskSurfaces";
import {
  RuleConfirmDialog,
  type RuleConfirmRequest,
} from "./RuleConfirmDialog";
import {
  ToolConfirmDialog,
  type ToolConfirmRequest,
} from "./ToolConfirmDialog";

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
  const [messages, setMessages] = useState<ChatLine[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const bubbleSelection = useAiBubbleSelection();
  const messagesRef = useRef(messages);
  messagesRef.current = messages;
  const sessionIdRef = useRef(sessionId);
  sessionIdRef.current = sessionId;
  const [packets, setPackets] = useState<ContextPacket[]>([]);
  const [selectedPacketIds, setSelectedPacketIds] = useState<string[]>([]);
  const [packetsOpen, setPacketsOpen] = useState(false);
  const [toolConfirmRequest, setToolConfirmRequest] =
    useState<ToolConfirmRequest | null>(null);
  const [ruleConfirmRequest, setRuleConfirmRequest] =
    useState<RuleConfirmRequest | null>(null);

  const [writingPatches, setWritingPatches] = useState<PatchProposal[]>([]);
  const [citationResult, setCitationResult] =
    useState<CitationCheckResult | null>(null);
  const [organizeSuggestions, setOrganizeSuggestions] = useState<
    OrganizeSuggestion[]
  >([]);
  const [organizeSelection, setOrganizeSelection] = useState<Set<string>>(
    new Set(),
  );
  const [researchResult, setResearchResult] =
    useState<ResearchFocusPayload | null>(null);
  const [researchProgress, setResearchProgress] =
    useState<ResearchProgressData | null>(null);
  const [researchRunning, setResearchRunning] = useState(false);
  const [researchPanelExpanded, setResearchPanelExpanded] = useState(false);
  const researchDetailRef = useRef<HTMLDivElement>(null);
  const [generatingResearchNote, setGeneratingResearchNote] = useState(false);
  const [docSummary, setDocSummary] = useState<string | null>(null);
  const [docIssues, setDocIssues] = useState<string[]>([]);
  const [contextStatusData, setContextStatusData] =
    useState<ContextStatus | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);
  const [activityHint, setActivityHint] = useState<string | null>(null);
  const [sessionTokenUsage, setSessionTokenUsage] = useState<TokenUsage | null>(
    null,
  );
  const streamBuf = useRef("");
  const requestIdRef = useRef<string | null>(null);
  const toolConfirmInFlightRef = useRef<Set<string>>(new Set());
  const toolConfirmSettledRef = useRef<Set<string>>(new Set());
  const [harnessRequestId, setHarnessRequestId] = useState<string | null>(null);
  const [auditDrawerOpen, setAuditDrawerOpen] = useState(false);
  const assistantRun = useAssistantRun("chat");
  const composerDisabled =
    streaming || assistantRun.isBusy || toolConfirmRequest !== null;
  const chromeActionsDisabled = streaming;

  const { handleCitationClick, citationMiss, clearCitationMiss } =
    useCitationClick(packets, () => setPacketsOpen(true), setSelectedPacketIds);

  useEffect(() => {
    onChromeChange?.(
      buildAssistantChromeSnapshot({
        sessionTokenUsage,
        activityHint,
        streaming,
        messages,
        harnessPhaseLabel: null,
        packets,
        harnessRequestId,
      }),
    );
  }, [
    activityHint,
    harnessRequestId,
    messages,
    onChromeChange,
    packets,
    sessionTokenUsage,
    streaming,
  ]);

  useEffect(() => {
    const openAudit = () => setAuditDrawerOpen(true);
    window.addEventListener(OPEN_AUDIT_TRAIL_EVENT, openAudit);
    return () => window.removeEventListener(OPEN_AUDIT_TRAIL_EVENT, openAudit);
  }, []);

  useEffect(() => {
    if (!streaming) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listenAiRequestStarted((payload) => {
      if (cancelled) return;
      requestIdRef.current = payload.request_id;
      setHarnessRequestId(payload.request_id);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [streaming]);
  const researchRequestIdRef = useRef<string | null>(null);
  const panelSendActiveRef = useRef(false);
  const forceNewSessionRef = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const messageListRef = useRef<HTMLDivElement>(null);
  const { profile: promptProfile } = usePromptProfile();
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

  useEffect(() => {
    if (!selectionQuote?.text) return;
    setActionState(buildActionState("writing", "idle"));
  }, [selectionQuote?.filePath, selectionQuote?.text]);

  useEffect(() => {
    if (!prefillMessage?.trim()) return;
    setInput(prefillMessage.trim());
  }, [prefillMessage]);

  useEffect(() => {
    syncActiveAiScene(actionState.intent);
  }, [actionState.intent]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    void listenToolConfirmRequest((req) => {
      if (req.tool_name === "update_user_rule") {
        const ruleType =
          typeof req.arguments.rule_type === "string"
            ? req.arguments.rule_type
            : "custom_rules";
        const ruleText =
          typeof req.arguments.rule === "string"
            ? req.arguments.rule
            : JSON.stringify(req.arguments);
        setRuleConfirmRequest({
          rule: ruleText,
          rule_type: ruleType,
          source: "ai_detected",
        });
      } else {
        setToolConfirmRequest(req);
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    const setupResearchListener = async () => {
      return listenResearchProgress((payload) => {
        setResearchProgress(payload);
        if (payload.state === "running") {
          setResearchRunning(true);
        }
        if (
          payload.state === "completed" ||
          payload.state === "failed" ||
          payload.state === "aborted"
        ) {
          setResearchRunning(false);
          setActionState((prev) => ({
            ...prev,
            status:
              payload.state === "completed"
                ? "completed"
                : payload.state === "aborted"
                  ? "completed"
                  : "error",
          }));
        }
      });
    };

    let unlisten: (() => void) | undefined;
    void setupResearchListener().then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, []);

  useAssistantLlmStream({
    panelSendActiveRef,
    requestIdRef,
    streamBufRef: streamBuf,
    setMessages,
    setStreaming,
  });

  const clearTaskSurfaces = useCallback(() => {
    setWritingPatches([]);
    setCitationResult(null);
    setOrganizeSuggestions([]);
    setOrganizeSelection(new Set());
    setResearchResult(null);
    setResearchProgress(null);
    setResearchRunning(false);
    setDocSummary(null);
    setDocIssues([]);
    setLastError(null);
  }, []);

  const handleNewChat = useCallback(() => {
    clearTaskSurfaces();
    clearCitationMiss();
    setPackets([]);
    setSelectedPacketIds([]);
    setMessages([]);
    setSessionId(null);
    setSessionTokenUsage(null);
    setInput("");
    setActivityHint(null);
    setStreaming(false);
    streamBuf.current = "";
    requestIdRef.current = null;
    setHarnessRequestId(null);
    forceNewSessionRef.current = true;
    setActionState(buildActionState("chat", "idle"));
  }, [clearCitationMiss, clearTaskSurfaces]);

  const handleRetract = useCallback(
    async (index: number) => {
      const target = messagesRef.current[index];
      if (!target) return;
      // Abort any active stream first
      if (streaming && requestIdRef.current) {
        try {
          await llmAbort(requestIdRef.current);
        } catch {
          /* ignore */
        }
        setStreaming(false);
      }
      // Backend retract by seq
      const sid = sessionIdRef.current;
      const seq = target.seq;
      if (sid && seq) {
        try {
          await sessionRetract(sid, seq);
        } catch (err) {
          console.warn("[retract] backend failed:", err);
        }
      }
      // Truncate frontend messages
      setMessages((prev) => prev.slice(0, index));
    },
    [streaming],
  );

  const handleInsertToEditor = useCallback(() => {
    if (!onInsertToEditor) return;
    const indices = Array.from(bubbleSelection.selected).sort((a, b) => a - b);
    const content = indices
      .map((i) => messagesRef.current[i])
      .filter((m): m is ChatLine => m != null)
      .map((m) => {
        if (m.role === "user") return `> ${m.content}`;
        return m.content;
      })
      .join("\n\n");
    if (content) {
      onInsertToEditor(content);
      bubbleSelection.clear();
    }
  }, [onInsertToEditor, bubbleSelection]);

  const handleCopySelected = useCallback(async () => {
    const indices = Array.from(bubbleSelection.selected).sort((a, b) => a - b);
    const content = indices
      .map((i) => messagesRef.current[i])
      .filter((m): m is ChatLine => m != null)
      .map((m) => m.content)
      .join("\n\n");
    if (content) {
      try {
        await navigator.clipboard.writeText(content);
      } catch {
        /* ignore */
      }
      bubbleSelection.clear();
    }
  }, [bubbleSelection]);

  const handleExportSelected = useCallback(() => {
    const indices = Array.from(bubbleSelection.selected).sort((a, b) => a - b);
    const lines = indices
      .map((i) => messagesRef.current[i])
      .filter((m): m is ChatLine => m != null)
      .map((m) => {
        if (m.role === "user") return `## 用户\n\n${m.content}`;
        if (m.role === "assistant") return `## 助手\n\n${m.content}`;
        return `## ${m.role}\n\n${m.content}`;
      });
    if (lines.length === 0) return;
    const md = lines.join("\n\n---\n\n");
    const blob = new Blob([md], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `iris-export-${new Date().toISOString().slice(0, 10)}.md`;
    a.click();
    URL.revokeObjectURL(url);
    bubbleSelection.clear();
  }, [bubbleSelection]);

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
    [contextScope, notePath, sessionId, webSearch],
  );

  const appendUserMessage = useCallback((rawMessage: string) => {
    const display = stripMentionTokensForDisplay(rawMessage);
    setMessages((prev) => [...prev, { role: "user", content: display }]);
  }, []);

  const ensureAssistantStreamSlot = useCallback(() => {
    setMessages((prev) => {
      const last = prev[prev.length - 1];
      if (last?.role === "assistant") return prev;
      return [...prev, { role: "assistant", content: "" }];
    });
  }, []);

  const appendAssistantSummary = useCallback(
    (intent: AssistantIntent, count?: number) => {
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          content: buildTaskSummary(intent, count),
        },
      ]);
    },
    [],
  );

  const executeKnowledgeChat = useCallback(
    async (
      rawMessage: string,
      intent: AssistantIntent,
      options?: { startNewSession?: boolean },
    ) => {
      setStreaming(true);
      streamBuf.current = "";
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
          const notice = refreshNotice;
          setMessages((prev) => [...prev, { role: "system", content: notice }]);
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
          streamBuf.current,
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
        streamBuf.current = "";
      }
    },
    [
      assistantRun,
      contextScope,
      ensureAssistantStreamSlot,
      getNoteContent,
      notePath,
      packets,
      selectedPacketIds,
      sessionId,
      webSearch,
    ],
  );

  const handleHarnessResume = useCallback(async () => {
    if (!harnessRequestId) return;
    setLastError(null);
    setStreaming(true);
    setActivityHint("正在从 checkpoint 恢复 Agent…");
    ensureAssistantStreamSlot();
    try {
      const raw = await harnessResume(harnessRequestId);
      const result = raw as {
        content?: string;
        tool_calls?: Parameters<typeof mapChatToolCallsForUi>[0];
        tool_results?: Parameters<typeof mapChatToolCallsForUi>[1];
        evidence_packets?: ContextPacket[];
        usage?: TokenUsage;
      };
      const toolCalls = mapChatToolCallsForUi(
        result.tool_calls,
        result.tool_results,
      );
      const content = result.content?.trim() ?? "";
      if (result.evidence_packets?.length) {
        setPackets((prev) =>
          mergeContextPackets(prev, result.evidence_packets ?? []),
        );
      }
      if (result.usage) {
        setSessionTokenUsage((prev) =>
          accumulateTokenUsage(prev, result.usage!),
        );
      }
      setMessages((prev) => {
        const next = [...prev];
        const last = next[next.length - 1];
        if (last?.role === "assistant") {
          next[next.length - 1] = { ...last, content, toolCalls };
        } else {
          next.push({ role: "assistant", content, toolCalls });
        }
        return next;
      });
    } catch (error) {
      setLastError(invokeErrorMessage(error));
    } finally {
      setStreaming(false);
      setActivityHint(null);
    }
  }, [harnessRequestId, ensureAssistantStreamSlot]);

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
    [assembleContextForChat, clearTaskSurfaces, executeKnowledgeChat],
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
      getWritingContext,
      getNoteContent,
      notePath,
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
    [assistantRun, clearTaskSurfaces, webSearch],
  );

  const handleExpandResearchDetail = useCallback(
    (_result: ResearchFocusPayload) => {
      setResearchPanelExpanded(true);
      requestAnimationFrame(() => {
        researchDetailRef.current?.scrollIntoView({
          behavior: "smooth",
          block: "nearest",
        });
      });
    },
    [],
  );

  const abortResearch = useCallback(async () => {
    const id = researchRequestIdRef.current;
    if (!id) return;
    try {
      await researchAbort(id);
      setResearchRunning(false);
      setResearchProgress((prev) =>
        prev ? { ...prev, state: "aborted" } : null,
      );
      setActionState(buildActionState("research", "completed"));
    } catch (error) {
      setLastError(invokeErrorMessage(error));
    }
  }, []);

  const handleGenerateResearchNote = useCallback(async () => {
    if (!researchResult) return;
    setGeneratingResearchNote(true);
    try {
      const note = await researchGenerateNote({
        topic: researchResult.topic,
        summary: researchResult.summary,
        evidence_count: researchResult.evidence_matrix.total_evidence_count,
        coverage_score: researchResult.evidence_matrix.coverage_score,
      });
      setMessages((prev) => [
        ...prev,
        {
          role: "system",
          content: `研究笔记建议路径：${note.suggested_path}`,
        },
      ]);
    } catch (error) {
      setLastError(invokeErrorMessage(error));
    } finally {
      setGeneratingResearchNote(false);
    }
  }, [researchResult]);

  const send = useCallback(async () => {
    if (!input.trim() || composerDisabled) return;
    const rawMessage = input.trim();
    const intent = resolveAssistantIntent({
      message: rawMessage,
      hasSelection: Boolean(
        getWritingContext()?.selection || selectionQuote?.text,
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
    contextScope.pathPrefixes.length,
    contextScope.paths.length,
    getWritingContext,
    input,
    composerDisabled,
    messages,
    notePath,
    runChapter,
    runCitation,
    runDocumentCheck,
    runKnowledgeChat,
    runOrganize,
    runResearch,
    runWriting,
    selectionQuote?.text,
  ]);

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

  const handleQuoteToInput = useCallback((text: string) => {
    const quoted = text
      .split("\n")
      .map((line) => `> ${line}`)
      .join("\n");
    setInput((prev) =>
      prev.trim() ? `${prev.trim()}\n\n${quoted}\n\n` : `${quoted}\n\n`,
    );
    textareaRef.current?.focus();
  }, []);

  const handleToolConfirm = useCallback(
    async (
      requestId: string,
      toolCallId: string,
      decision: "approve" | "reject" | "modify",
      modifiedArgs?: unknown,
    ) => {
      const confirmKey = `${requestId}:${toolCallId}`;
      if (
        toolConfirmInFlightRef.current.has(confirmKey) ||
        toolConfirmSettledRef.current.has(confirmKey)
      ) {
        return;
      }
      toolConfirmInFlightRef.current.add(confirmKey);
      const intent = actionState.intent;
      const pendingConfirm = toolConfirmRequest;
      setToolConfirmRequest(null);
      setStreaming(true);
      setActivityHint(
        decision === "reject" ? "已拒绝，正在生成替代回答…" : "继续执行中…",
      );
      ensureAssistantStreamSlot();
      let nextTaskStatus: AssistantTaskStatus = "completed";
      try {
        const raw = await toolConfirmIpc({
          request_id: requestId,
          tool_call_id: toolCallId,
          decision,
          modified_args: modifiedArgs,
        });
        const result = raw as {
          resumed?: boolean;
          content?: string;
          tool_calls?: Parameters<typeof mapChatToolCallsForUi>[0];
          tool_results?: Parameters<typeof mapChatToolCallsForUi>[1];
          evidence_packets?: ContextPacket[];
          usage?: TokenUsage;
          pending_confirmation?: boolean;
          status?: string;
          installed_skill?: string;
        };
        if (!result.resumed) {
          nextTaskStatus = "completed";
          setActionState(buildActionState(intent, nextTaskStatus));
          return;
        }
        const toolCalls = mapChatToolCallsForUi(
          result.tool_calls,
          result.tool_results,
        );
        const content = result.content?.trim() ?? "";
        if (result.evidence_packets?.length) {
          setPackets((prev) =>
            mergeContextPackets(prev, result.evidence_packets ?? []),
          );
        }
        if (result.usage) {
          setSessionTokenUsage((prev) =>
            accumulateTokenUsage(prev, result.usage!),
          );
        }
        setMessages((prev) => {
          const next = [...prev];
          const last = next[next.length - 1];
          if (last?.role === "assistant") {
            next[next.length - 1] = {
              ...last,
              content,
              toolCalls,
            };
          } else {
            next.push({ role: "assistant", content, toolCalls });
          }
          if (
            (decision === "approve" || decision === "modify") &&
            pendingConfirm?.tool_name === "skills_install"
          ) {
            const notice = skillInstallSuccessNotice({
              installedSkill: result.installed_skill,
              preview: pendingConfirm.preview,
              arguments: pendingConfirm.arguments,
            });
            if (notice) {
              next.push({ role: "system", content: notice });
            }
          }
          return next;
        });
        const stillPending =
          result.pending_confirmation === true ||
          result.status === "pending_tools" ||
          (toolCalls?.some((t) => t.status === "pending") ?? false);
        nextTaskStatus = stillPending ? "awaiting_confirmation" : "completed";
        setActionState(buildActionState(intent, nextTaskStatus));
        if (!stillPending) {
          setHarnessRequestId(null);
          requestIdRef.current = null;
        }
      } catch (error) {
        const message = invokeErrorMessage(error);
        nextTaskStatus = "error";
        setMessages((prev) => [
          ...prev,
          { role: "system", content: `工具确认失败: ${message}` },
        ]);
        setActionState(buildActionState(intent, nextTaskStatus, message));
      } finally {
        toolConfirmInFlightRef.current.delete(confirmKey);
        toolConfirmSettledRef.current.add(confirmKey);
        setStreaming(false);
        setActivityHint(null);
        assistantRun.setFromTaskStatus(nextTaskStatus, intent);
      }
    },
    [
      actionState.intent,
      assistantRun,
      ensureAssistantStreamSlot,
      toolConfirmRequest,
    ],
  );

  const dismissToolConfirm = useCallback(() => {
    const req = toolConfirmRequest;
    if (req) {
      void handleToolConfirm(req.request_id, req.tool_call_id, "reject");
      return;
    }
    setToolConfirmRequest(null);
    assistantRun.setFromTaskStatus("completed", actionState.intent);
  }, [actionState.intent, assistantRun, handleToolConfirm, toolConfirmRequest]);

  const handleAcceptPatch = useCallback(
    async (patch: PatchProposal) => {
      try {
        const result = await patchApply(patch);
        if (!result.success) {
          throw new Error(result.error ?? "补丁应用失败");
        }
        const noteContent = getNoteContent();
        const before = noteContent.slice(0, patch.range.start);
        const after = noteContent.slice(patch.range.end);
        const next = before + patch.replacement_text + after;
        onPatchApplied?.(next);
        setWritingPatches((prev) =>
          prev.filter((item) => item.id !== patch.id),
        );
      } catch (error) {
        setLastError(invokeErrorMessage(error));
      }
    },
    [getNoteContent, onPatchApplied],
  );

  const handleAcceptOrganize = useCallback(async () => {
    const selected = organizeSuggestions.filter((item) =>
      organizeSelection.has(item.id),
    );
    if (selected.length === 0) return;
    try {
      const result = await organizeApply(selected);
      setOrganizeSuggestions((prev) =>
        prev.filter((item) => !result.applied.includes(item.id)),
      );
      setOrganizeSelection(new Set());
      onVaultRefresh?.();
    } catch (error) {
      setLastError(invokeErrorMessage(error));
    }
  }, [onVaultRefresh, organizeSelection, organizeSuggestions]);

  const activeScene: AiScene = resolveAiSceneForIntent(actionState.intent);

  const handleLoadSession = useCallback(
    (id: number, loaded: ChatLine[]) => {
      setSessionId(id);
      setMessages(loaded);
      forceNewSessionRef.current = false;
      clearTaskSurfaces();
      clearCitationMiss();
      setActionState(buildActionState(actionState.intent, "idle"));
    },
    [actionState.intent, clearCitationMiss, clearTaskSurfaces],
  );

  return (
    <div
      className="ai-sidecar flex h-full flex-col bg-ai-workspace"
      data-testid="unified-assistant-panel"
    >
      <header className="ai-sidecar-header shrink-0 border-b border-border/60 px-3 py-1.5">
        <div className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 flex-1 items-center">
            <AssistantPersonaDisplay profile={promptProfile} />
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            <AgentStatusBadge
              webSearchEnabled={webSearch}
              scene={activeScene}
              disabled={chromeActionsDisabled}
              auditAvailable={Boolean(harnessRequestId)}
              onOpenAudit={() => setAuditDrawerOpen(true)}
            />
            <SessionHistoryDropdown
              scene={activeScene}
              notePath={notePath}
              currentSessionId={sessionId}
              disabled={chromeActionsDisabled}
              onSelectSession={handleLoadSession}
              onDeleted={(id) => {
                if (sessionId === id) {
                  handleNewChat();
                }
              }}
            />
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-8 gap-1 px-2 text-xs"
              title="新对话（不加载本笔记下的历史会话）"
              onClick={handleNewChat}
              disabled={chromeActionsDisabled}
            >
              <MessageSquarePlus className="h-3.5 w-3.5" />
              新对话
            </Button>
          </div>
        </div>
      </header>

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
        onClearOrganizeSelection={() => setOrganizeSelection(new Set())}
        onToggleOrganizeSuggestion={(id) =>
          setOrganizeSelection((prev) => {
            const next = new Set(prev);
            if (next.has(id)) next.delete(id);
            else next.add(id);
            return next;
          })
        }
        onAcceptOrganize={() => void handleAcceptOrganize()}
        evidenceRefreshNotice={assistantRun.evidenceRefreshNotice}
        writingPatches={writingPatches}
        onAcceptPatch={(item) => void handleAcceptPatch(item)}
        onRejectPatch={(item) =>
          setWritingPatches((prev) =>
            prev.filter((patchItem) => patchItem.id !== item.id),
          )
        }
        onCopyPatch={(item) =>
          void navigator.clipboard.writeText(item.replacement_text)
        }
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
        onConfirm={async (request) => {
          const key =
            request.rule_type && request.rule_type !== "custom_rules"
              ? request.rule_type
              : "custom_rules";
          await profileSetRule({
            key,
            description: request.rule,
            source: request.source,
          });
          setRuleConfirmRequest(null);
        }}
        onReject={() => setRuleConfirmRequest(null)}
        onClose={() => setRuleConfirmRequest(null)}
      />
      <AuditTrailDrawer
        open={auditDrawerOpen}
        onOpenChange={setAuditDrawerOpen}
        requestId={harnessRequestId}
      />
    </div>
  );
}
